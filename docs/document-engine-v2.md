# Document Engine v2 — 无 Python 依赖的文档智能处理引擎

> 基于 `OCR.md` 全新方案，替代旧版 Python Worker 架构。
> 本文档为开发计划与架构设计，覆盖 Phase 0-10 全部阶段。
> **实施状态**：本文是设计基线；已完成实现、门禁和限制以
> [`document-engine-development-report.md`](document-engine-development-report.md) 为准。

## 1. 项目概述

**目标**：在 CrucibleBox 工具箱中构建一个完整的 Document Engine 插件——

> 离线、本地、无 Python 依赖的文档智能处理引擎

最终用户安装 CrucibleBox 后，**无需安装** Python / pip / Conda / CUDA Toolkit / Paddle 环境，
即可使用 OCR、文档解析、格式转换、文档切分功能。

**功能范围**：
- OCR（图片 / PDF / 批量，中英文 + 混合）
- PDF 解析（文本层直接提取 / 扫描件 OCR）
- 文档解析（PDF / DOCX / PPTX / XLSX / TXT / Markdown / HTML）
- 文档切分（Structure / Semantic / Hybrid，用于 RAG / Embedding / 知识库）
- 格式转换（PDF ↔ DOCX ↔ Markdown ↔ TXT ↔ HTML）
- 批量处理 + 任务队列（暂停/恢复/取消/重试/断点续处理）
- 缓存系统（避免重复处理）
- 模型管理（查看/下载/删除/更新）

## 2. 与旧方案对比

| 维度 | 旧方案（Phase 0-3，已废弃） | 新方案（本文档） |
|---|---|---|
| OCR 运行时 | Python PaddleOCR（子进程） | PaddleOCR ONNX + Rust `ort` crate |
| Worker 语言 | Python Worker | Rust Worker（`ocr-worker.exe`） |
| 格式转换 | Python 库（未实现） | LibreOffice Headless + Pandoc（sidecar） |
| PDF 解析 | MinerU（Python，未实现） | PDFium/MuPDF（Rust）+ OCR fallback |
| 用户依赖 | 需安装 Python 3.10-3.12 | **零依赖** |
| 模型管理 | pip install | Model Manager（下载 ONNX 模型文件） |
| GPU 支持 | CUDA Toolkit（需用户安装） | ONNX Runtime CUDA EP（随包分发） |

## 3. 总体架构

```
CrucibleBox (Tauri 2 + Rust)
│
├── Document Engine Plugin (trusted service)
│   ├── renderer.tsx          # React UI（8 个面板）
│   └── main.ts               # backend entry → invokeTrustedService
│
├── document_engine_service   # Rust trusted service（请求校验 + 任务调度）
│   ├── TaskManager           # 统一任务队列（多 resource key）
│   ├── FileAnalyzer          # 文件分析 + 自动路由（已实现，复用）
│   ├── CacheManager          # 缓存系统
│   └── ModelManager          # 模型管理
│
├── ocr-worker.exe            # 独立 Rust 可执行文件
│   ├── ort (ONNX Runtime)
│   ├── PaddleOCR Detection ONNX
│   ├── PaddleOCR Recognition ONNX
│   └── stdin/stdout JSON 行协议
│
├── LibreOffice Headless      # 可选 sidecar（DOCX/PPTX/XLSX ↔ PDF）
│   └── soffice --headless
│
└── Pandoc                    # 可选 sidecar（Markdown ↔ HTML/DOCX）
```

**通信链**：
```
renderer.tsx                          main.ts (sidecar quickjs-ng)
    │ sendToBackend(msg)                   │ onMessage(msg)
    └──────── MessagePort ──────────────► │
                                    ctx.api.invokeTrustedService('document-engine', ...)
                                                │
                                                ▼
                                    envelope_host::host_dispatch
                                                │
                                                ▼
                                document_engine_service::dispatch (Rust)
                                                │
                                    ┌───────────┼───────────┐
                                    ▼           ▼           ▼
                              ocr-worker   LibreOffice   Pandoc
                              (spawn)      (spawn)       (spawn)
```

## 4. 技术栈

### 新增 Rust 依赖

| Crate | 版本 | 用途 |
|---|---|---|
| `ort` | 2.x | ONNX Runtime Rust 绑定（OCR 推理） |
| `image` | 0.25 | 图片预处理（缩放/灰度/二值化） |
| `lopdf` | 0.34 | PDF 文本提取（文本 PDF 直接提取） |
| `pdfium-render` | 0.8 | PDF 页面渲染为位图（扫描 PDF → OCR） |
| `docx-rs` | 0.5 | DOCX 解析（XML 结构提取） |
| `calamine` | 0.26 | XLSX 解析 |
| `comrak` | 0.34 | Markdown 解析/生成 |
| `html5ever` | 0.27 | HTML 解析 |

### 外部 Sidecar

| 工具 | 用途 | 分发方式 |
|---|---|---|
| `ocr-worker.exe` | OCR 推理（Rust 编译） | 随包分发（Tauri externalBin） |
| LibreOffice | DOCX/PPTX/XLSX ↔ PDF 转换 | 检测系统安装，未安装则功能降级 |
| Pandoc | Markdown ↔ HTML/DOCX 转换 | 检测系统安装或随包分发（~30MB 单文件） |

## 5. OCR 方案

### 5.1 核心选型

**PaddleOCR ONNX**（不使用 Python PaddleOCR Runtime / PyTorch / TensorFlow）

```
Rust
 │
 onnxruntime (ort crate)
 │
 PaddleOCR ONNX Model
```

### 5.2 模型清单

```
models/
├── detection/
│   ├── ch_PP-OCRv4_det_infer.onnx    # 中文文本检测
│   └── en_PP-OCRv3_det_infer.onnx    # 英文文本检测
├── recognition/
│   ├── ch_PP-OCRv4_rec_infer.onnx    # 中英文识别
│   └── en_PP-OCRv4_rec_infer.onnx    # 英文识别
├── dictionary/
│   ├── ppocr_keys_v1.txt             # 中文字典
│   └── en_dict.txt                   # 英文字典
└── layout/
    └── picodet_lcnet_x2_5_layout.onnx # 版面分析（可选）
```

### 5.3 推理流程

```
输入图片
  │
  ▼
图片预处理（image crate）
  │  缩放 / 归一化 / 通道调整
  ▼
文本检测（Detection ONNX）
  │  输出：文本区域 bbox 列表
  ▼
按 bbox 裁剪文本行
  │
  ▼
文本识别（Recognition ONNX）
  │  输出：每行文本 + 置信度
  ▼
后处理
  │  NMS / 字典解码 / 坐标还原
  ▼
输出 JSON
```

### 5.4 GPU 支持

```rust
use ort::execution_providers::CUDAExecutionProvider;

let session = Session::builder()?
    .with_execution_providers([
        CUDAExecutionProvider::default().build()
    ])?
    .commit_from_file("models/detection/model.onnx")?;
```

- `auto`：检测 NVIDIA GPU + CUDA → GPU，否则 CPU
- 默认 OCR Worker = 1（避免 VRAM OOM）
- 不允许多个大模型同时加载

## 6. OCR Worker 设计

### 6.1 独立 Rust Crate

`ocr-worker` 编译为独立可执行文件 `ocr-worker.exe`，作为 Tauri `externalBin` 分发。

### 6.2 通信协议

stdin/stdout JSON 行协议（`\n` 分隔）：

**请求帧（Rust 主进程 → ocr-worker）**：
```json
{
  "type": "ocr",
  "requestId": "uuid",
  "input": "C:\\path\\to\\image.png",
  "options": {
    "language": "zh",
    "device": "auto",
    "modelDirectory": "C:\\models"
  }
}
```

**进度帧（ocr-worker → Rust 主进程）**：
```json
{
  "type": "progress",
  "requestId": "uuid",
  "stage": "detect",
  "percent": 42,
  "message": "检测文本区域 12/28"
}
```

**结果帧**：
```json
{
  "type": "result",
  "requestId": "uuid",
  "text": "完整识别文本",
  "blocks": [
    {
      "text": "第一行文本",
      "bbox": [10, 20, 200, 45],
      "confidence": 0.98
    }
  ]
}
```

**错误帧**：
```json
{
  "type": "error",
  "requestId": "uuid",
  "code": "ocr-failed",
  "message": "模型加载失败"
}
```

**取消帧**：
```json
{ "type": "cancel", "requestId": "uuid" }
```

### 6.3 Worker 生命周期

| 事件 | 动作 |
|---|---|
| spawn | 启动 ocr-worker.exe，注入模型路径 |
| idle | 无任务，保持进程（warm start） |
| busy | 处理中，禁止并发请求 |
| crash | 退出码非零 → 重启 Worker，任务标记 failed |
| OOM | CUDA OOM → 降级 CPU，重启 Worker |

## 7. PDF 处理

### 7.1 智能路由（复用 `document_analyzer.rs`）

```
输入 PDF
  │
  ▼
File Analyzer（已实现）
  │  检测：文本层 / 页数 / 图片数
  ▼
┌──────────────────────────────┐
│ 有文本层？                    │
│   是 → lopdf 直接提取文本     │
│   否 → pdfium-render 渲染    │
│        → 位图 → OCR Worker   │
│ 混合 → 逐页判断              │
└──────────────────────────────┘
```

### 7.2 文本 PDF（直接提取）

```rust
use lopdf::Document;

let doc = Document::load("input.pdf")?;
for page_num in 1..=doc.get_pages().len() {
    // 提取文本、字体、图片元数据
}
```

### 7.3 扫描 PDF（渲染 → OCR）

```rust
use pdfium_render::prelude::*;

let pdfium = Pdfium::new(...);
let document = pdfium.load_pdf_from_file("scan.pdf")?;
for page in document.pages() {
    let bitmap = page.render(..).as_image();
    // bitmap → image crate → OCR Worker
}
```

## 8. 文档解析

### 8.1 统一 Document JSON

复用已有数据模型（`docs/document-engine-data-model.md`）：

```
Document
├── Metadata (title, author, pageCount, hasTextLayer, ...)
├── Pages[]
│   └── Blocks[]
│       ├── text / heading / paragraph
│       ├── table / image / formula
│       └── code / list / quote
└── Structure (outline, readingOrder)
```

### 8.2 各格式解析器

| 格式 | 解析方式 | Rust crate |
|---|---|---|
| PDF（文本） | 直接提取 | `lopdf` |
| PDF（扫描） | 渲染 → OCR | `pdfium-render` + OCR Worker |
| DOCX | XML 解析 | `docx-rs` 或 `zip` + XML |
| PPTX | XML 解析 | `zip` + XML |
| XLSX | 表格解析 | `calamine` |
| TXT | 直接读取 | `std::fs` |
| Markdown | AST 解析 | `comrak` |
| HTML | DOM 解析 | `html5ever` |

## 9. 文档切分

### 9.1 三种策略

| 策略 | 说明 | 默认 |
|---|---|---|
| Structure | 按标题/章节/段落结构切分 | |
| Semantic | 语义边界 + token 限制 | |
| Hybrid | 结构优先，超长块再按语义拆分 | ✅ |

### 9.2 可配置参数

```json
{
  "strategy": "hybrid",
  "targetTokens": 512,
  "maxTokens": 1024,
  "overlap": 50,
  "minChunkSize": 100
}
```

### 9.3 数学文档保护

- 定理 + 证明不拆分（同一 chunk）
- 公式保留 LaTeX
- 定义独立成块
- 代码块完整保留

### 9.4 Chunk 输出结构

```json
{
  "id": "c1",
  "document_id": "d1",
  "title": "第三章",
  "chapter": "第三章 线性代数",
  "section": "3.1 矩阵",
  "content": "矩阵是...",
  "page_start": 1,
  "page_end": 2,
  "type": "text",
  "token_count": 500,
  "character_count": 1500,
  "block_ids": ["b1", "b2", "b3"]
}
```

## 10. 格式转换

### 10.1 统一中间格式

```
源文件 → Document Parser → Unified Document Model → Converter → 目标格式
```

不实现直接转换（如 PDF 直接转 TXT），所有转换经过 Document Model 中间层。

### 10.2 转换矩阵

| 源 \ 目标 | Markdown | DOCX | TXT | HTML | JSON | PDF |
|---|---|---|---|---|---|---|
| PDF（文本） | ✅ | ✅ | ✅ | - | ✅ | - |
| PDF（扫描） | ✅ | ⚠️ | ✅ | - | ✅ | - |
| DOCX | ✅ | - | ✅ | - | ✅ | ✅ |
| Markdown | - | ✅ | ✅ | ✅ | ✅ | ✅ |
| HTML | ✅ | - | ✅ | - | ✅ | - |
| TXT | ✅ | - | - | - | ✅ | - |

✅ = 稳定 / ⚠️ = 实验性

### 10.3 转换后端

| 转换 | 后端 | 方式 |
|---|---|---|
| DOCX → PDF | LibreOffice Headless | `soffice --headless --convert-to pdf` |
| PPTX → PDF | LibreOffice Headless | 同上 |
| XLSX → PDF | LibreOffice Headless | 同上 |
| PDF → DOCX | LibreOffice Headless | 同上 |
| Markdown → HTML | Pandoc | `pandoc input.md -o output.html` |
| HTML → Markdown | Pandoc | `pandoc input.html -o output.md` |
| Markdown → PDF | Pandoc + LaTeX | `pandoc input.md -o output.pdf` |
| PDF → Markdown | 自研 | Document Model → Markdown |
| DOCX → Markdown | 自研 | DOCX XML → Markdown |

### 10.4 Sidecar 检测

```rust
fn detect_libreoffice() -> Option<String> {
    // 检测系统 PATH 上的 soffice
    // 返回路径或 None
}

fn detect_pandoc() -> Option<String> {
    // 检测系统 PATH 上的 pandoc
    // 或检查 bundled pandoc
    // 返回路径或 None
}
```

未安装时对应功能降级（UI 提示安装指引）。

## 11. 任务系统

### 11.1 状态机

```
queued → running → completed
                → failed → (retry) → queued
                → paused → (resume) → running
         → cancelled
```

### 11.2 任务类型

| 类型 | resource key | 说明 |
|---|---|---|
| ocr | `ocr` | 图片/PDF OCR |
| parse | `parse` | 文档解析 |
| convert | `convert` | 格式转换 |
| chunk | `chunk` | 文档切分 |
| batch | `batch` | 批量处理 |

### 11.3 功能

- 进度 + ETA + 速度（pages/sec）
- 暂停/恢复（checkpoint 机制）
- 取消（向 Worker 发送 cancel 帧）
- 重试（失败任务重新入队）
- 断点续处理（长任务分页 checkpoint）

### 11.4 错误隔离

批处理中单文件失败不中断整体：

```
1000 文件 → 998 成功 / 2 失败
最终：{ "completed": 998, "failed": 2, "failedFiles": [...] }
```

## 12. 缓存系统

### 12.1 Cache Key

```
key = sha256(source_hash + engine_version + model_version + options)
```

### 12.2 存储

SQLite 表 `document_engine_cache`：

```sql
CREATE TABLE document_engine_cache (
    cache_key TEXT PRIMARY KEY,
    result_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    access_count INTEGER DEFAULT 1
);
```

### 12.3 策略

- 命中时直接返回缓存结果（< 10ms）
- 缓存不过期（手动清理或按 LRU）
- 模型更新后旧缓存自动失效（model_version 变化）

## 13. GPU 支持

### 13.1 设备选择

```json
{ "device": "auto" | "cpu" | "gpu" }
```

- `auto`：检测 NVIDIA GPU + CUDA → GPU，否则 CPU
- 默认 OCR Worker = 1（避免 VRAM OOM）

### 13.2 检测

```rust
fn detect_gpu() -> serde_json::Value {
    // nvidia-smi -L → GPU 型号
    // 检查 CUDA 可用性
    json!({ "available": true, "device": "NVIDIA GeForce RTX 5070" })
}
```

### 13.3 VRAM 管理

- 不同时加载多个大模型
- CUDA OOM → 降级 CPU + 重启 Worker
- 模型按需加载/卸载

## 14. 模型管理

### 14.1 Model Manager

```
models/
├── detection/
│   └── ch_PP-OCRv4_det_infer.onnx
├── recognition/
│   └── ch_PP-OCRv4_rec_infer.onnx
├── dictionary/
│   └── ppocr_keys_v1.txt
└── layout/
    └── picodet_lcnet_x2_5_layout.onnx
```

### 14.2 功能

- 查看已安装模型（名称/大小/版本）
- 下载模型（从 GitHub Release / CDN）
- 删除模型
- 更新模型
- 模型路径通过配置管理（不硬编码）

### 14.3 配置

```json
{
  "modelDirectory": "%APPDATA%\\cruciblebox\\document-engine\\models",
  "device": "auto"
}
```

### 14.4 分发策略

- 模型**不随包分发**（避免安装包过大）
- 首次使用时通过 Model Manager 下载
- 下载进度通过 TaskManager 管理

## 15. Tauri 集成

### 15.1 trusted service 模式

复用现有 `invokeTrustedService` 机制（与 UniEnv 对齐）：

```
plugin.json: "permissions": ["trusted:document-engine"]
main.ts: ctx.api.invokeTrustedService('document-engine', 'message', payload)
Rust: document_engine_service::dispatch()
```

### 15.2 Worker 管理

OCR Worker / LibreOffice / Pandoc 均由 Rust trusted service 管理：
- spawn 子进程
- stdin/stdout JSON 行协议
- 进程生命周期（idle/busy/crash/restart）
- 不暴露给插件 frontend

## 16. UI 设计

### 16.1 面板结构

| 面板 | 功能 |
|---|---|
| **首页** | 拖入文件 + 快捷操作（OCR/转换/解析/切分/批量） |
| **OCR** | 选择文件 → 语言 → 设备 → 开始 → 结果查看 |
| **转换** | 源格式 → 目标格式 → 转换 → 预览 |
| **切分** | 选择策略 → 参数 → 切分 → Chunk 查看器 |
| **批量** | 目录选择 → 规则 → 批量处理 → 进度 |
| **任务** | 实时进度 + 暂停/恢复/取消/重试 |
| **模型** | 模型列表 + 下载/删除/更新 |
| **结果** | Markdown 预览 + JSON 查看 + 导出 |

### 16.2 引擎状态面板

```
[Python: N/A]  [CUDA: ✓ RTX 5070]  [OCR Worker: ✓ running]
[PaddleOCR Det: ✓ v4]  [PaddleOCR Rec: ✓ v4]  [LibreOffice: ✗ 未安装]
```

## 17. 代码复用清单

### 17.1 直接复用

| 文件 | 说明 |
|---|---|
| `document_analyzer.rs` | 文件分析 + 自动路由（Phase 3 已完成，100% 复用） |
| `plugins/document-engine/src/main.ts` | trusted service 调用方式不变 |
| `plugins/document-engine/src/renderer-entry.tsx` | 插件入口契约不变 |
| `docs/document-engine-data-model.md` | Document/Page/Block/Chunk 模型不变 |

### 17.2 重构

| 文件 | 改动 |
|---|---|
| `document_engine_service.rs` | 移除 `detect_python/detect_module/run_python`，改为 `detect_onnx_runtime()` + `detect_models()` + `detect_libreoffice()` + `detect_pandoc()` |
| `document_engine_task.rs` | 扩展 TaskManager，加 `pause()` / `resume()` / `checkpoint()` |
| `plugins/document-engine/src/renderer.tsx` | UI 骨架保留，填充实际功能面板 |
| `plugin.json` | 移除 `pythonPath`，新增 `onnxRuntime` 配置 |

### 17.3 删除

| 代码 | 原因 |
|---|---|
| `detect_python()` | 不再需要 Python |
| `detect_module()` | 不再需要 pip 模块检测 |
| `run_python()` | 不再调用 Python |
| `pythonPath` 配置 | 不再需要 |
| `document-engine-worker-protocol.md` | Python Worker 协议废弃 |

### 17.4 新建

| 文件 | 说明 |
|---|---|
| `ocr-worker/` (独立 crate) | Rust OCR Worker，编译为 `ocr-worker.exe` |
| `src-tauri/src/ocr_worker.rs` | Worker Manager（spawn/通信/生命周期） |
| `src-tauri/src/pdf_parser.rs` | PDF 解析（lopdf + pdfium-render） |
| `src-tauri/src/docx_parser.rs` | DOCX 解析 |
| `src-tauri/src/chunker.rs` | 文档切分（Structure/Semantic/Hybrid） |
| `src-tauri/src/converter.rs` | 格式转换（LibreOffice + Pandoc + 自研） |
| `src-tauri/src/cache_manager.rs` | 缓存系统 |
| `src-tauri/src/model_manager.rs` | 模型管理（下载/安装/删除） |

## 18. 新增 Rust 依赖

| Crate | 版本 | 用途 |
|---|---|---|
| `ort` | 2.x | ONNX Runtime（OCR 推理） |
| `image` | 0.25 | 图片预处理 |
| `lopdf` | 0.34 | PDF 文本提取 |
| `pdfium-render` | 0.8 | PDF 页面渲染 |
| `docx-rs` | 0.5 | DOCX 解析 |
| `calamine` | 0.26 | XLSX 解析 |
| `comrak` | 0.34 | Markdown 解析/生成 |
| `html5ever` | 0.27 | HTML 解析 |

## 19. 外部 Sidecar

| 工具 | 用途 | 分发方式 | 检测方式 |
|---|---|---|---|
| `ocr-worker.exe` | OCR 推理 | 随包分发（Tauri externalBin） | 始终可用 |
| LibreOffice | DOCX/PPTX/XLSX ↔ PDF | 用户自行安装 | `where soffice` |
| Pandoc | Markdown ↔ HTML/DOCX | 随包分发或用户安装 | `where pandoc` |

## 20. 开发阶段

### Phase 0 — 架构重构

**目标**：清理旧 Python 方案代码，建立新架构骨架

**交付物**：
- 重构 `document_engine_service.rs`（移除 Python 检测）
- 更新 `plugin.json`（移除 pythonPath）
- 新建 `docs/document-engine-v2.md`（本文档）
- 重写 Worker 协议文档

**验证**：`cargo test` 通过，`getStatus` 返回 ONNX Runtime 状态

---

### Phase 1 — ONNX Runtime + 模型准备

**目标**：Rust 中加载 ONNX Runtime，准备 PaddleOCR ONNX 模型

**交付物**：
- `Cargo.toml` 添加 `ort` + `image`
- 模型转换脚本（Python 仅用于开发阶段：PaddlePaddle → ONNX）
- 模型文件放到 `E:\OCR\Models\`
- ONNX 模型加载 POC

**验证**：能加载 ONNX 模型并执行推理（输出 tensor shape 正确）

---

### Phase 2 — Rust OCR Worker

**目标**：开发独立 `ocr-worker.exe`

**交付物**：
- `ocr-worker/` 独立 Rust crate
- 图片预处理 + 文本检测 + 文本识别 + 后处理
- stdin/stdout JSON 行协议
- GPU/CPU auto 检测

**验证**：10 张测试图片 OCR 结果正确，置信度 > 0.9

---

### Phase 3 — Document Engine 接口整合

**目标**：Rust trusted service 调用 OCR Worker

**交付物**：
- `ocr_worker.rs`：Worker Manager
- `document_engine_service.rs`：`document.ocr` 操作
- 进度上报链路（Worker → TaskManager → 前端）

**验证**：通过插件 UI 触发 OCR → 看到真实结果

---

### Phase 4 — PDF 解析

**目标**：PDF 智能解析（文本 PDF + 扫描 PDF）

**交付物**：
- `pdf_parser.rs`：lopdf 文本提取 + pdfium-render 渲染 → OCR
- 智能路由（复用 FileAnalyzer）

**验证**：100 页扫描 PDF 全部识别，文本 PDF 提取正确

---

### Phase 5 — Converter（格式转换）

**目标**：文档格式转换系统

**交付物**：
- `converter.rs`：LibreOffice + Pandoc + 自研转换
- 转换矩阵实现

**验证**：转换矩阵中所有路径至少一个方向通过

---

### Phase 6 — Chunker（文档切分）

**目标**：智能文档切分

**交付物**：
- `chunker.rs`：Structure / Semantic / Hybrid 策略
- 数学文档保护

**验证**：数学教材切分后定理+证明在同一 chunk

---

### Phase 7 — Job Manager（任务管理）

**目标**：统一任务队列，完整生命周期

**交付物**：
- 扩展 `document_engine_task.rs`：pause/resume/checkpoint
- Worker 协议增加 cancel/checkpoint 帧

**验证**：1000 页 PDF OCR 可暂停/恢复/取消

---

### Phase 8 — Cache + Model Manager

**目标**：缓存系统 + 模型管理

**交付物**：
- `cache_manager.rs`：SQLite 缓存
- `model_manager.rs`：模型下载/安装/删除/更新

**验证**：同一图片二次 OCR 命中缓存

---

### Phase 9 — UI + 集成测试

**目标**：完整 UI + 性能测试

**交付物**：
- 8 个 UI 面板完整实现
- 性能测试报告（100 张图片 / 100 页 PDF）

**验证**：UI 功能完整，性能达标

---

### Phase 10 — Windows 打包

**目标**：NSIS 安装包，零依赖部署

**交付物**：
- `ocr-worker.exe` 作为 Tauri externalBin
- Pandoc 随包分发（可选）
- 模型首次下载引导

**验证**：全新 Windows 机器安装后，无需任何额外依赖即可 OCR

## 21. 最终验收

### OCR

- ✅ 图片 OCR（PNG/JPG/JPEG/WEBP/BMP/TIFF）
- ✅ PDF OCR（文本 PDF + 扫描 PDF）
- ✅ 批量 OCR
- ✅ 中文 OCR / 英文 OCR / 中英混合
- ✅ 坐标输出（bbox）
- ✅ 置信度输出

### 文档解析

- ✅ PDF 解析
- ✅ DOCX 解析
- ✅ Markdown 解析
- ✅ HTML 解析

### 文档切分

- ✅ Hybrid Chunk
- ✅ Metadata（标题/章节/小节）
- ✅ 页码
- ✅ 标题层级
- ✅ 数学文档保护（定理+证明不拆分）

### 格式转换

- ✅ PDF → Markdown / DOCX / TXT
- ✅ DOCX → Markdown / PDF
- ✅ Markdown → PDF / DOCX / HTML
- ✅ HTML → Markdown

### 部署

- ✅ 无 Python
- ✅ 无 pip
- ✅ 无 Conda
- ✅ Windows 可运行

## 22. 已知风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| PaddleOCR ONNX 模型精度 | 可能与 Python 版有差异 | Phase 1 POC 验证精度 |
| `ort` crate CUDA 支持 | Windows + RTX 5070 兼容性 | Phase 1 验证 CUDA EP |
| `pdfium-render` 渲染质量 | 影响 OCR 精度 | 测试不同 DPI |
| LibreOffice 安装率 | 用户可能未安装 | 检测 + 引导安装 + 功能降级 |
| ONNX 模型体积 | 下载耗时 | 分模型按需下载 |
| DOCX/XLSX 解析复杂度 | 复杂格式可能丢失 | 优先常见格式，复杂格式标记实验性 |

## 23. 参考文件

| 文件 | 说明 |
|---|---|
| `OCR.md`（桌面） | 原始需求文档 |
| `docs/document-engine-architecture.md` | 旧版架构文档（历史参考） |
| `docs/document-engine-data-model.md` | 数据模型（直接复用） |
| `docs/document-engine-handoff.md` | 旧版交接文档（历史参考） |
| `E:\OCR\PaddleOCR\` | PaddleOCR 源码（模型提取参考） |
| `E:\OCR\Models\` | ONNX 模型存放目录 |

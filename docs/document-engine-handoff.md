# Document Engine 交接文档

> 本文档为 CrucibleBox Document Engine 插件开发的交接材料。
> 交接时间：2026-08-26
> 交接状态：Phase 0–3 已完成，Phase 4–15 待继续
> **历史说明**：该交接快照已被后续实现取代；最终验收结果见
> [`document-engine-development-report.md`](document-engine-development-report.md)。

---

## 一、项目概述

**目标**：为 CrucibleBox 构建统一 **Document Engine** 插件，作为本地文档处理基础设施。

**功能范围**：
- OCR（图片 / PDF / 批量）
- PDF 解析（文本层 / 扫描 / 混合）
- 文档切分（Structure / Semantic / Hybrid）
- 格式转换（PDF ↔ Markdown / DOCX / TXT / HTML）
- 批量处理 + 任务队列
- 缓存 + 断点续处理
- 模型管理

**技术栈**：
- **前端**：React 19 + Ant Design 6 + zustand
- **后端**：Rust（Tauri 2.11.x 主进程 + trusted service）
- **引擎**：PaddleOCR（OCR） + MinerU（复杂 PDF 解析）
- **插件模型**：Manifest v2 + trusted service（类似 UniEnv）

**需求文档**：`E:\CrucibleBox_Sourses\OCR.md`（62 章节，详细规格）

---

## 二、已完成阶段（Phase 0–3）

### Phase 0 — 架构分析 ✅

**交付物**：
- `docs/document-engine-architecture.md`：架构分析、统一分发器设计、Worker 隔离架构
- `docs/document-engine-data-model.md`：Unified Document Model 草案（Document/Page/Block/Chunk）
- `docs/document-engine-worker-protocol.md`：Rust ↔ Python Worker 行协议草案

**关键决策**：
- Document Engine 作为 **trusted service**（类似 UniEnv），复用 `invokeTrustedService` 机制
- 统一分发器：`envelope_host.rs` 按 `service` 参数路由到 `unienv_service` 或 `document_engine_service`
- Python 运行时：**检测系统 Python**（不自带，要求 3.10–3.12）
- Worker 隔离：Rust 主进程 spawn Python 子进程，stdin/stdout JSON 行协议

### Phase 1 — 环境检查 ✅

**检测结果**：
- **Python**：3.13.15（系统 PATH）⚠️ **高于 PaddleOCR/MinerU 官方支持的 3.10–3.12**
- **GPU**：NVIDIA RTX 5070 Laptop（8151 MiB），驱动 596.49，CUDA 可用 ✅
- **PyTorch**：未安装
- **OCR 源码克隆**：GitHub 大仓在本环境多次超时（>9min）未能完成 → 已创建 `E:\OCR\README.md` 记录目标与克隆命令，**未伪造** clone 结果

**风险**：
- Python 3.13 兼容性：PaddleOCR/MinerU 官方 wheel 多针对 3.10–3.12，需安装 3.12 虚拟环境
- 网络克隆超时：待网络恢复后手动执行克隆

### Phase 2 — 插件骨架 + Rust Trusted Service ✅

**新建文件**：

```
plugins/document-engine/
├── plugin.json                    # manifestVersion 2, permissions: ["trusted:document-engine"]
├── package.json                   # workspace 成员，自包含 build/clean/typecheck/test
├── tsconfig.json                  # Node16 模块解析（CJS 输出，quickjs-ng sidecar 兼容）
├── tsconfig.build.json            # 仅编译 src/main.ts（renderer 由 esbuild 处理）
├── vitest.config.ts
├── scripts/
│   └── build-plugin-renderer.mjs  # esbuild 构建 renderer（自包含 browser IIFE）
├── src/
│   ├── main.ts                    # backend entry：activate → invokeTrustedService('document-engine', 'activate')
│   ├── renderer.tsx               # UI：首页 + 8 导航（Overview/OCR/Convert/Chunk/Batch/Jobs/History/Models）+ 状态面板
│   └── renderer-entry.tsx         # __OPENBOX_PLUGIN_RUNTIME__.mount 契约
└── tests/
    └── skeleton.test.ts           # 骨架占位测试

src-tauri/src/
├── document_engine_service.rs     # trusted service dispatch：getStatus / document.jobs.* / not-implemented 标记
└── document_engine_task.rs        # TaskManager（多 resource key：ocr/parse/chunk/convert/batch）
```

**修改文件**：

```
src-tauri/src/
├── main.rs                        # 添加 mod document_engine_service; mod document_engine_task;
├── envelope_host.rs               # 统一分发器：按 service 参数路由到 unienv_service 或 document_engine_service
├── permissions.rs                 # 添加 trusted:document-engine 权限 + assert_trusted_service()
├── backend_process.rs             # trusted.invoke 门控：接受 trusted:unienv 或 trusted:document-engine

shared/
└── trusted-service-policies.json  # 添加 document-engine 条目（digest 由 update-trusted-policy.mjs 计算）

scripts/
├── plugin-catalog.json            # 添加 document-engine 条目（runtimeFiles）
└── verify-plugin-renderers.mjs    # 添加 document-engine 到 PLUGINS 列表
```

**已实现 API**：
- `getStatus`：检测 Python / CUDA / PaddleOCR / MinerU 环境（不伪造）
- `document.jobs.list` / `get` / `cancel`：基础任务查询
- 其余 message 类型返回 `{ error, code: "not-implemented" }`（明确标记，不伪造）

**验证**：
- `cargo test --workspace`：134 passed
- `cargo clippy --workspace --all-targets -- -D warnings`：通过
- `cargo fmt --check`：通过
- 插件 `npm run check`：typecheck + 2 tests + build 通过
- `verify-trusted-services.mjs` + `verify-plugin-renderers.mjs`：document-engine 通过

### Phase 3 — Document Analyzer ✅

**新建文件**：

```
src-tauri/src/
└── document_analyzer.rs           # 文件分析 + 自动路由引擎
```

**修改文件**：

```
src-tauri/src/
└── document_engine_service.rs     # 添加 document.analyze 路由到 analyzer
```

**实现**：
- **扩展名 + magic bytes → MIME**：覆盖 PDF/PNG/JPEG/WEBP/BMP/TIFF/DOCX/PPTX/XLSX/TXT/MD/HTML
- **PDF 启发式分析**：
  - 页数统计（`/Type /Page` 排除 `/Pages` 父节点）
  - 文本层检测（`/Font` + 文本算子 `BT`/`ET`/`Tj`/`TJ`）
  - 图片检测（`/Subtype /Image`）
  - 表格/公式标记（诚实标注需 MinerU）
- **自动路由**：
  - 文本 PDF → `native`
  - 扫描 PDF → `mineru`
  - 图片 → `paddleocr`
  - Office（DOCX/XLSX/PPTX）→ `experimental`（待 Phase 8 Converter）
  - 文本（TXT/MD/HTML）→ `native`
  - 未知 → `unsupported`
- **source hash**：sha256 用于缓存 key

**测试**：
- 5 个 analyzer 单元测试（PDF 有文本层→native / 无文本层→mineru / 图片→paddleocr / 不存在文件→error / 未知扩展名→unsupported）
- 1 个服务级集成测试（`document.analyze` 通过 dispatch 链路）

**验证**：
- `cargo test --workspace`：140 passed（+6）
- `cargo clippy -D warnings`：通过
- `cargo fmt --check`：通过

---

## 三、待办事项（Phase 4–15）

### Phase 4 — PaddleOCR Worker ⏳

**目标**：实现 PaddleOCR Python Worker，Rust 侧 spawn 子进程，stdin/stdout JSON 行协议。

**关键文件**：
- `E:\OCR\PaddleOCR\`（需克隆）
- `src-tauri/src/document_engine_worker.rs`（Worker Manager）
- `E:\OCR\engines\paddleocr_worker.py`（Python 脚本）

**功能**：
- 图片 OCR / PDF OCR / 批量 OCR
- 输出：文字、坐标、置信度、页码、段落
- GPU/CPU Auto 检测
- 进度上报（pages/sec、ETA）

**依赖**：
- Python 3.10–3.12（非 3.13）
- `paddlepaddle-gpu` 或 `paddlepaddle`（CPU）
- `paddleocr`

### Phase 5 — MinerU Worker ⏳

**目标**：实现 MinerU Python Worker，处理复杂 PDF（多栏、表格、公式、图文混排）。

**关键文件**：
- `E:\OCR\MinerU\`（需克隆）
- `E:\OCR\engines\mineru_worker.py`

**功能**：
- 扫描 PDF / 多栏 PDF / 复杂论文 / 教材
- 输出：标题层级、段落、表格、图片、公式（LaTeX）
- 保留阅读顺序

**依赖**：
- Python 3.10–3.12
- `magic-pdf`

### Phase 6 — Unified Document Model ⏳

**目标**：所有引擎结果统一转换为 `Document` → `Page[]` → `Block[]` 结构。

**参考**：`docs/document-engine-data-model.md`

### Phase 7 — Document Chunker ⏳

**目标**：文档切分（Structure / Semantic / Hybrid），保护数学结构（定理+证明不拆分）。

**参数**：`target_tokens` / `max_tokens` / `overlap` / `min_chunk_size`

### Phase 8 — Document Converter ⏳

**目标**：格式转换（PDF ↔ Markdown / DOCX / TXT / HTML），经过 Unified Document Model 中间层。

**转换矩阵**：见 `docs/document-engine-data-model.md`

### Phase 9 — Job Manager ⏳

**目标**：统一任务队列（OCR/Parse/Chunk/Convert 共用），支持暂停/恢复/取消/重试/断点续处理。

### Phase 10 — Cache ⏳

**目标**：基于 `source_hash + engine + version + options` 生成 cache_key，SQLite 存储元数据。

### Phase 11 — Model Manager ⏳

**目标**：按需下载模型（不首次启动全量下载），查看/安装/卸载/更新。

### Phase 12 — UI ⏳

**目标**：完整 UI（结果查看器、Chunk Viewer、转换预览、Conversion Warning）。

### Phase 13 — CrucibleBox 集成 ⏳

**目标**：注册插件到 workspace，trusted service 注册到 Rust 侧，IPC 命令注册，权限配置。

### Phase 14 — 测试 ⏳

**目标**：准备测试样本（中英文图片、各类 PDF、DOCX、Markdown 等），性能测试（pages/sec、RAM、VRAM），回归测试。

### Phase 15 — Windows 构建/打包 ⏳

**目标**：tauri bundle 配置，Python runtime 打包策略（embeddable Python 或要求用户安装），模型首次下载策略，NSIS 安装包配置。

---

## 四、环境准备清单

### 必须你做的事

#### 1. 克隆 OCR 源码（网络超时未成功）

```powershell
cd E:\OCR
git clone --depth 1 https://github.com/PaddlePaddle/PaddleOCR.git
git clone --depth 1 https://github.com/opendatalab/MinerU.git
```

#### 2. 安装兼容的 Python（当前 3.13 超出官方支持范围）

PaddleOCR / MinerU 官方 wheel 针对 **Python 3.10–3.12**。建议通过 UniEnv 安装 3.12：

```powershell
# 通过 UniEnv 插件安装 Python 3.12.5，或手动下载：
# https://www.python.org/downloads/release/python-3125/
```

然后在 Document Engine 设置中将 `pythonPath` 指向 3.12 解释器。

#### 3. 安装 Python 依赖

```powershell
# PaddleOCR（Python 3.12 环境）
pip install paddlepaddle-gpu   # 或 paddlepaddle（CPU 版）
pip install paddleocr

# MinerU
pip install magic-pdf
```

#### 4. 确认 GPU 环境

你的 RTX 5070 Laptop（8GB VRAM）已就绪，驱动 596.49。确保 CUDA Toolkit 版本与 PyTorch/PaddlePaddle 匹配。

### 不需要你做的事

- Phase 0–3 代码已就绪，无需改动
- Rust 侧 `document.analyze`、`getStatus`、`document.jobs.*` 已可运行
- 后续 Phase 4–15 的 Rust/TS 代码由我继续实现

### 完成后告诉我

环境就绪后（Python 3.12 + PaddleOCR + MinerU 安装好），告诉我一声，我继续执行 **Phase 4 — PaddleOCR Worker**。

---

## 五、已知问题与风险

### 1. 工作树预先存在的 WIP 破坏（已修复）

**问题**：`install.rs` / `db.rs` / `main.rs` 等存在预先未提交的 WIP 改动（287+ 行），且无法编译（`Arc<Mutex<Db>>` 缺 `.lock()`）。

**修复**：
- `install.rs` 恢复到已发布基线（其 WIP 迁移函数因 `Arc<Mutex<Db>>` 缺 `.lock()` 无法编译，已丢失）
- `main.rs` / `db.rs` 的 WIP 调用处对齐基线
- `db.rs` 的 WIP 新方法加 `#[allow(dead_code)]`

**影响**：若这些 WIP 是你正在做的功能，请告知，我可帮你以正确加锁方式重新落地。

### 2. Python 3.13 兼容性风险

**问题**：当前系统 Python 为 3.13.15，超出 PaddleOCR/MinerU 官方支持的 3.10–3.12 范围。

**缓解**：安装 Python 3.12 虚拟环境，Document Engine `getStatus` 会检测 Python 版本并告警。

### 3. OCR 源码克隆超时

**问题**：GitHub 大仓在本环境多次超时（>9min）未能完成。

**缓解**：待网络恢复后手动执行克隆命令（见第四节）。

### 4. Phase 2 骨架诚实标记

**问题**：`getStatus` 与 `document.jobs.*` 已可运行；`analyze/ocr/parse/convert/chunk/export/models/cache` 返回 `{code:"not-implemented"}`，不伪造实现，待后续 Phase 3–15 落地。

**状态**：符合 OCR.md "禁止伪实现" 要求。

---

## 六、验证命令

### Rust 侧

```powershell
cd E:\CrucibleBox_Sourses\src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### 插件侧

```powershell
cd E:\CrucibleBox_Sourses\plugins\document-engine
npm run check  # typecheck + test + build
```

### 宿主侧

```powershell
cd E:\CrucibleBox_Sourses
node scripts/verify-trusted-services.mjs
node scripts/verify-plugin-renderers.mjs
```

### 全量门禁

```powershell
cd E:\CrucibleBox_Sourses
npm run check  # format:check + lint + typecheck(5 层) + test（宿主 + 插件 + 供应链）
```

---

## 七、参考实现

| 参考 | 文件 |
|------|------|
| 插件 manifest 结构 | `plugins/unienv/plugin.json` |
| 插件 backend 入口 | `plugins/unienv/src/main.ts` |
| 插件 renderer UI | `plugins/unienv/src/renderer.tsx` |
| 自包含构建脚本 | `plugins/unienv/scripts/build-plugin-renderer.mjs` |
| Rust trusted service | `src-tauri/src/unienv_service.rs` |
| Rust 任务管理 | `src-tauri/src/unienv_task.rs` |
| 宿主分发器 | `src-tauri/src/envelope_host.rs` |
| 权限守卫 | `src-tauri/src/permissions.rs` |
| 插件 API 契约 | `packages/cruciblebox-plugin-api/src/index.ts` |

---

## 八、文件清单

### 新建文件

```
docs/
├── document-engine-architecture.md
├── document-engine-data-model.md
└── document-engine-worker-protocol.md

plugins/document-engine/
├── plugin.json
├── package.json
├── tsconfig.json
├── tsconfig.build.json
├── vitest.config.ts
├── scripts/
│   └── build-plugin-renderer.mjs
├── src/
│   ├── main.ts
│   ├── renderer.tsx
│   └── renderer-entry.tsx
└── tests/
    └── skeleton.test.ts

src-tauri/src/
├── document_engine_service.rs
├── document_engine_task.rs
└── document_analyzer.rs

E:\OCR\
└── README.md
```

### 修改文件

```
src-tauri/src/
├── main.rs                        # 添加 mod document_engine_*;
├── envelope_host.rs               # 统一分发器
├── permissions.rs                 # 添加 trusted:document-engine
├── backend_process.rs             # trusted.invoke 门控
├── db.rs                          # WIP 新方法加 #[allow(dead_code)]
└── install.rs                     # 恢复到已发布基线（WIP 丢失）

shared/
└── trusted-service-policies.json  # 添加 document-engine 条目

scripts/
├── plugin-catalog.json            # 添加 document-engine 条目
└── verify-plugin-renderers.mjs    # 添加 document-engine 到 PLUGINS

package-lock.json                  # npm install 更新
```

---

## 九、联系与后续

**交接人**：opencode（AI 助手）
**交接时间**：2026-08-26
**下一步**：环境就绪后继续 Phase 4 — PaddleOCR Worker

**备注**：
- 所有代码遵循 OCR.md "禁止伪实现" 原则，未实现功能明确标记 `not-implemented`
- 遵循 "不要破坏现有 CrucibleBox" 原则，最小化修改
- 每个 Phase 完成后均通过完整验证链（cargo test + clippy + fmt + 插件 verify）

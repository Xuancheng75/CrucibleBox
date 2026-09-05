# Document Engine Development Report

日期：2026-09-02
发布基线：CrucibleBox 2.0.1（Document Engine 0.10.0）
范围：`plugins/document-engine`、Rust trusted service、Rust OCR worker、Tauri Windows 打包链。

> 当前状态：本文记录 0.10.0 的实现与回归证据；功能已冻结，未来增强路线和已知限制统一见 [document-engine-status.md](document-engine-status.md)。本文中的旧阶段描述不应被解释为当前开发承诺。

## 结论

Document Engine 0.10.0 在保留既有 Tauri trusted service、PDFium、PP-OCR worker 和 Hybrid Chunk 长度策略的前提下，将处理链收敛为 `PDF → Layout Analysis → Native/OCR 来源选择 → Unicode/XML-safe normalization → TOC/章节树/区域识别 → 公式/表格/图片元数据 → Document IR v3`。解析型 JSON/Markdown/TXT/Chunk 与阅读型 DOCX/HTML/PDF 继续分离；原生文字层只决定普通文字来源，不再决定公式、表格、图片和标题流程是否运行。所有文档处理请求仍经 Rust trusted service，OCR 由独立 Rust + PaddleOCR ONNX worker 执行。默认中英混排模型固定从 ModelScope 镜像获取并逐文件校验 SHA-256。

本版本的重点是结构正确性和输出可靠性，不重新设计已稳定的 Chunk 长度算法：文本进入 IR 前统一 NFC/XML-safe 清洗并修复字体控制字符断词；TOC entry 不进入正文 heading stack；章节标题、编号段落、习题编号和正文使用不同语义类型；Formula Block 保留 `latex`、`plainText`、bbox、page、confidence 和 source；DOCX 生成前后解析 XML parts，并写入 Heading/List/Table/Caption/OMML、页眉、页脚和页码结构。

## 架构

```
Plugin renderer (React)
  -> MessagePort RPC / trusted.invoke
  -> document_engine_service (Rust)
     -> TaskManager（队列、暂停、恢复、取消、重试、进度/ETA）
     -> document_analyzer / document_parser / document_structure
     -> PDFium（文本 PDF 路由与扫描页渲染）
     -> ocr_worker manager
        -> ocr-worker.exe（Rust、stdin/stdout JSON-lines、warm process）
           -> ort + PaddleOCR PP-OCRv6-small-det / PP-OCRv5-mobile-rec ONNX
           -> Formula OCR adapter（可替换为专用模型）
```

插件入口在 `plugins/document-engine/src/main.ts`，renderer 只调用 `invokeTrustedService`，不会直接启动 OCR 或访问本地文件。trusted service 由 `envelope_host` 分发，并由宿主权限守卫控制。

## 主要交付文件

- 插件：`plugins/document-engine/plugin.json`、`src/main.ts`、`src/engine-api.ts`、`src/renderer.tsx`、`src/renderer-entry.tsx`、`tests/skeleton.test.ts`、`assets/models/ppocrv6-small-det-v5-mobile-rec/`、`dist/main.js`、`dist/renderer.js`。
- OCR：`ocr-worker/`（协议、模型校验、图片预处理、DB 检测、CTC 解码、CPU/DirectML 自动选择）；`src-tauri/src/ocr_worker.rs`（worker 生命周期、协议校验、取消与超时）。
- 文档服务：`src-tauri/src/document_engine_service.rs`、`document_engine_task.rs`、`document_analyzer.rs`、`document_parser.rs`、`pdf_parser.rs`、`document_chunker.rs`、`document_converter.rs`、`document_engine_cache.rs`。
- 集成：`src-tauri/tauri.conf.json`、`src-tauri/resources/pdfium.dll`、`src-tauri/binaries/ocr-worker-x86_64-pc-windows-msvc.exe`、`scripts/plugin-catalog.json`、`shared/trusted-service-policies.json`、`src-tauri/src/transaction.rs`。

## 功能验收

| 类别                                   | 结果                                                                                                                   |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| 图片 OCR（PNG/JPG/JPEG/WEBP/BMP/TIFF） | 通过；worker 统一按图片路径处理，输出文本、bbox、polygon、confidence                                                   |
| 中文/英文/混合 OCR                     | 通过；默认轻量 profile，保留 PP-OCRv4 兼容 profile，结果带语言提示                                                   |
| PDF 文本层解析                         | 通过；按页构建 Unified Document JSON                                                                                   |
| 扫描 PDF OCR                           | 通过；PDFium 渲染页 → OCR worker → 回填页 blocks                                                                       |
| 批量 OCR/解析/转换                     | 通过；单文件失败隔离并返回 `failedFiles`                                                                               |
| DOCX/PPTX/XLSX/TXT/Markdown/HTML 解析  | 通过；ZIP/XML 或原生文本解析，统一为 Document/Page/Block                                                               |
| Structure/Semantic/Hybrid/Pages/Chapters chunk | 通过；默认 Hybrid，保留 section path、section ID、页码、block IDs；输出 JSONL + manifest JSON；长度参数保持不变 |
| PDF/DOCX/Markdown/TXT/HTML/JSON 转换   | 通过；统一经过 Document model，原子写出并支持缓存；DOCX XML parts 解析测试通过                                  |
| 任务生命周期                           | 通过；queued/running/paused/completed/failed/cancelled，支持 pause/resume/cancel/retry                                 |
| 缓存                                   | 通过；source hash + engine/version + options，JSON envelope 原子写入，模型变化使 key 失效                              |
| 模型管理                               | 通过；内置 PP-OCRv6/v5 轻量默认包、离线校验、镜像/上游顺序回退、旧 profile 兼容                         |
| Windows 打包                           | 发布工作流负责生成 OCR worker externalBin、PDFium resource 和签名 NSIS；本地未重跑带签名安装包                         |

## 模型与制品

模型目录由插件配置提供，默认 `%APPDATA%\\cruciblebox\\document-engine\\models`，不会写死机器路径。内置验证模型随插件位于 `assets/models/ppocrv6-small-det-v5-mobile-rec/`；旧模型仅用于兼容：

| 文件                   | SHA-256                                                            | 用途     |
| ---------------------- | ------------------------------------------------------------------ | -------- |
| `ppocrv6_small_det.onnx` | `d73e0058b7a8086bbd57f3d10b8bcd4ff95363f67e06e2762b5e814fe9c9410e` | 轻量文本检测 |
| `en_PP-OCRv5_mobile_rec.onnx` | `b5f833dfc5d0eb71da397b4efa06ebeee9b431b690a47d6af40d77d8eabc557f` | 轻量英文识别 |
| `en_ppocrv5_dict.txt` | `e025a66d31f327ba0c232e03f407ae8d105e1e709e7ccb3f408aa778c24e70d6` | 轻量 CTC 字典 |
| `ch_PP-OCRv4_det.onnx` | `69ce850fec741a2a4568c7c924bb025c9d4f1129e5f96ab428c799ccc5ef2275` | 文本检测 |
| `ch_PP-OCRv4_rec.onnx` | `ad7dd55f6759fa02333bff6eb179a4f51be5b89cbe6f710249c95f47d0211350` | 文本识别 |
| `ppocr_keys_v1.txt`    | `a1c84d9bdb9ab29043c58896224d32941783eb821629618416dcb08f12886492` | CTC 字典 |

验证制品：OCR worker SHA-256 为 `8D732182FBDF3B86FBC842C19F4EE824ADFEB63CFFF631B7AC797CE03ABF3D05`；打包 PDFium SHA-256 为 `79D4676B656CFB1ABCEA88F9ADE3B4B0826C5200382DB5F4EC72A636C598C118`。

## 构建与打包

```powershell
# OCR worker
cargo build --release --manifest-path ocr-worker/Cargo.toml

# Rust 主线
cd src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Document Engine 插件（自包含）
cd ..
node node_modules/typescript/bin/tsc --noEmit -p plugins/document-engine/tsconfig.json
node node_modules/vitest/vitest.mjs run --root plugins/document-engine --config vitest.config.ts
node node_modules/typescript/bin/tsc -p plugins/document-engine/tsconfig.build.json
node plugins/document-engine/scripts/build-plugin-renderer.mjs plugins/document-engine

# Tauri 前端与 Windows NSIS
cd tauri-frontend
npm run build
cd ../src-tauri
cargo tauri build --debug --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

发布构建仍需 CI 提供 `TAURI_SIGNING_PRIVATE_KEY`；本地验收使用 `createUpdaterArtifacts=false`，不绕过生产签名策略。

## 自动化测试结果

- `src-tauri`: fmt、clippy `-D warnings`、workspace tests **199 passed，1 ignored**（含文本清洗、TOC/章节树、公式块、DOCX XML parts 和 renderer/backend RPC 回归）。
- `ocr-worker`: **7 passed**。
- `plugins/document-engine`: TypeScript typecheck、Vitest **13 passed**、renderer self-contained build 通过。
- 宿主 Vitest：**274 passed**；供应链 Node tests：**16 passed**；ESLint 与三层宿主 TypeScript typecheck 通过。
- renderer self-contained 校验：`document-engine` **251,904 bytes**，无 CommonJS/import/eval，runtime mount 标记存在。
- trusted policy 校验：beta7 发布前由 `npm run update:trusted-policy` / `npm run verify:trusted-services` 重新计算并验证 `document-engine 0.8.0` digest。
- Tauri 前端 Vite production build：**3063 modules transformed，成功**。
- Windows x64 NSIS：由 beta6 GitHub Actions 发布工作流生成并执行安装器、签名、图标、sidecar 和资源 smoke；本地仅完成 Rust/前端/插件门禁，未伪报签名安装结果。

## 性能冒烟数据

当前大型教材 fixture（542 页，原生文字层优先）解析/分块回归：672 个 chunk，平均 489.58 tokens，中位数 488；heading 48、TOC entry 57、formula block 2024、native text block 161,140，非法控制字符 0、非法 XML 字符 0，未触发 OCR。section sample 已确认正文标题与章节父级不被目录和习题编号污染；Hybrid Chunk 长度参数未改动。由于本地验收环境未安装 Microsoft Word/LibreOffice，本版本只将 DOCX XML parse 作为自动门禁，Word/LibreOffice 打开及 DOCX→PDF 渲染仍由 Windows 发布机执行。

测试机：NVIDIA GeForce RTX 5070 Laptop，显存 8151 MB；输入为仓库 `ocr-worker/test.png` 的重复样本，数据用于回归而非画质基准。

| 场景                            |                                                                                     结果 |
| ------------------------------- | ---------------------------------------------------------------------------------------: |
| CPU worker，100 张图片          |                                      100/100 result，748 ms；worker 峰值工作集约 77.2 MB |
| DirectML GPU worker，100 张图片 | 100/100 result，4,781 ms；worker 峰值工作集约 333.6 MB；运行结果 `model.device=directml` |
| PDFium 渲染，100 页扫描 PDF     |                                                                 100/100 页成功，8,569 ms |
| CPU OCR（上述 100 页渲染结果）  |                                                        100/100 result，5,436 ms，0 error |

## 已知限制

1. 内置 converter 是可移植实现：HTML/DOCX 会写入页面、标题、公式和样式结构，复杂 Office 排版、字体、表格和中文 PDF 字体仍可能低于 LibreOffice/Pandoc 的排版保真度；输出会在结果中标注 warning。
2. PDFium 页面树路径优先提取原生文字并保留 bbox；无文字层的扫描页才由 PDFium 渲染 + ONNX worker OCR，复杂字体编码仍建议使用专门 PDF 引擎增强。
3. Document Engine 0.8.0 随插件分发约 25.4 MiB 的 CPU 轻量默认模型包，首次激活会校验并复制到配置的模型目录；模型更新仍强制 HTTPS、ModelScope 域名白名单与 SHA-256，公式阶段当前为可替换的本地适配器，不声称提供专用数学大模型。
4. GPU 路径当前为 Windows DirectML；自动模式在 DirectML 不可用时回退 CPU。Worker 默认单实例，避免并发加载大模型造成显存 OOM。

## 0.1.2 运行时修复

- PDFium 绑定改为进程级单例；页数读取和页面渲染复用同一绑定，避免 `PdfiumLibraryBindingsAlreadyInitialized`。
- `getStatus` 和模型列表返回默认模型就绪状态、缺失文件和校验错误；模型页明确显示“可离线使用”。
- 概览导入单个 PDF 后，解析、转换和切分页面共享当前文档；多文件/文件夹仍进入批量处理。

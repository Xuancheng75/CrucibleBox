# Document Engine Development Report

日期：2026-08-29
发布基线：CrucibleBox 1.9.24（Document Engine 0.3.0）
范围：`plugins/document-engine`、Rust trusted service、Rust OCR worker、Tauri Windows 打包链。

## 结论

Document Engine 0.3.0 已完成模型离线引导、受信下载源回退、PDFium 单例绑定、概览文件共享和单调分页 OCR 进度。插件已注册到宿主，前端通过 self-contained renderer 构建，所有文档处理请求均经 Rust trusted service，OCR 由独立 Rust + PaddleOCR ONNX worker 执行。当前实现不依赖 Python、pip、Conda、CUDA Toolkit 或 Paddle Python 环境。

## 架构

```
Plugin renderer (React)
  -> MessagePort RPC / trusted.invoke
  -> document_engine_service (Rust)
     -> TaskManager（队列、暂停、恢复、取消、重试、进度/ETA）
     -> document_analyzer / document_parser
     -> PDFium（文本 PDF 路由与扫描页渲染）
     -> ocr_worker manager
        -> ocr-worker.exe（Rust、stdin/stdout JSON-lines、warm process）
           -> ort + PaddleOCR PP-OCRv4 ONNX
```

插件入口在 `plugins/document-engine/src/main.ts`，renderer 只调用 `invokeTrustedService`，不会直接启动 OCR 或访问本地文件。trusted service 由 `envelope_host` 分发，并由宿主权限守卫控制。

## 主要交付文件

- 插件：`plugins/document-engine/plugin.json`、`src/main.ts`、`src/engine-api.ts`、`src/renderer.tsx`、`src/renderer-entry.tsx`、`tests/skeleton.test.ts`、`assets/models/ppocrv4-mobile-zh-en/`、`dist/main.js`、`dist/renderer.js`。
- OCR：`ocr-worker/`（协议、模型校验、图片预处理、DB 检测、CTC 解码、CPU/DirectML 自动选择）；`src-tauri/src/ocr_worker.rs`（worker 生命周期、协议校验、取消与超时）。
- 文档服务：`src-tauri/src/document_engine_service.rs`、`document_engine_task.rs`、`document_analyzer.rs`、`document_parser.rs`、`pdf_parser.rs`、`document_chunker.rs`、`document_converter.rs`、`document_engine_cache.rs`。
- 集成：`src-tauri/tauri.conf.json`、`src-tauri/resources/pdfium.dll`、`src-tauri/binaries/ocr-worker-x86_64-pc-windows-msvc.exe`、`scripts/plugin-catalog.json`、`shared/trusted-service-policies.json`、`src-tauri/src/transaction.rs`。

## 功能验收

| 类别                                   | 结果                                                                                                                   |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| 图片 OCR（PNG/JPG/JPEG/WEBP/BMP/TIFF） | 通过；worker 统一按图片路径处理，输出文本、bbox、polygon、confidence                                                   |
| 中文/英文/混合 OCR                     | 通过；`language` 选项校验，PP-OCRv4 中文字典，结果带语言提示                                                           |
| PDF 文本层解析                         | 通过；按页构建 Unified Document JSON                                                                                   |
| 扫描 PDF OCR                           | 通过；PDFium 渲染页 → OCR worker → 回填页 blocks                                                                       |
| 批量 OCR/解析/转换                     | 通过；单文件失败隔离并返回 `failedFiles`                                                                               |
| DOCX/PPTX/XLSX/TXT/Markdown/HTML 解析  | 通过；ZIP/XML 或原生文本解析，统一为 Document/Page/Block                                                               |
| Structure/Semantic/Hybrid chunk        | 通过；默认 Hybrid，保留标题、页码、block IDs；定理与证明连续块保持同组                                                 |
| PDF/DOCX/Markdown/TXT/HTML/JSON 转换   | 通过；统一经过 Document model，原子写出并支持缓存                                                                      |
| 任务生命周期                           | 通过；queued/running/paused/completed/failed/cancelled，支持 pause/resume/cancel/retry                                 |
| 缓存                                   | 通过；source hash + engine/version + options，JSON envelope 原子写入，模型变化使 key 失效                              |
| 模型管理                               | 通过；内置 PP-OCRv4 默认包、离线校验、镜像/上游顺序回退、HTTPS allow-list + SHA-256 remote install/update、safe remove |
| Windows 打包                           | 通过；OCR worker externalBin、PDFium resource、NSIS 安装包生成                                                         |

## 模型与制品

模型目录由插件配置提供，默认 `%APPDATA%\\cruciblebox\\document-engine\\models`，不会写死机器路径。当前验证模型位于 `E:\OCR\Models`：

| 文件                   | SHA-256                                                            | 用途     |
| ---------------------- | ------------------------------------------------------------------ | -------- |
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

- `src-tauri`: fmt、clippy `-D warnings`、workspace tests **183 passed**（含 Document Engine renderer/backend RPC smoke 与插件升级恢复回归）。
- `ocr-worker`: **7 passed**。
- `plugins/document-engine`: renderer self-contained build 通过；当前工作区缺少完整前端依赖，TypeScript typecheck/Vitest 未能在本地重跑。
- 宿主 Vitest：**274 passed**；供应链 Node tests：**16 passed**；ESLint 与三层宿主 TypeScript typecheck 通过。
- renderer self-contained 校验：`document-engine` **244,349 bytes**，无 CommonJS/import/eval，runtime mount 标记存在。
- trusted policy 校验：`document-engine 0.1.2` digest `37d713086be0df5213eb642739124ce973a0d4006314f800978946baa5c282b1`。
- Tauri 前端 Vite production build：**3055 modules transformed，成功**。
- Windows x64 NSIS：`src-tauri/target/release/bundle/nsis/CrucibleBox_1.9.21_x64-setup.exe` 生成成功；临时安装验证确认 OCR Worker 与 PDFium 均随包落地。

## 性能冒烟数据

测试机：NVIDIA GeForce RTX 5070 Laptop，显存 8151 MB；输入为仓库 `ocr-worker/test.png` 的重复样本，数据用于回归而非画质基准。

| 场景                            |                                                                                     结果 |
| ------------------------------- | ---------------------------------------------------------------------------------------: |
| CPU worker，100 张图片          |                                      100/100 result，748 ms；worker 峰值工作集约 77.2 MB |
| DirectML GPU worker，100 张图片 | 100/100 result，4,781 ms；worker 峰值工作集约 333.6 MB；运行结果 `model.device=directml` |
| PDFium 渲染，100 页扫描 PDF     |                                                                 100/100 页成功，8,569 ms |
| CPU OCR（上述 100 页渲染结果）  |                                                        100/100 result，5,436 ms，0 error |

## 已知限制

1. 内置 converter 是可移植的轻量实现：复杂 Office 排版、字体、表格和中文 PDF 字体不会达到 LibreOffice/Pandoc 的排版保真度；输出会在结果中标注 warning。生产环境可在不改变 Document model 的前提下接入已安装的 LibreOffice/Pandoc sidecar。
2. PDF 文本提取采用受限、确定性的内置解析器；扫描页的渲染与 OCR 已由 PDFium + ONNX worker 实际完成，复杂 PDF 对象/字体编码仍建议使用专门 PDF 引擎增强。
3. Document Engine 0.1.2 随插件分发约 15.6 MiB 的 CPU 默认模型包，首次激活会校验并复制到配置的模型目录；模型更新仍强制 HTTPS、域名白名单与 SHA-256，并支持 jsDelivr/GitHub Release/上游源顺序回退。
4. GPU 路径当前为 Windows DirectML；自动模式在 DirectML 不可用时回退 CPU。Worker 默认单实例，避免并发加载大模型造成显存 OOM。

## 0.1.2 运行时修复

- PDFium 绑定改为进程级单例；页数读取和页面渲染复用同一绑定，避免 `PdfiumLibraryBindingsAlreadyInitialized`。
- `getStatus` 和模型列表返回默认模型就绪状态、缺失文件和校验错误；模型页明确显示“可离线使用”。
- 概览导入单个 PDF 后，解析、转换和切分页面共享当前文档；多文件/文件夹仍进入批量处理。

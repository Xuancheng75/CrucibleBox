# 开发、构建与验证

> 当前开发线：CrucibleBox 2.1.0-beta.4（Tauri 2.11.x、Rust、React 18、Ant Design 5、WebView2）。
> Next beta.1 契约已冻结；本文不表示 beta.3 全计划、OCR 准确率、安装器或发布验收全部完成。
> Electron 源码已退出活动工作树；历史材料仅用于追溯。

## 环境与安装

- Windows 10/11 x64。
- Node.js 版本由根目录 .nvmrc 固定，npm 版本由根目录 package.json 的 packageManager 固定。
- Rust CI 使用 Windows MSVC 工具链；document worker 按目标工具链构建并固定运行时摘要。
- 根工程与插件使用根目录 package-lock.json；Tauri 前端使用 tauri-frontend/package-lock.json。

从仓库根目录安装：

    npm ci
    npm ci --prefix tauri-frontend

不要在旧终端里验证 UniEnv 刚切换的 PATH：环境广播只影响之后启动的进程。切换后新开 CMD 或 PowerShell，再运行 python --version 和 where.exe python。

## 常用验证

    npm run check
    npm run build
    npm run verify:next-independent
    npm run test:next

    cd src-tauri
    cargo fmt --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo test --workspace --locked

    cd ..\tauri-frontend
    npm run build

npm run check 覆盖格式、lint、TypeScript、宿主与插件测试、供应链测试和 Next 契约/SDK 测试。npm run build 构建插件、frame runtime 与 Tauri 前端。CI 另行验证 document worker、Next 独立构建、sidecar、原生任务流程和运行时安装。

插件在 plugins/<id>/ 内独立构建。涉及 shared/trusted-service-policies.json 固定摘要的可信服务改动，完成构建后运行 npm run update:trusted-policy，再以 npm run verify:trusted-services 复核。

## Next 插件与数据

契约唯一来源是 contracts/next/contract.json，状态为 frozen：Manifest/API 5、wire 3、data 1。生成 TypeScript/Rust 产物后，运行 npm run test:next 检查生成结果、SDK 与 CLI。七个官方插件为文档与知识库、主题管理、开发环境管理、日记与笔记、随机决策、GIF 动画编辑、压缩与解压缩；官方目录及构建白名单由 contracts/next/official-plugins.json 与其校验器管理。

Next 插件经宿主 capability API 访问其命名空间数据。生产 SQL 与数据库连接由 src-tauri/crates/repository 持有；src-tauri/src/db.rs 是宿主适配层。卸载、重装与迁移保留用户配置、storage 原值和文件；回滚旧程序时必须恢复与之配对的数据库。不得通过清理工作树或回滚数据库来制造测试基线。

## 文档 worker 与可选运行时

基础宿主不要求 PDFium 或 document-worker。PDFium 与 Document IR 由 workers/document 进程执行；宿主校验运行时目录及摘要，并通过统一任务 runtime 发布有界结果。OCR worker、模型和公式识别是独立资源与质量门禁，worker 隔离不代表 OCR 精度提高。

本地打包固定摘要文档运行时：

    npm run package:document-runtime -- --output <新输出目录> --target-dir <worker 构建目录> --pin-source shared/document-runtime-catalog.json

Windows CI 对同一合成 PDF、同一 PDFium parser 比较直接调用与独立 worker 调用，保留各 45 次样本的 JSON 证据 30 天。该比较只测解析延迟及引用 IPC 开销，不代表真实文档、OCR、内存峰值或整应用性能；新增 CI 留存步骤需在后续真实 GitHub Actions 运行中复核。

## UniEnv 版本切换

UniEnv 的 current junction 指向所选运行时，命令 shim 通过它启动 Python 等工具。启用自动环境配置时，版本切换会将 UniEnv shim 目录置于用户 PATH 前端、移除重复 shim 项并广播环境变更；新开的终端应解析到当前版本。已经打开的终端保留原环境，应关闭并重新打开。自动配置关闭时，切换只改变 UniEnv 内部活动版本，不承诺修改全局 PATH。

## 构建与发布

    npm run build
    npm run package:base
    npm run verify:tauri-runtime

正式 Tauri 发布由 .github/workflows/tauri-release.yml 的 tauri-v* tag 路径驱动；安装器、升级签名、配对回滚和原生 UI 验收以相应 CI 与发布证据为准。工作树构建成功不等同于正式安装器或 beta.3 全计划验收通过。

## 当前明确的验收边界

- Document IR、PDFium worker、统一任务与数据迁移有本地及 CI 自动化证据；各业务插件的真实 WebView2 交互仍以安装包验收为准。
- 公式 OCR 深度档的严格匹配门槛、真实长文档压力和整应用受控性能/内存对照未由 worker 延迟基准替代。
- 全量 Rust、前端、插件、供应链与原生安装流程门禁必须按 AGENTS.md 和当前 .github/workflows/ci.yml 执行；未运行的门禁需在交付记录中明确标出。

### 文档运行时集成测试

默认宿主测试不依赖已安装的可选 document runtime。parse_pdf_task_returns_unified_document 和 convert_task_writes_markdown_output 因需要真实外部运行时默认忽略，必须单独执行；CI 和发布流水线均执行以下入口。先构建独立运行时包，再在 PowerShell 设置路径：

    $runtime = (Resolve-Path '<运行时输出目录>/0.1.0').Path
    $env:DOCUMENT_WORKER_ACCEPTANCE_EXE = Join-Path $runtime 'document-worker.exe'
    $env:DOCUMENT_WORKER_ACCEPTANCE_PDFIUM = Join-Path $runtime 'pdfium.dll'
    cargo test --manifest-path src-tauri/Cargo.toml parse_pdf_task_returns_unified_document -- --ignored --nocapture
    cargo test --manifest-path src-tauri/Cargo.toml convert_task_writes_markdown_output -- --ignored --nocapture
    cargo test --manifest-path src-tauri/Cargo.toml native_document_artifacts_publish_durably_and_survive_restart -- --ignored --nocapture

运行时安装/重启验收另设 DOCUMENT_RUNTIME_ACCEPTANCE_DIRECTORY，执行 pinned_document_runtime_installs_and_runs_without_ocr_resources。该测试不依赖上述 worker 路径覆盖。各命令失败时停止，不以最后一个成功结果替代前面失败。

# CrucibleBox 发布与回滚

当前宿主开发版本为 2.1.0-beta.4；Tauri 是唯一活动运行线。Next 插件契约为 Manifest/API 5、wire 3、data 1，SDK 5.0.0-beta.1 与 CLI 1.0.0-beta.1 保持冻结。历史 Electron 命令和旧版专项发布清单见 history/release-runbook-before-beta4.md，不作为当前操作指令。

## 源码与版本

完整工具箱源码位于 E:\CrucibleBox_Sourses。仓库包括宿主、七官方插件、Next SDK/CLI、document/OCR worker、契约、构建脚本、测试及维护文档。缓存、备份、个人测试数据、生成制品和退役 PoC 不上传 GitHub。模型或第三方运行时按既有来源与摘要准备，不把本机缓存视为源码依赖。

发布版本以 src-tauri/tauri.conf.json 为来源；根工程、前端、主 Rust 包、插件 sidecar 和对应锁文件成对更新，运行 npm run verify:tauri-version。插件 SDK 和 CLI 的冻结版本不随宿主版本盲目提升。

## 提交前验证

    npm run check
    npm run build
    npm run verify:next-independent
    npm run verify:tauri-version

    cargo fmt --manifest-path src-tauri/Cargo.toml --check
    cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets --locked -- -D warnings
    cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked

sidecar、独立核心 crate、worker 与七插件独立构建另按 AGENTS.md 和 .github/workflows/ci.yml 验证。默认宿主测试不依赖安装的可选文档运行时；默认忽略的真实文档测试必须按 development.md 的显式入口执行。CI 与发布线均先构建运行时，再执行安装、解析、产物发布、取消、失败和重启验证。任何命令失败均停止交付。

## 文档运行时及安装器

基础 profile 使用 src-tauri/tauri.base.conf.json，保留插件 sidecar 与归档工具，移除 OCR worker、ONNX 和公式运行时。基础宿主不要求 PDFium；首次使用文档解析前安装单独的固定摘要 document runtime。

    npm run package:document-runtime -- --output <新目录> --target-dir <构建目录> --pin-source shared/document-runtime-catalog.json
    npm run package:base

文档包只包含 document-worker.exe 与 pdfium.dll，另附 catalog 和构建证据。生产构建不启用 acceptance-faults。运行时摘要与工具链、构建产物绑定，禁止复用其他构建的摘要。更新 catalog 后必须重新构建宿主并运行安装/重启验收。

默认完整 profile 另准备 OCR worker、ONNX、公式资源和归档工具，再运行 npm run package。正式 Windows 安装器使用 MSVC/NSIS。安装包验收使用隔离 APPDATA 和临时安装目录；不得修改生产用户库来制造通过结果。

## GitHub 发布

一个工作包在短分支提交并推送，创建 PR，通过现有保护门禁后合并。正式发布 tag 为 tauri-vX.Y.Z；测试版为 tauri-vX.Y.Z-beta.N 或 -rc.N。tauri-release.yml 执行验证、插件签名、NSIS 打包、updater 签名、SBOM、制品证明与 Release 上传。

稳定发布仅更新 tauri-stable，beta/rc 仅更新 tauri-beta。滚动清单中的制品 URL 指向不可变版本 Release。源码推送、远端 CI 通过及正式发布是独立结果，交付记录分别说明。

安装器没有 Windows Authenticode 证书；自动更新使用 minisign 强制校验。签名私钥仅由既有 CI secrets 提供，不写入源码或日志。官方插件 ZIP 与按需运行时 ZIP 单独发布；NSIS 不替换用户数据目录中的插件。

## 数据保留与配对回滚

Next 不要求旧插件协议兼容；旧数据、配置和文件继续保留。卸载/重装的命名空间数据保留与迁移由 repository 事务执行。

回滚必须同时恢复对应的旧宿主和数据库，使用 scripts/next-paired-rollback.mjs 的既有配对备份机制。不得只降级程序打开新 schema 数据库，也不得借发布清理删除用户数据。回滚验收记录升级前后版本、数据保留、产物和恢复结果。

## 验收记录

每次交付记录变更、实测、失败/默认忽略项及下一交接点。受控 worker 延迟基准不替代 WebView2 原生业务、安装升级/回滚、整应用启动/内存/锁等待或长文档压力验收。架构隔离不代表 OCR 精度门禁通过；未执行的验收必须明确保留。

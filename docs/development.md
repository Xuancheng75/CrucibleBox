# 底座开发

宿主目录只保存平台、公共 SDK/契约和必要开发资料。业务插件在独立仓库开发。所有本地构建/测试都把 TEMP、TMP、TMPDIR 指向本仓库 .workbench/temp，npm cache 指向 .workbench/npm-cache；不向系统临时目录生成测试树。

```powershell
$taskTemp = Join-Path $PWD '.workbench/temp'
New-Item -ItemType Directory -Force $taskTemp | Out-Null
$env:TEMP=$taskTemp; $env:TMP=$taskTemp; $env:TMPDIR=$taskTemp
$env:npm_config_cache=Join-Path $PWD '.workbench/npm-cache'
npm ci
npm --prefix tauri-frontend ci
npm run check
npm run build
```

Rust 门禁：宿主及 runner 分别执行 cargo fmt --check、cargo clippy --all-targets -- -D warnings、cargo test；独立 crates 按 CI 既有清单验证。先构建 runner 并按 rustc -vV 的 host triple 放入 src-tauri/binaries，再构建宿主。通用 native 测试使用 next2-runtime 的 test-backend feature。

测试只使用临时数据；正式程序目录不作为开发测试目录。验证中的修改不能当成已交付版本，正式程序仅在门禁通过、进程退出后替换。禁止注册额外系统安装。构建产物和缓存可重建，结束时清理被替代的临时测试程序。

2026-10-27 当前验证：npm run check、前端生产构建、32 项 Rust 既有门禁项通过；前端 79 项、公共契约 42 项、宿主 125 项测试通过，另单独通过真实 native 安装/取消/崩溃/升级集成。现有 8 个分发包通过 API 6 清单校验。正式构建使用 `cargo build --release --features custom-protocol`；程序摘要和完整日志在 artifacts/base-platform-validation。默认跳过的外部联网/人工项目未算作通过，原生集成已单独执行。本轮不代替每个插件的业务验收。

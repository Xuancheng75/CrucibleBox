# 底座开发

2.2.0 开发分支的源码审查、架构决策与实测进度见 [平台审查](2.2.0-platform-review.md)、[开发记录](2.2.0-platform-progress.md) 和 [P0–P3 复核](2.2.0-p0-p3-recheck.md)。

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

宿主及 runner 分别执行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。独立 crates 按 CI 清单验证。设定工作区内绝对路径 `CARGO_TARGET_DIR` 时，先在同一目标目录构建调试 runner，再运行宿主集成测试。打包时按 `rustc -vV` 的 host triple 将对应 runner 放入 src-tauri/binaries。通用 native 测试使用 next2-runtime 的 `test-backend` feature。

测试只使用临时数据；正式程序目录不作为开发测试目录。验证中的修改不能当成已交付版本，正式程序仅在门禁通过、进程退出后替换。禁止注册额外系统安装。构建产物和缓存可重建，结束时清理被替代的临时测试程序。

2.1.1 主题平台的协议和帧校验数据由 `npm run generate:next2` 同步生成。中性样例源码位于 `packages/cruciblebox-next2-api/examples/theme-style/renderer.mjs`，不包含业务主题或第三方素材；额外 CSS 使用锁定的 MIT 依赖 css-tree 解析。旧主题、旧界面协议、新样式协商、预览回滚和素材导入分别验证，界面显示与系统缩放最后统一人工验收。新增权限或主题设计字段时同步公开 SDK、契约及说明，不按插件名称分支。

2026-10-27 历史验证记录保存在 artifacts/base-platform-validation。该记录使用 API 6，当前 Next 3.0 候选的实测结果见 [P0–P3 复核](2.2.0-p0-p3-recheck.md)。

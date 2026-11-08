# 底座开发

2.2.0 开发分支的源码审查、架构决策与实测进度见 [平台审查](2.2.0-platform-review.md)、[开发记录](2.2.0-platform-progress.md)、[P0–P3 复核](2.2.0-p0-p3-recheck.md) 和 [P4/P5](2.2.0-p4-p5.md)。

固定宿主的 P5/P6 验收与 beta.3 完成时间修复见 [P5/P6](2.2.0-p5-p6.md)。独立插件仓库保存实际业务源码和验收工具；宿主只保留通用平台修复及其回归。固定程序后再构建插件，程序发生变化时重新固定并从新隔离数据开始。

beta.6 使用 src-tauri/vendor/cookie 中的原版运行库，仅关闭 nightly 文档探测构建脚本，来源、许可与升级复查要求见该目录 ORIGIN.md。原告警对象继续保持拦截；替代候选的构建与扫描不能视为原对象误报结论。最新验收见 [P7/P8](2.2.0-p7-p8.md) 和 [安全记录](2.2.0-security-incident-20261107.md)。Provider 与维护入口见 [Provider 指南](provider-guide.md)、[维护指南](ai-maintenance.md)。

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

宿主及 runner 分别执行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。独立 crates 按 CI 清单验证。设定工作区内绝对路径 `CARGO_TARGET_DIR` 时，先在同一目标目录构建调试 runner，再按 `rustc -vV` 的 host triple 将它复制到 src-tauri/binaries 对应文件，随后运行宿主集成测试。Tauri 构建会将暂存 sidecar 复制回目标目录，旧暂存会覆盖刚构建的 runner；若已发生覆盖，应重新构建 runner 并更新暂存后再运行宿主。正式打包使用同一目标平台的 release runner。通用 native 测试使用 next2-runtime 的 `test-backend` feature。

测试只使用临时数据；正式程序目录不作为开发测试目录。验证中的修改不能当成已交付版本，正式程序仅在门禁通过、进程退出后替换。禁止注册额外系统安装。构建产物和缓存可重建，结束时清理被替代的临时测试程序。

2.1.1 主题平台的协议和帧校验数据由 `npm run generate:next2` 同步生成。中性样例源码位于 `packages/cruciblebox-next2-api/examples/theme-style/renderer.mjs`，不包含业务主题或第三方素材；额外 CSS 使用锁定的 MIT 依赖 css-tree 解析。旧主题、旧界面协议、新样式协商、预览回滚和素材导入分别验证，界面显示与系统缩放最后统一人工验收。新增权限或主题设计字段时同步公开 SDK、契约及说明，不按插件名称分支。

2026-10-27 历史验证记录保存在 artifacts/base-platform-validation。该记录使用 API 6，当前 Next 3.0 候选的实测结果见 [P0–P3 复核](2.2.0-p0-p3-recheck.md)。

P4/P5 的专用宿主测试位于 src-tauri/src/install_next3_services_tests.rs。默认运行文件、下载与日志的真实 QuickJS 调用；Python 和独立模板测试需显式准备资源及 ZIP。先按上文设置环境并构建 runner，然后将 CRUCIBLEBOX_NEXT3_PYTHON_RESOURCE 指向隔离、清单完整的 CPython 资源目录，运行 declared_python_runtime_requires_consent_and_runs_bounded_processes（--ignored）。将 CRUCIBLEBOX_NEXT3_SDK_SAMPLES 指向发布形态 CLI 独立构建的模板 ZIP 目录，运行 independent_sdk_templates_install_and_execute_through_the_public_contract（--ignored）。具体夹具布局见测试中的环境读取及文件名；本轮证据在 .workbench/2.2.0-p4-p5。独立 native 插件的 CARGO_TARGET_DIR 必须留在该插件项目内。

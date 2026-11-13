# 本地交付与发布

2026-11-08 用户要求补完原任务书，包括最后正式交付。本轮先补齐实现、固定验证和最终制品，再执行对应公开发布、稳定渠道与唯一 Portable 更新；只公开宿主及获准分发包，私有源码和构建输入不公开。2.2.0 正式候选仍在验证，不能沿用下文 beta.6 的通过结论。

历史 2.2.0-beta.6 已完成依赖替代、优化构建、程序及交付包扫描、完整小时、用户指定的原生操作回执和解压制品实际安装/调用；本地候选及清单见 [执行记录](2.2.0-p7-p8.md)。原告警对象继续拦截，未取得专家误报复核结论，不恢复或重建该对象。新版本的公开交付按本页首段本轮授权执行，实际制品与未测边界以补完记录为准。

## Next 3.0 冻结条件

先完成 [开发门禁](development.md)、[P7 清单](testing.md)、实际安装/运行/数据恢复演练及必要的人工验收，逐项记录通过、失败或未测。固定最终宿主和 runner 的版本、源码提交与摘要，再在独立仓库构建插件；宿主改变时重新固定并复验相关调用链。

当前执行 Manifest 7、API 7、wire 5、数据 schema 1。公开 API/UI 为 7.0.0，CLI 为 3.0.0，Rust SDK 为 0.2.0。按 [兼容矩阵](2.2.0-p0-p3-recheck.md#版本与兼容策略) 和当前契约逐方法 since 验证最低宿主及能力要求；不能把只使用基础能力的版本要求推广到新增方法。业务候选的数量和适配程度以独立仓库证据为准，不能将旧版七个插件全数写成 Next 3.0 已支持。

## 构建与核验

所有临时目录和缓存遵循 development.md。使用锁定依赖，宿主及 runner 使用一致的 Windows x64 MSVC 目标；先构建 release runner，再暂存到 src-tauri/binaries 对应 triple 文件。设置自定义 CARGO_TARGET_DIR 时从该目录取程序，现有 stage:plugin-host 脚本使用默认 runner target 路径，不能直接假定它会读取自定义目录。

前端构建及九处版本对齐通过后，用正式资源配置构建宿主。隔离数据中执行固定宿主验收，不运行系统安装器或增加本机安装实例。SDK 独立 npm pack/Cargo 包、CLI 模板 init/build/check/pack、包安装及真实调用分别验证。业务源码、构建输入和逐项业务结果留在独立插件仓库。

分发清单至少说明宿主/runner、支持的公开 SDK、独立插件包、必要资源、版本兼容、权限变化、旧执行链停止和数据保留。现有工作流生成 Rust SBOM 与来源证明；npm SBOM 使用 npm run generate:sbom 补齐，完整附件使用 node scripts/generate-release-checksums.mjs <最终制品目录> 生成 SHA256SUMS。最终清单生成后冻结附件。源包摘要通过不能代替二进制安全复核，构建成功也不能代替防护扫描。

本地 Windows x64 SDK 包携带 API/UI/CLI、esbuild 及本机二进制、adm-zip、semver 的七个 tarball、原生 SDK crate、契约及 API 索引。离线锁文件保留 esbuild 所有平台的可选依赖元数据，只将本机依赖指定为随附文件；必须解压到新目录、用空 npm 缓存执行 npm ci --offline --ignore-scripts，再独立生成、构建、测试及打包插件。四份 SBOM 分别命名为 cruciblebox-rust、cruciblebox-plugin-host、cruciblebox-npm、cruciblebox-frontend-npm，避免同名覆盖。附制品 SHA256SUMS、程序身份、来源提交、许可、已知限制和真实验收状态。

现有 Tauri 发布链见 [.github/workflows/tauri-release.yml](../.github/workflows/tauri-release.yml)：tauri-vX.Y.Z[-beta.N|-rc.N] 必须与配置版本一致，预发布与稳定渠道分开。工作流构建 NSIS、Portable、updater 清单及签名、SBOM 和证明并保存草稿 Release。updater 使用 minisign；当前零证书策略下 Authenticode 为 NotSigned，不写成 Windows 受信发行者签名。私钥仅使用受控机密配置，不写入源码、文档或交付包。

## 发布和更新唯一正式目录

用户本轮补完指令已包含新 2.2.0 正式交付；先准备可审阅制品、完整发布说明与真实验证记录，再执行宿主公开推送、PR、合并、标签、Release、稳定渠道和唯一 Portable 更新。插件私有源码及构建输入不公开，不以旧 2.1.1 的历史授权代替本轮范围。Portable 替换仍须等待用户正常退出旧程序。

获得对应授权并通过门禁后，检查草稿附件、下载链接、版本和完整性；发布后从公开下载地址复核实际制品。预发布不改稳定渠道。稳定发布同步 latest.json 与 plugins.json，验证索引和固定版本下载实际一致。签名 updater 的真实安装切换仍须单独验收，不通过本机额外安装实例补测。

2026-11-13 用户已明确允许仅在 GitHub 托管的一次性 Windows 虚拟机运行实际安装器切换。使用 [手动签名切换工作流](../.github/workflows/updater-transition.yml) 和 [具体方案](2.2.0-updater-transition.md)：先准备正式签名草稿，核对来源与附件，再显式开启已批准测试；签名篡改拒绝、有效包安装、新程序启动与旧数据保留全部通过后才发布及切稳定渠道。本机安装禁令不变。

E:\CrucibleBox_Portable 是唯一正式程序目录。替换前请用户正常关闭程序，确认宿主及 worker 均已退出；禁止控制桌面或直接结束正式进程来代替这一步。按 [迁移与恢复](next3-migration.md) 核对重要数据和程序/数据配对，保留用户配置、KV、迁移标记、已安装插件状态与输出。更新后验证实际版本和核心功能，清理被替代的临时程序副本；没有新候选完整验证时不替换正式目录。

## 2.1.1 历史授权边界

用户于 2026-11-05 授权公开发布 2.1.1 并更新本机 Portable，包括七个保留插件的分发制品。该交付不公开插件私有源码或私密构建输入，不恢复已撤下的文档工作台，不附带 OCR/Office 资源。此前未收到的 P7 原生界面、长时会话、正常窗口关闭重开及真实办公软件结果继续标为未测，不能借新文档补写通过。

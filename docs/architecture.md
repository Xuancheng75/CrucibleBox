# 工具箱底座架构

当前源码正在补完原任务书，候选为 2.2.0-beta.7，尚未完成验证或正式交付。beta.6 的既有结果仅是历史本地候选证据，不能据此宣布原任务书全部完成。进度见 [补完记录](2.2.0-completion-audit.md)。

CrucibleBox 2.2.0 的平台架构名称为 **Next 3.0**。架构名称与宿主版本、Manifest/API、wire 版本和 SDK 包名分别管理。开发分支沿用 `next2` 包名及契约目录，运行行为见 [2.2.0 开发记录](2.2.0-platform-progress.md)。

当前源码候选 beta.7 执行 Manifest 7、API 7、wire 5，数据 schema 为 1。历史 P6 基线为 beta.3；beta.6 的 P7 固定宿主及一小时验证和 P8 本地候选交付已完成，原生人工回执范围与未测项目单列。Tauri/Rust 管理安装、生命周期与持久数据，React 界面管理插件导航、市场、设置与任务。各版本的演进规则见 [P0–P3 复核](2.2.0-p0-p3-recheck.md)，历史阶段见 [P4/P5 验证](2.2.0-p4-p5.md)、[P5/P6](2.2.0-p5-p6.md) 与 [P7/P8](2.2.0-p7-p8.md)，本轮实现和实测状态以补完记录为准。

宿主不编译任何插件业务，也不按插件名称选择分发路径。前端插件使用 opaque sandbox iframe 与 MessagePort；JS backend 由独立 QuickJS runner 执行；Windows x64 原生 backend 由通用进程监督器执行。服务由 manifest 的 exports/dependencies 声明；宿主只识别平台方法，业务方法按声明路由。

公共代码位于 packages/cruciblebox-next2-*、contracts/next2 和 src-tauri/crates。通用文件能力、存储、网络、主题、任务、资源包和安装器属于底座。插件处理算法、第三方业务库、业务 worker、模型与插件分发目录由独立插件仓库管理。

内建与插件 Capability 都进入 next2-protocol/gateway.rs 的 Registry，由同一依赖、版本和身份规则解析。内建能力的 Provider ID 为保留的 cruciblebox-host；platform_service.rs 按契约装配方法处理器，不承担另一套能力解析。capability_service.rs 提供调用方能力查询及全 Provider 状态目录；实际实例、故障、资源和启用记录决定状态，发现不授予调用权限。

文件结果写入、复制/移动和校验下载共用既有任务授权与持久发布机制；流式写入绑定同一任务。通用任务、SQLite 与发布日志只由宿主维护，原生插件使用公开 SDK 的 Call.task_context/TaskContext.publish_file，不再嵌入任务框架或创建第二个任务 ID。网络处理在 network_service.rs、download_service.rs 和 websocket_service.rs，文件在 file_operations.rs/file_mutation.rs；进程处理在 process_service.rs，Windows 监督复用 next2-runtime。

共享 Runtime Provider 声明 runtimeCapabilities，消费者声明接口 dependencies、引擎 runtimeRequirements 及执行权限。Registry 分别解析接口、资源包和引擎版本；Provider 持有一份批准资源，消费者不复制。执行仍完整验证实际资源清单和原生批准，插件不能向公共进程 API 传任意 EXE 路径。UI Contributions 由通用工作区加载，Provider 和业务实现仍在独立插件中。

API 6 及更旧插件停止执行，保留数据库记录、配置、KV、迁移标记与用户文件。升级到 Manifest 7 后按稳定插件身份访问原有命名空间。插件负责解释和迁移自己的业务数据。

能力范围见 [plugin-sdk.md](plugin-sdk.md)，开发与验证见 [development.md](development.md)。

自 beta.4 起，在 Tauri/数据库初始化前持有同一 APPDATA profile 的排他文件句柄，进程退出或崩溃后由 OS 释放，防止两套任务与安装状态并行写入。不同 profile 可分别持有各自锁；旧宿主不理解该锁，升级前仍必须关闭旧进程。Windows 不再创建“外观”菜单，主题恢复入口在设置页和独立原生快捷键；2026-11-08 用户确认这几项及正常关闭重开通过。自 beta.5 起页面会话创建使用现有串行数据库锁，避免后台写入造成瞬时 BUSY；beta.6 的 16 页面挂载复验通过。

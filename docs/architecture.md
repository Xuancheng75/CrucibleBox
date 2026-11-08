# 工具箱底座架构

当前源码正在补完原任务书，候选为 2.2.0-beta.7，尚未完成验证或正式交付。beta.6 的既有结果仅是历史本地候选证据，不能据此宣布原任务书全部完成。进度见 [补完记录](2.2.0-completion-audit.md)。

CrucibleBox 2.2.0 的平台架构名称为 **Next 3.0**。架构名称与宿主版本、Manifest/API、wire 版本和 SDK 包名分别管理。开发分支沿用 `next2` 包名及契约目录，运行行为见 [2.2.0 开发记录](2.2.0-platform-progress.md)。

当前固定候选 2.2.0-beta.6 执行 Manifest 7、API 7、wire 5，数据 schema 为 1；P6 基线为 beta.3；P7 固定宿主及一小时验证和 P8 本地候选交付已完成，原生人工回执范围与未测项目单列。Tauri/Rust 管理安装、生命周期与持久数据，React 界面管理插件导航、市场、设置与任务。各版本的演进规则见 [P0–P3 复核](2.2.0-p0-p3-recheck.md)，新增能力见 [P4/P5 验证](2.2.0-p4-p5.md)，独立插件与固定宿主验证见 [P5/P6](2.2.0-p5-p6.md)，当前状态见 [P7/P8](2.2.0-p7-p8.md)。

宿主不编译任何插件业务，也不按插件名称选择分发路径。前端插件使用 opaque sandbox iframe 与 MessagePort；JS backend 由独立 QuickJS runner 执行；Windows x64 原生 backend 由通用进程监督器执行。服务由 manifest 的 exports/dependencies 声明；宿主只识别平台方法，业务方法按声明路由。

公共代码位于 packages/cruciblebox-next2-*、contracts/next2 和 src-tauri/crates。通用文件能力、存储、网络、主题、任务、资源包和安装器属于底座。插件处理算法、第三方业务库、业务 worker、模型与插件分发目录由独立插件仓库管理。

文件结果写入、校验下载和资源工具执行共用既有任务授权与持久发布机制。网络与进程处理分别位于 next2_host/network_service.rs、process_service.rs，Windows 进程监督复用 next2-runtime。运行时按资源和工具声明解析，插件不能向公共进程 API 传任意 EXE 路径。UI Contributions 由通用工作区加载，Provider 和业务实现仍在独立插件中。

API 6 及更旧插件停止执行，保留数据库记录、配置、KV、迁移标记与用户文件。升级到 Manifest 7 后按稳定插件身份访问原有命名空间。插件负责解释和迁移自己的业务数据。

能力范围见 [plugin-sdk.md](plugin-sdk.md)，开发与验证见 [development.md](development.md)。

自 beta.4 起，在 Tauri/数据库初始化前持有同一 APPDATA profile 的排他文件句柄，进程退出或崩溃后由 OS 释放，防止两套任务与安装状态并行写入。不同 profile 可分别持有各自锁；旧宿主不理解该锁，升级前仍必须关闭旧进程。Windows 不再创建“外观”菜单，主题恢复入口在设置页和独立原生快捷键；2026-11-08 用户确认这几项及正常关闭重开通过。自 beta.5 起页面会话创建使用现有串行数据库锁，避免后台写入造成瞬时 BUSY；beta.6 的 16 页面挂载复验通过。

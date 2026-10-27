# 工具箱底座架构

当前执行契约为 Manifest/API 6、wire 4、数据 schema 1；宿主版本为 2.1.0。Tauri/Rust 管理安装、生命周期与持久数据，React 界面管理插件导航、市场、设置与任务。

宿主不编译任何插件业务，也不按插件名称选择分发路径。前端插件使用 opaque sandbox iframe 与 MessagePort；JS backend 由独立 QuickJS runner 执行；Windows x64 原生 backend 由通用进程监督器执行。服务由 manifest 的 exports/dependencies 声明；宿主只识别平台方法，业务方法按声明路由。

公共代码位于 packages/cruciblebox-next2-*、contracts/next2 和 src-tauri/crates。通用文件能力、存储、网络、主题、任务、资源包和安装器属于底座。插件处理算法、第三方业务库、业务 worker、模型与插件分发目录由独立插件仓库管理。

API 5 及更旧插件不安装、不启动。数据库旧记录、原配置、KV、迁移标记与用户文件保留；不再执行按插件名称编写的业务数据搬运。升级到 API 6 后按稳定插件身份恢复原有命名空间；需要解释旧业务数据时由插件的迁移逻辑负责。

能力范围见 [plugin-sdk.md](plugin-sdk.md)，开发与验证见 [development.md](development.md)。

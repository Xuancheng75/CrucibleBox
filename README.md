# CrucibleBox

Windows 10/11 x64 可扩展工具箱底座，基于 Tauri 2、Rust、React 与 SQLite。当前宿主版本 2.1.0；活动插件契约为 Manifest/API 6、wire 4、data schema 1。

底座提供插件安装与管理、隔离界面、JS/原生 backend 生命周期、通用存储与配置、任务、文件引用、网络、主题和资源包。业务功能通过独立插件补充；宿主不包含业务插件源码、业务 worker、模型或按插件名称分支的执行规则。

- [架构](docs/architecture.md)
- [支持的插件与公共能力](docs/plugin-sdk.md)
- [开发与验证](docs/development.md)
- [安全边界](docs/security-model.md)
- [安装与恢复](docs/install-recovery.md)
- [本地交付](docs/release-runbook.md)

公共 SDK/协议及明确列出的通用组件采用 MIT，宿主采用 AGPL-3.0-only，详见 LICENSE-SCOPE.md。旧代码保留 Git 历史；归档资料由独立插件仓库管理。

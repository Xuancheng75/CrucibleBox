# CrucibleBox

当前源码正在补完原任务书，候选为 2.2.0-beta.7，尚未完成验证或正式交付。beta.6 的既有结果仅是历史本地候选证据，不能据此宣布原任务书全部完成。进度见 [补完记录](docs/2.2.0-completion-audit.md)。

Windows 10/11 x64 可扩展工具箱底座，基于 Tauri 2、Rust、React 与 SQLite。Next 3.0 当前固定候选为 2.2.0-beta.6，执行 Manifest 7、API 7、wire 5，数据 schema 为 1。P7 固定宿主验证、一小时 18000 次调用、菜单/主题恢复/正常重开用户回执及 P8 本地候选交付已完成；具体范围和未测项目见执行记录。公开正式版为 2.1.1。

底座提供插件安装与管理、隔离界面、JS/原生 backend 生命周期、通用存储与配置、任务、文件引用、网络、主题和资源包。业务功能通过独立插件补充；宿主不包含业务插件源码、业务 worker、模型或按插件名称分支的执行规则。

- [架构](docs/architecture.md)
- [Next 3.0 版本策略与 P0–P3 复核](docs/2.2.0-p0-p3-recheck.md)
- [P4 公共能力与 P5 开发工具](docs/2.2.0-p4-p5.md)
- [P5/P6 固定宿主验收](docs/2.2.0-p5-p6.md)
- [当前 P7/P8 状态](docs/2.2.0-p7-p8.md)
- [支持的插件与公共能力](docs/plugin-sdk.md)
- [API 索引](docs/plugin-api-reference.md) · [Provider 指南](docs/provider-guide.md)
- [维护交接](docs/ai-maintenance.md) · [测试指南](docs/testing.md) · [数据迁移](docs/next3-migration.md)
- [开发与验证](docs/development.md)
- [安全边界](docs/security-model.md)
- [安装与恢复](docs/install-recovery.md)
- [本地交付](docs/release-runbook.md)

公共 SDK/协议及明确列出的通用组件采用 MIT，宿主采用 AGPL-3.0-only，详见 LICENSE-SCOPE.md。旧代码保留 Git 历史；归档资料由独立插件仓库管理。

# 许可范围

CrucibleBox 宿主自本次修改起采用 **AGPL-3.0-only**，完整文本见 `LICENSE`。原有版权及 MIT 声明保存在 `LICENSES/MIT.txt`；已经发布的旧版本及其既有授权不因本次修改而撤回。

以下公共接口、SDK 和可供插件编译使用的通用组件继续采用 **MIT**：

- `packages/` 下的公共插件 API、UI、构建工具、原生 SDK 和 RPC 包。
- `contracts/` 下的公开协议、schema 与生成接口定义。
- `shared/frame-validation.mjs` 及其类型声明，保留原公共接口的 MIT 许可。
- `src-tauri/crates/task-core`、`task-store`、`task-runtime`、`file-publication`、`next-data`、`next2-protocol`。

这些目录各自的许可文件与第三方声明继续适用。MIT 例外仅涉及上述组件，不自动授予使用宿主其他 AGPL 实现的额外权利。宿主运行时、安装器、界面和业务无关的宿主管理代码仍按 AGPL 提供。

官方业务插件采用独立的专有许可，不属于宿主 AGPL 授权范围。业务插件源码和历史业务实现已迁至 `E:\CrucibleBox_Plugins`，宿主当前树不再保留这些实现。原件与备份完整迁出，Git 历史未重写。该专有声明不撤回历史版本已经授予的 MIT 权利，也不改变插件内第三方组件的许可。

插件与宿主通过公开接口通信；插件不得静态编入仅按 AGPL 提供的宿主实现后仍声称相关代码可闭源。第三方依赖的来源、许可和声明必须单独保留。

本次仅调整当前源码树，保留 Git 历史、分支、标签及原有工作副本；没有清理远程历史或发布新许可版本。

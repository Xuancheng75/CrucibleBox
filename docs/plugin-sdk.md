# CrucibleBox Next 插件 SDK（冻结的 beta.1 契约）

Next beta.1 使用冻结的 Manifest/API 5、wire 3、data 1；SDK 5.0.0-beta.1 与独立构建 CLI 1.0.0-beta.1 的源目录和生成产物均封存。alpha.7 保持不可变作为历史消费者交接快照；正式契约由 contracts/next/contract.json 生成。旧数据与配对回滚保留，旧 v2-v4 runtime 不再执行。

## Next 插件

Manifest 使用 `id`，声明 `manifestVersion: 5`、`sdkApiVersion: 5`、`wireVersion: 3`、`dataSchemaVersion: 1`、`renderer: "dist/renderer.js"` 和精确 `permissions`。可选 backend 是 `"dist/main.js"` 字符串；renderer-only 省略 backend，不能沿用旧协议的布尔值或必需 main 占位规则。renderer 导出 ESM `mount(context)`，从 `createClient(context)` 获取能力；无父窗口或 Node 依赖。

七个官方插件目录以 `scripts/next-plugin-catalog.json` 为准。插件自带 `vendor/next-api` 和 `vendor/plugin-build`，独立执行 clean/build/typecheck/test/pack。两个可独立安装示例在 `templates/next-examples`。宿主只消费清单声明的运行文件；GIF 的 classic worker 是显式资源。构建 CLI 对样式、资源、导入路径和预算失败关闭。

存储 owner 来自宿主会话，不接受插件传入其他 owner；无 SQL 或任意文件系统 API。支持 keys 分页、batch/transact 原子事务和有界大值传输：单值 4 MiB、事务 8 MiB、帧 64 KiB。旧值超预算时拒绝读取而不截断或改写。config.get/patch 保留未知已有键，损坏 JSON 拒绝修改。任务 get/list/cancel 按 owner 校验，晚到事件不能覆盖终态；复杂服务详情与大结果由不可变分块快照读取。

宿主提供 beta.3 appearance 和文件拖入事件；下载独立要求 browser:downloads。主题读取要求 theme:read，预览/提交/回滚要求 theme:write。三个可信服务还要求固定身份、版本、权限及运行文件 SHA-256 完全匹配，安装与每次服务/结果分块调用重新校验。

接口版本及文件白名单通过不可变消费者交接快照封存；源契约已冻结为 beta.1，alpha.6 与 alpha.7 包不可原地重写。

## 旧协议与数据回滚策略

Next 宿主只接受 Manifest/API v5、wire v3；v2–v4 的 renderer/backend 执行路径、旧 SQL RPC 与自动兼容加载已经退出生产分发和启动流程。旧包和 SQLite 原值保留作迁移与诊断材料，不会在 Next 宿主中被静默转换或执行。升级或卸载后的数据保留策略见安装恢复文档；回滚旧程序时必须通过 scripts/next-paired-rollback.mjs 同时恢复匹配的旧程序与数据库。

生产 sidecar 中的 new Function 是 QuickJS 内部用于加载 Next backend CJS 模块的包装器；它不执行插件 renderer，也不恢复旧 v2–v4 运行时。该 loader 只解析安装包内模块，宿主能力仍经 Next wire v3、身份绑定和 PermissionGuard 校验。旧 SQL RPC 名称只保留在被冻结的 legacy 类型/fixture 中，Next host method 集不实现 db.query 或 db.execute，并由权限测试断言拒绝。

旧版 Manifest/API 说明已归档；不要依据旧文档把 v2–v4 字段填入 Next manifest。

## beta.3 插件 UI 组件

packages/cruciblebox-plugin-ui 提供可独立安装的 @cruciblebox/plugin-ui，React 作为 peer dependency，兼容 React 18/19，不依赖 Ant Design 或宿主运行时。组件含 PluginPage、Toolbar、SplitPane、Field、Button、TextInput、FileList、TaskProgress、ResultList、EmptyState 和 ErrorPanel。内置样式使用 beta.3 主题变量；插件可在自身 renderer 中独立构建并携带 UI 包。

Next beta.1 契约冻结与 beta.3 主程序发布验收分开追踪；两者均不声明 OCR 精度门槛已通过。

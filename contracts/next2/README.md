# 活动插件契约

Next 3.0 开发候选只执行 Manifest 7、API 7、wire 5，数据 schema 为 1。来源为 contract.json，generate-next2-contract.mjs 生成 Rust/TypeScript/Schema，`--check` 检查漂移。fixtures.json 用于协议和校验器对照。版本演进见 [P0–P3 复核](../../docs/2.2.0-p0-p3-recheck.md)。

内建方法由公共契约定义。`exports` / `dependencies` 声明能力和版本。2.2.0 开发分支支持方法级 `params` / `result` Schema 和 `contexts`，宿主在直接调用及任务执行路径校验。旧字符串方法和插件自己的 backend 目标延续不透明负载。调用者身份、提供者、任务、文件引用及资源挂载由宿主验证。原生 backend 是 Windows x64 可信本地 EXE，JS backend 使用 QuickJS。详见 [P0/P1 验收](../../docs/2.2.0-p0-p1.md)。

renderer 租期由可信主窗口在页面仍挂载、安装身份及权限仍有效时续签，显式释放后不可恢复。宿主 frame runtime 分配严格递增的 `renderer-N` 上行帧 ID，回应恢复客户端原相关 ID。宿主在最近 64 个编号内允许首次到达的乱序请求，拒绝重放。采用不透明 ID 的会话保留 `sessionRequests` 预算。续期保留请求记录，并受停用、卸载及版本替换约束。

Next 3.0 的事件、取消、任务和流式读取见 [P3 IPC、事件与任务](../../docs/2.2.0-p3.md)。旧执行链已归档，旧数据继续保留。SDK 能力与限制见 [插件能力与开发边界](../../docs/plugin-sdk.md)。

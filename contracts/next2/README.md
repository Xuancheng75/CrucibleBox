# 活动插件契约

当前宿主只执行 Manifest/API 6、wire 4、data schema 1。来源为 contract.json；generate-next2-contract.mjs 生成 Rust/TypeScript/schema，--check 检查漂移。fixtures.json 是中性合成用例，不是业务验收。

内建方法由公共契约定义。`exports` / `dependencies` 声明能力和版本。2.2.0 开发分支支持方法级 `params` / `result` Schema 和 `contexts`，宿主在直接调用及任务执行路径校验。旧字符串方法和插件自己的 backend 目标延续不透明负载。调用者身份、提供者、任务、文件引用及资源挂载由宿主验证。原生 backend 是 Windows x64 可信本地 EXE，JS backend 使用 QuickJS。详见 [P0/P1 验收](../../docs/2.2.0-p0-p1.md)。

renderer 租期由可信主窗口在页面仍挂载、安装身份及权限仍有效时续签，显式释放后不可恢复。宿主 frame runtime 分配严格递增的 `renderer-N` 上行帧 ID，回应恢复客户端原相关 ID；renderer 使用固定大小的序号记录拒绝重放，其他不透明 ID 会话保留 `sessionRequests` 预算。此规则不更改 Manifest/API 6、wire 4 或插件业务接口。正常续期不重新加载页面，不能以续期重置请求记录或跨越停用、卸载及版本替换边界。

旧执行链已归档，旧数据继续保留。SDK 能力与限制见 ../../docs/plugin-sdk.md。

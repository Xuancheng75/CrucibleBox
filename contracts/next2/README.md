# 活动插件契约

当前宿主只执行 Manifest/API 6、wire 4、data schema 1。来源为 contract.json；generate-next2-contract.mjs 生成 Rust/TypeScript/schema，--check 检查漂移。fixtures.json 是中性合成用例，不是业务验收。

平台只枚举公共能力，backend/service 业务方法不透明解析。exports/dependencies 声明服务与版本；调用者身份、提供者、任务、文件引用及资源挂载由宿主验证。原生 backend 是 Windows x64 可信本地 EXE；JS backend 使用 QuickJS。

旧执行链已归档，旧数据继续保留。SDK 能力与限制见 ../../docs/plugin-sdk.md。

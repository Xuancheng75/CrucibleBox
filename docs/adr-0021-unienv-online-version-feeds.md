# ADR-0021：UniEnv 在线版本源（动态版本发现）

状态：已采纳（1.9.12 引入，1.9.13 调整交互）
关联：ADR-0017（UniEnv 中断安装恢复）、`docs/security-model.md`、
`shared/trusted-service-policies.json`

## 背景

UniEnv 的版本目录最初是**编译期固定**的（artifact-integrity.ts / Rust
unienv_catalog.rs 内置版本 + SHA-256）。上游发布新版本后必须等插件更新才能
安装，用户反馈「可下载的最新版不是真正意义上的最新版」。

## 决策

1. **分级开放**：仅上游提供机器可读权威 SHA-256 的工具开放在线新版本：
   - node：dist/index.json + SHASUMS256.txt
   - go：go.dev/dl json（files[].sha256）
   - java：Adoptium API v3（package.checksum）
   python/git 无此类校验源 → 新版本仅提示「待插件更新」，**不放宽为
   「HTTPS 源即信任」**。
2. **非阻塞交互**：listVersions 永远只返回内置目录（秒回）；在线发现由
   `checkOnlineVersions` 消息显式触发（renderer「检查语言新版本」按钮），
   独立线程执行并以 8s 硬超时兜底——ureq 超时不覆盖 DNS 解析挂起，曾致宿主
   响应超过 renderer RPC 30s 上限、版本下拉永久为空。
3. **校验语义不变**：下载后仍按上游声明的 SHA-256 fail-closed 校验；摘要来源
   从编译期常量改为运行期官方端点声明，属显式的产品级取舍（所有端点均为
   HTTPS 官方域，且镜像仅改变下载 URL，摘要始终取自所选优先源的声明文件）。
4. **配置开关**：`onlineVersions`（on/off，默认 on）。关闭时按钮隐藏且
   checkOnlineVersions 返回 online-check-disabled。任何网络失败静默回退内置
   目录。

## 后果

- 新语言版本无需发插件版即可被用户安装（node/go/java）。
- 换来两个新约束：官方端点结构变更需随插件更新适配；镜像可用性影响动态
  安装的下载成功率（静态目录行为不变）。
- trusted-service-policies 的 digest 重钉流程不受影响（仍只钉插件三文件）。

## 参考

- 实现：`src-tauri/src/unienv_versions.rs`、`unienv_service.rs`
  （listVersions / checkOnlineVersions / resolve_version）
- 真机验证记录：node 在线发现 860 版本、26.7.0 动态安装成功（2026-08）

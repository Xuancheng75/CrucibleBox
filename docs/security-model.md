# CrucibleBox 安全模型

> 当前规范（替代 trusted-release.md / adr-0005 / adr-0011 中的安全部分；ADR 原文保留在 docs/ 作决策溯源）。
> 基线：CrucibleBox 1.5.23。仅支持 Windows 10/11 x64。

## 1. 信任模型 A（核心前提）

插件为**可信代码**：权限声明是 SDK 能力门控，**不是安全边界**。安全控制点集中在：

1. 安装时用户确认（显示插件名/版本/作者/描述 + 权限差异）；
2. 渲染隔离（跨源 sandboxed iframe）；
3. 主进程 IPC 校验（可信 sender + 参数校验）；
4. 供应链可追溯（Ed25519 签名 + SBOM + attestation）。

**明确不承诺**：

- OS 级强制沙箱——utility process backend 是故障隔离，不是恶意代码沙箱；
- Windows Authenticode——安装器有意不签名，Windows 显示 Unknown publisher/SmartScreen；
- 权限声明不能防御恶意插件——普通 backend 只允许用户明确安装的可信代码。

## 2. 渲染隔离（renderer）

- 插件 renderer 运行在跨源 sandboxed iframe，唯一 `cruciblebox-plugin://<token>.session` origin（`PluginRendererSessionRegistry` 签发，TTL + owner 绑定）。
- 会话绑定主窗口 owner，index 单次消费；请求 owner 由 Electron `webRequest` 注入的进程内 HMAC 证明（`PluginRendererRequestOwnerProof`），不信任 renderer 自报字段。
- 宿主与 frame 用专用 MessagePort + 严格版本化 RPC（renderer RPC v1）：参数/结果/事件/JSON 预算/请求 ID/64 并发上限均验证。
- 宿主页 CSP：`default-src 'self'`；v2 frame 只允许 self script、禁止 connect；启动断言 `electronAPI`/Node 全局不存在且父 document 不可达。
- 通知与主题写入在宿主 bridge 再检查权限。

## 3. Backend 隔离（utility process）

- backend 统一 `utilityProcess.fork`（Electron 43），无进程内回退。
- worker 只继承最小环境：SystemRoot/临时目录/区域/时区；**不传**完整 PATH、HOME、云凭证。
- backend RPC v2：随机 token（`/^[A-Za-z0-9_-]{32,128}$/`）、requestId、精确方法/字段、JSON 深度/节点/字节预算、64 并发上限。
- 主进程统一权限断言：`PermissionGuard.assert` 覆盖 database/storage/log/notification/dialog/fetch/file/shortcut/event/trusted 全部能力。
- 任意已装 backend 仍属可信代码；Node permission model 不作授权依据。

## 4. 安装与更新安全

- 安装确认：`previewInstall` 读取待装 plugin.json（不解压安装），用户确认后提交同一不可变 stage token。
- zip 限制：≤200MB、条目 ≤5000、解压总量 ≤600MB（`PluginArchivePolicy`）。
- manifest 策略（`PluginManifestPolicy`）：≤256 KiB、严格字段、完整 SemVer、已知且不重复权限、规范化相对 JS 入口、普通文件与根目录 containment。
- 目录复制拒绝 symlink/特殊文件/超限条目；stage/backup/target 必须是插件根目录直接子项。
- 升级/卸载/停用/配置写入由维护租约协调，fail-closed。
- 事件总线按插件隔离：`plugin:${id}:` 前缀；fetch 30s 超时 + ≤50MB。

## 5. UniEnv 可信服务（高信任模型）

- `trusted:unienv` wire 权限 = 宿主固定摘要第一方实现的 `environment.manage` capability；普通 backend 不获得进程/下载/解压/环境修改能力。
- 激活校验（`TrustedServiceRuntime.assertTrustedUniEnvBundle`，fail-closed）：
  - manifest 精确匹配 name/version/`manifestVersion:2`/`backendApiVersion:2`/`main:dist/main.js`/`renderer:dist/renderer.js`；
  - permissions 长度恰为 1 且为 `trusted:unienv`；
  - 固定文件集（`dist/main.js` + `dist/renderer.js` + `plugin.json`）与实际文件集排序后完全一致；
  - `sha256(路径\0内容\0)` 聚合 digest 等于 pinned 值；
  - 文件总字节 ≤5 MiB。
- 任一不符 → 激活被拒；构造失败即抛错；disposed/未激活拒调；未知服务/操作抛错；消息载荷经 RPC 校验。
- 构建门禁 `verify-trusted-services.mjs` 与运行时同算法校验，digest 由 `shared/trusted-service-policies.json` 钉死（1.5.24 后将由 `update-trusted-policy.mjs` 生成）。
- 恢复：激活时只清理直属、非 symlink 的 `.unienv-staging-*` 中断目录，绝不触碰已安装 runtime；恢复/配置校验失败 → 安装/组合/卸载/版本切换全部 fail-closed。

## 6. 发布与供应链

- 插件 Ed25519 签名：正式 `release` 强制验签（仓库外密钥，私钥永不入库）；本地 unsigned sideload 属 Full Trust，不伪装为已认证来源。
- CycloneDX SBOM（宿主 + 六插件 7 份）、`SHA256SUMS`、GitHub artifact attestation。
- Electron fuses：禁用 RunAsNode/NODE_OPTIONS/inspector/file 协议额外权限；启用 cookie 加密、ASAR integrity、only-load-app-from-ASAR、WASM trap。
- 生产宿主页走 `openbox-app://app/index.html`；资源协议只服务 renderer 根内固定静态类型。
- Windows 更新制品校验器：channel 元数据版本/installer basename/blockmap/installer SHA-512 一致，否则 fail-closed。

## 7. 崩溃恢复（可靠性边界，非安全边界）

- per-plugin single-flight；启动/调用超时强制终止并等待真实退出。
- 指数退避 1s/5s/30s；5 分钟内 3 次崩溃 → 隔离并持久化 disabled，用户显式重新启用清除历史。
- 关机取消恢复计时器并等待所有 runtime 退出；维护/停用/卸载优先于自动重启。

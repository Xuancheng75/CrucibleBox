# CrucibleBox 安全模型（Tauri 2 基线，1.9.25）

> 当前规范（替代 trusted-release.md / adr-0005 / adr-0011 中的安全部分；ADR 原文保留在 docs/ 作决策溯源）。
> 运行时基线：Tauri 2.11.x（Rust core + WebView2）+ 插件 Rust sidecar（quickjs-ng）。本文按 1.9.25
> 发布基线维护，仅支持 Windows 10/11 x64。
> Electron 历史安全面（utilityProcess.fork / webRequest HMAC / Electron fuses）见 `docs/electron-legacy-registry.md` 冻结层。

## 1. 信任模型 A（核心前提）

插件为**可信代码**：权限声明是 SDK 能力门控，**不是安全边界**。安全控制点集中在：

1. 安装时用户确认（显示插件名/版本/作者/描述 + 权限差异）；
2. 渲染隔离（跨源 sandboxed iframe）；
3. 宿主 IPC/帧协议校验（窗口身份 + 信封校验）；
4. 供应链可追溯（Ed25519 签名 + SBOM + attestation）。

**明确不承诺**：

- OS 级强制沙箱——sidecar（quickjs-ng 隔离）是故障隔离，不是恶意代码沙箱；
- Windows Authenticode——安装器有意不签名，Windows 显示 Unknown publisher/SmartScreen；
- 权限声明不能防御恶意插件——普通 backend 只允许用户明确安装的可信代码。

## 2. 渲染隔离（renderer）

- 插件 renderer 运行在跨源 sandboxed iframe，唯一 origin（`src-tauri/src/plugin_session.rs` 签发，
  TTL + owner-webview 绑定）。Tauri 自定义协议在 Windows 使用 **path 型**
  `http://cruciblebox-plugin.localhost/<token>/index.html`（PoC 结论：`scheme://` 不受支持）。
- session 绑定主窗口 label（owner），index 单次消费（issued→active）；owner 身份由 Rust core
  的 window label 绑定证明，不信任 renderer 自报字段。
- 宿主与 frame 用专用 MessagePort + 严格版本化 RPC（renderer RPC）：参数/结果/事件/JSON 预算/
  请求 ID/64 并发上限均验证（`shared/plugin-renderer-rpc.ts`）。
- 宿主页 CSP（`tauri.conf.json`）：`default-src 'self'`；协议 handler 附加
  `Cross-Origin-Resource-Policy: cross-origin` + CORS 头。资源路由只允许 session 插件目录内的
  白名单 MIME 普通文件（`src-tauri/src/plugin_protocol.rs` 穿越/symlink 防护）。
- 通知与主题写入在宿主 bridge 再检查权限。

## 3. Backend 隔离（Rust sidecar）

- backend 运行在独立 `cruciblebox-plugin-host` 进程（quickjs-ng 内嵌，无 Node builtin）。
- 传输 = stdin/stdout 长度前缀帧（`frame.rs`，8MB 上限）；信封 v2（`envelope.rs`）：
  随机 token（`/^[A-Za-z0-9_-]{32,128}$/`）、requestId、WORKER_METHODS(4) 白名单、
  HOST_METHODS(19) 白名单、JSON 深度/节点/字节预算、64 并发上限（payload 预算 256KB/深度16/
  数组512/对象键256/字符串64KB）。
- **宿主权限守卫是唯一权威边界**（1.9.2 宿主集成落地，不可省略）：插件可绕过 `__buildCtx`
  直接调 `__hostRequest`，宿主侧必须对每个 host 方法做 PermissionGuard 逐调用校验
  （对等 `PermissionGuard.ts` 语义）；sidecar 侧仅白名单 + 预算最小防护。
- CJS loader 路径防逃逸：相对/绝对路径均须落在 plugin 根内（normalize + 前缀比对）；
  `__cjsLoad` 二次根校验（防任意读盘）。
- 任意已装 backend 仍属可信代码；quickjs 无 Node 能力，隔离目标是"无 Node 运行时面"。

## 4. 安装与更新安全

> 安装事务链当前仍是 Electron 冻结层实现（语义见 `docs/architecture.md` 插件安装章节）；
> Rust 等价随 1.9.2 落地。契约保持不变。

- 安装确认：`previewInstall` 读取待装 plugin.json（不解压安装），用户确认后提交同一不可变 stage token。
- zip 限制：≤200MB、条目 ≤5000、解压总量 ≤600MB（`PluginArchivePolicy`）。
- manifest 策略（`PluginManifestPolicy`）：≤256 KiB、严格字段、完整 SemVer、已知且不重复权限、
  规范化相对 JS 入口、普通文件与根目录 containment。
- 目录复制拒绝 symlink/特殊文件/超限条目；stage/backup/target 必须是插件根目录直接子项。
- 升级/卸载/停用/配置写入由维护租约协调，fail-closed。
- 事件总线按插件隔离：`plugin:${id}:` 前缀；fetch 30s 超时 + ≤50MB。

## 5. UniEnv 可信服务（高信任模型）

- `trusted:unienv` wire 权限 = 宿主固定摘要第一方实现的 `environment.manage` capability；普通
  backend 不获得进程/下载/解压/环境修改能力。
- 激活校验（`TrustedServiceRuntime.assertTrustedUniEnvBundle`，fail-closed）：
  - manifest 精确匹配 name/version/`manifestVersion:2`/`backendApiVersion:2`/`main:dist/main.js`/`renderer:dist/renderer.js`；
  - permissions 长度恰为 1 且为 `trusted:unienv`；
  - 固定文件集（`dist/main.js` + `dist/renderer.js` + `plugin.json`）与实际文件集排序后完全一致；
  - `sha256(路径\0内容\0)` 聚合 digest 等于 pinned 值（`shared/trusted-service-policies.json`）；
  - 文件总字节 ≤5 MiB。
- 任一不符 → 激活被拒；构造失败即抛错；disposed/未激活拒调；未知服务/操作抛错。
- 构建门禁 `verify-trusted-services.mjs` 与运行时同算法校验；digest 由 `npm run update:trusted-policy` 重钉（1.9.0 起插件独立化同样适用）。
- 恢复：激活时只清理直属、非 symlink 的 `.unienv-staging-*` 中断目录，绝不触碰已安装 runtime；
  恢复/配置校验失败 → 安装/组合/卸载/版本切换全部 fail-closed。
- **在线版本源（1.9.12+，ADR-0021）**：node/go/java 允许安装内置目录之外的新版本——
  摘要来源从编译期固定改为运行期官方端点声明（dist SHASUMS256.txt / dl json sha256 /
  Adoptium checksum，均为 HTTPS 官方域），下载后仍 fail-closed 校验；python/git 因无机器
  可读校验源不开放在线新版本。交互非阻塞：`checkOnlineVersions` 显式触发 + 独立线程
  8s 硬超时（覆盖 DNS 解析挂起），`onlineVersions` 配置可整体关闭。

## 6. 发布与供应链

- 插件 Ed25519 签名：正式 `release` 强制验签（仓库外密钥，私钥永不入库；canonical JSON +
  node:crypto Ed25519，`plugin-artifact-provenance.mjs`）；本地 unsigned sideload 属 Full Trust，
  不伪装为已认证来源。
- **Tauri 发布链**（`tauri-release.yml`，tauri-v* tag）：tauri-plugin-updater **minisign 强制签名**
  JSON 清单（`latest.json`，公钥入库 `tauri.conf.json`，私钥 CI secret `TAURI_SIGNING_PRIVATE_KEY`）；
  CycloneDX SBOM（cargo-cyclonedx Rust + 前端/插件 SBOM 双源）；GitHub artifact attestation；
  产物门禁（installer 唯一 + latest.json signature + NotSigned 断言，零证书策略一致）。
- **Electron 发布链**（`release.yml`，v* tag）：冻结中（fuses/ASAR integrity 等 Electron 专属面归档）。
- 生产宿主页走 `tauri.localhost`（tauri-frontend dist，frontendDist）；资源协议只服务白名单。
- 更新：updater 前端 `check()` 已最小接入（1.8.4）；下载/安装 UI 随 1.9.2 前端迁移落地。

## 7. 崩溃恢复（可靠性边界，非安全边界）

- per-plugin single-flight；启动/调用超时强制终止并等待真实退出（sidecar 侧 30s recv_timeout；
  宿主看门狗随 1.9.2 落地）。
- 指数退避 1s/5s/30s；5 分钟内 3 次崩溃 → 隔离并持久化 disabled，用户显式重新启用清除历史。
- 关机取消恢复计时器并等待所有 runtime 退出；维护/停用/卸载优先于自动重启。

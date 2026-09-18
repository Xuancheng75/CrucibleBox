# CrucibleBox 插件 SDK v3

> 当前规范（替代 plugin-sdk-migration.md；模板在 `templates/plugin-template`）。
> 当前契约：Manifest/API v3；宿主继续兼容 Manifest/API v2。v3 renderer 沿用稳定的 v2 帧协议，新增能力声明、最低宿主版本和信任级别。

## 1. Manifest 契约

```jsonc
{
  "name": "<plugin-id>",
  "version": "0.4.11",
  "displayName": "日记",
  "author": "cruciblebox",
  "main": "dist/main.js", // 必须存在；renderer-only 时宿主只校验不加载
  "renderer": "dist/renderer.js",
  "manifestVersion": 3,
  "backendApiVersion": 3, // "backend": false 时可省略
  "rendererApiVersion": 3,
  "minimumHostVersion": "2.1.0-beta.1",
  "trustLevel": "standard",
  "capabilities": {
    "storage": true,
    "network": true,
    "events": true,
    "ui": true
  },
  "permissions": ["storage:read", "storage:write"], // 只声明实际使用的权限
  "config": {}
}
```

- 新插件使用 `manifestVersion: 3`、`backendApiVersion: 3`、`rendererApiVersion: 3`；v2 插件无需修改即可继续运行。
- `minimumHostVersion` 声明最低宿主版本；`minHostVersion` 仅为 v2 兼容保留。
- `trustLevel` 为 `standard` 或 `full`。`full` 自动申请 `host:full-trust`，安装和升级确认页显示高风险提示。
- `capabilities` 支持 `storage/fs/network/process/archive/tasks/events/ui/system/crypto/credentials/pluginData`；已实现能力映射为宿主权限。
- `permissions`：只声明实际使用的权限；升级时预览列出新增/移除权限并二次确认；迁移不删配置/存储/目录。
- `backend: false`（renderer-only）：不创建 utility process，但仍参与启停/配置重启/退出清理/活跃查询；`main` 为必需占位入口；向 renderer-only 发送 backend 消息得确定性错误。
- 旧 v1 兼容：已安装 v1 包由 Legacy Full Trust 适配器运行，但宿主**不再接受**新 v1 安装或升级（1.5.23 起）。
- 版本一致性：`package.json` / `plugin.json` / lockfile 三处版本必须一致（构建门禁强制）。

## 2. Renderer API（自包含 browser IIFE）

- 插件 renderer 为**自包含 browser bundle**：无运行时 `require()`、不读 `window.electronAPI`/父窗口/Node 全局；能力仅来自 `PluginRenderProps.api`。
- `api` 提供：
  - `sendToBackend(message)` / `onBackendMessage(handler)`：与 backend 通信；
  - `notify({ title, body })`：系统通知；
  - `confirm(options)`：异步确认框（不用同步 `window.confirm`）；
  - `theme.get() / theme.set()`：读取/切换主题（`theme:write` 权限门控）。
- 下载用 Blob URL；插件运行在跨源 sandboxed iframe，样式/主题通过 `var(--ob-*)` CSS 变量 + `props.theme` 快照获取。
- 主题契约：`ToolboxTheme { id, name, mode, tokens }`；canvas 场景从 `getComputedStyle` 读取 `--ob-*` 并监听主题变化事件重绘。

## 3. Backend API（utility process）

- 仍导出 `activate(ctx)` / `deactivate()` / `onMessage(handler)`。
- 全部能力走**异步 SDK**（`Promise`）：`ctx.database.query/execute`、`ctx.storage.get/set/delete/list/batch`、`ctx.logger`、`ctx.api.*`（notify/dialog/fetch/readFile/writeFile/registerShortcut/onEvent/emitEvent/invokeTrustedService）。
- 长任务：立即返回 taskId，由宿主事件推送进度和终态；重连时读取一次任务快照。
- v3 增加 `ctx.pluginData`（插件私有存储别名）以及 `ctx.capabilities.events/system` 分组入口；v2 的 `ctx.storage`、`ctx.api` 保持可用。
- `ctx.api.fetch`：30s 超时、响应 ≤50MB；`ctx.api.registerShortcut`：全局快捷键（`Permission.Shortcut`）。
- 权限在**主进程统一断言**（`PermissionGuard`），子进程侧为 RPC 代理。
- 生命周期纪律：
  - `activate` 幂等可重启；失败抛明确错误；**activate 内不做不可回滚的数据修改**；
  - `deactivate` 停止计时器/订阅/快捷键/后台任务；
  - 意外退出由宿主崩溃恢复策略接管（指数退避/隔离）。

## 4. 私有存储 API

- `storage.get(key)` / `set(key, value)` / `delete(key)` / `list(prefix?)` / `batch(mutations)`。
- 约束：key ≤256 字符、拒绝控制字符；值必须有限无环 JSON、单值 ≤1 MiB；namespace 由宿主按插件 ID 绑定。
- `batch`：1–64 个严格 JSON set/delete，全部预校验后在宿主 `BEGIN IMMEDIATE` 中提交。
- **无跨键事务**：多字段原子更新存为单个 JSON 文档。
- 不直接执行 SQL；`database:*` 权限仅旧插件（v1 兼容期）保留，新插件不申请。

## 5. 权限清单（Permission 枚举）

Manifest v3 优先使用 `capabilities`，`permissions` 用于精确补充和 v2 兼容。

`database:read/write`（旧）、`storage:read/write`、`shell:exec`、`network:fetch`、`notification`、`clipboard`、`dialog`、`shortcut`、`file:read/write`、`theme:write`、三个第一方可信服务权限，以及 `host:full-trust`。

> 高权限能力（进程/下载/解压/环境修改）**不扩大通用插件能力**：应设计宿主持有的固定服务 + 操作白名单 + 输入协议 + 资源预算 + 摘要策略（UniEnv 即此模式）。

### 6.1 UniEnv 支持的工具与版本源（1.9.12+）

- 内置工具：Python、Node.js、Git、Go、Java (Temurin)、**Rust (rustup stable)**、**PHP (NTS x64 zip)**。
- 版本目录：编译期固定 + SHA-256 fail-closed；**node/go/java** 另支持在线发现新版本
  （官方端点权威摘要校验，交互与安全边界见 `adr-0021-unienv-online-version-feeds.md`；
  配置项 `onlineVersions` 默认开启，可关闭）。
- python/git 暂不支持在线新版本（无机器可读校验源），新版本需随插件更新。

## 6. 类型与构建

- API 类型唯一事实源：`packages/cruciblebox-plugin-api`（1.5.25 落地；此前各插件本地 `openbox-api.d.ts`）。
- 模板提供双 API v2 构建：backend 用 esbuild 独立 main CJS，renderer 用 esbuild browser IIFE。
- renderer 外部化 `react` + `cruciblebox-plugin-api`；产物进 `dist/`（`dist/main.js` + `dist/renderer.js`）。
- 当前 11 个正式插件均声明双 v2；插件目录与版本以 `scripts/plugin-catalog.json` 为准。

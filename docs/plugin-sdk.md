# CrucibleBox 插件 SDK v2

> 当前规范（替代 plugin-sdk-migration.md；模板在 `templates/plugin-template`）。
> 契约版本：Manifest v2（`manifestVersion: 2`）、backend API v2（`backendApiVersion: 2`）、renderer API v2（`rendererApiVersion: 2`）。
>
> **SDK v2 已冻结（1.7.0 起）**：本契约不再原位修改。任何 API 变更必须作为 **v3 提案**处理——
> 在 `packages/openbox-plugin-api` 发布新的 major 版本（`3.0.0`），升级模板与全部插件声明，
> 并同步本文档的契约版本。CI 通过 6 插件对冻结类型的构建兼容矩阵（`typecheck:plugins`）强制约束。

## 1. Manifest 契约

```jsonc
{
  "name": "<plugin-id>",
  "version": "0.4.11",
  "displayName": "日记",
  "author": "openbox",
  "main": "dist/main.js", // 必须存在；renderer-only 时宿主只校验不加载
  "renderer": "dist/renderer.js",
  "manifestVersion": 2,
  "backendApiVersion": 2, // "backend": false 时可省略
  "rendererApiVersion": 2,
  "permissions": ["storage:read", "storage:write"], // 只声明实际使用的权限
  "config": {}
}
```

- 必填：`manifestVersion: 2`、`backendApiVersion: 2`、`rendererApiVersion: 2`；未知版本关闭失败。
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
- 长任务：立即返回 taskId，由 renderer 轮询或显式取消；不阻塞 RPC。
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

`database:read/write`（旧）、`storage:read/write`、`shell:exec`（旧，未授予新插件）、`network:fetch`、`notification`、`clipboard`、`dialog`、`shortcut`、`file:read/write`、`theme:write`、`trusted:unienv`（仅宿主固定摘要第一方实现）。

> 高权限能力（进程/下载/解压/环境修改）**不扩大通用插件能力**：应设计宿主持有的固定服务 + 操作白名单 + 输入协议 + 资源预算 + 摘要策略（UniEnv 即此模式）。

## 6. 类型与构建

- API 类型唯一事实源：`packages/openbox-plugin-api`（1.5.25 落地；此前各插件本地 `openbox-api.d.ts`）。
- 模板提供双 API v2 构建：backend 用 esbuild 独立 main CJS，renderer 用 esbuild browser IIFE。
- renderer 外部化 `react` + `openbox-plugin-api`；产物进 `dist/`（`dist/main.js` + `dist/renderer.js`）。
- 六个生产插件（Diary/DiceRoller/GIF Editor/ThemeManager/Turntable/UniEnv）均声明双 v2。

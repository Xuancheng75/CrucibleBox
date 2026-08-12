# ADR-0004：插件渲染器跨源隔离

- 状态：已接受
- 日期：2026-08-10
- 里程碑：M2.2B

## 背景

旧版 `PluginHost` 在宿主 React document 中通过 `new Function` 执行插件代码。传入 Proxy 不能形成安全边界：插件仍可经 `document.defaultView`、构造器链或同一全局对象取回宿主 `window.electronAPI`，从而绕过 renderer API 和权限检查。BrowserWindow 的 Chromium sandbox 不能隔离同一 document 内的两段 JavaScript。

## 决策

- 每次打开插件创建一个不可预测、单次消费的 `openbox-plugin://<token>.session` 会话和唯一 origin。
- 插件界面运行在跨源 sandboxed iframe。v2 只启用 `allow-scripts allow-same-origin allow-downloads`；迁移期 v1 额外保留 `allow-modals`，只用于旧同步 `confirm` 兼容。
- 宿主与 iframe 只通过一次性 handshake token 和专用 `MessageChannel` 通信；不向 frame 暴露 preload、Electron IPC、Node 全局或插件 ID。
- `openbox-plugin` 协议要求主窗口 `webContentsId` 的进程内 HMAC 证明。会话绑定 owner、有限时、index 只能消费一次，资源请求只有激活后才可读取。
- renderer RPC 固定为版本 1 envelope，采用严格字段、方法特定参数/结果校验、JSON 深度/节点/字节预算、请求 ID 去重和 64 个并发请求上限。
- v2 插件 renderer 必须是自包含 browser IIFE，显式注册 `window.__OPENBOX_PLUGIN_RUNTIME__.mount(...)`，产物禁止 CommonJS `require`、ESM `import`、`eval` 和 `new Function`。
- 宿主页 CSP 删除 `unsafe-eval` 以及 `plugin:` script/connect，只允许 `frame-src openbox-plugin:`；插件 frame 使用独立响应头 CSP。v2 frame 的 `script-src` 只有 `'self'`、`connect-src 'none'`。

## 能力面

允许的 renderer RPC 为：

- `backend.send`
- `notification.show`
- `config.update`
- `theme.get` / `theme.set`
- `dialog.confirm`
- `layout.resize`

宿主事件为初始化、配置变化、主题变化、backend 消息和销毁。通知与主题写入在宿主 bridge 再次检查 manifest 权限；backend 的数据库、文件、网络和进程能力仍由主进程侧现有守卫决定。

## 兼容性

- manifest 缺少 `rendererApiVersion` 时按 v1 运行。旧 CJS renderer 只在隔离 iframe 内使用兼容 loader，宿主页不再执行 `new Function`。
- 六个生产插件均迁为 `rendererApiVersion: 2` 并升级版本；旧数据、插件 ID、配置、数据库 schema 不迁移。
- v1 是限时迁移通道，后续 SDK v2 稳定后应设弃用窗口并最终删除 iframe 内的 `unsafe-eval` 与 `allow-modals`。

## 后果

- 插件 renderer 无法读取或修改宿主 DOM、zustand store、其他插件 iframe 或 `window.electronAPI`。
- React/ReactDOM 被打入每个 v2 renderer，单插件产物增大；这是换取 origin 隔离和独立可构建性的明确成本。后续可用受控共享静态依赖优化，但不能重新合并执行上下文。
- backend 当前仍是可信 Node 代码。renderer 隔离不等于 backend 强制能力沙箱；后者由 M2.3 单独处理。

## 验证

- 纯函数与协议测试覆盖会话生命周期、owner 绑定、路径穿越、CSP、RPC schema、payload budget、权限拒绝和 MessagePort bridge。
- 静态产物验证覆盖六个 v2 renderer。
- packaged Electron 冒烟在临时 userData 安装 Dice 插件，验证唯一 origin、跨源 DOM 阻断、真实协议请求、MessagePort 初始化、renderer mount 和 resize RPC。

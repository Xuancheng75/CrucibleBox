# 插件平台 M2.2B：renderer 隔离与 renderer API v2

- 状态：已完成
- 日期：2026-08-10

## 数据流

```text
Host React main frame
  -> trusted main-frame IPC: createRendererSession(pluginId)
  -> openbox-plugin://<random>.session/index.html
  -> sandboxed cross-origin iframe
  -> transferred MessagePort + handshake token
  -> strict renderer RPC
  -> host capability adapter
  -> existing plugin backend RPC / theme / config / notification
```

插件 frame 不获得 Electron preload。主进程自定义协议不会相信请求携带的普通 owner 字段，而是由 Electron `webRequest` 根据真实 `webContentsId` 注入进程内 HMAC 证明，再由协议处理器验证。

## renderer API v2 构建契约

manifest 增加：

```json
{
  "rendererApiVersion": 2,
  "renderer": "dist/renderer.js"
}
```

入口使用宿主 frame runtime 提供的 mount 注册点：

```ts
window.__OPENBOX_PLUGIN_RUNTIME__.mount((container, initialProps, subscribeProps) => {
  const root = createRoot(container)
  const render = (props) => root.render(<Plugin {...props} />)
  render(initialProps)
  const unsubscribe = subscribeProps(render)
  return () => {
    unsubscribe()
    root.unmount()
  }
})
```

renderer 必须以 browser IIFE 自包含 React/ReactDOM。根脚本 `scripts/build-plugin-renderer.mjs` 统一构建，`scripts/verify-plugin-renderers.mjs` 在正式 build 中拒绝残留 `require`、`import`、动态执行或缺失 mount 标记的产物。

## 六插件迁移

| 插件          |  版本 | 特别处理                                                                        |
| ------------- | ----: | ------------------------------------------------------------------------------- |
| Diary         | 0.4.6 | KaTeX 字体内联为 data URL；保留 Blob 导出                                       |
| Dice Roller   | 0.1.2 | 新增 browser renderer entry                                                     |
| GIF Editor    | 0.3.3 | 自包含编辑器 renderer；保留 Blob 下载                                           |
| Theme Manager | 0.1.7 | 主题导入导出继续工作                                                            |
| Turntable     | 0.1.5 | 通知能力写入 manifest；canvas 主题逻辑保留                                      |
| UniEnv        | 0.4.1 | `renderer-task` 被打入单一 bundle；两处 `window.confirm` 迁为异步 `api.confirm` |

六份本地 `openbox-api.d.ts` 均包含受控 `api.confirm`。版本升级不改变配置 schema 或插件数据表。

## 安全边界

- 宿主 IPC 只接受 BrowserWindow main frame，且 URL 必须是生产 `out/renderer/index.html` 或开发服务器 origin。
- session token 与 handshake token 均为 256-bit 随机值；session 绑定 owner、TTL 和不可变入口元数据。
- 协议只允许 GET、规范化路径和白名单 MIME；拒绝 symlink、越界、超大或读取期间变化的文件。
- 每个 frame 响应带 CSP、Permissions-Policy、no-referrer、nosniff 和 no-store。
- frame runtime 启动时主动断言没有 `electronAPI`、`process`、`require`，并确认父 document 跨源不可达。

renderer 侧现在形成强 Web 隔离边界。backend 仍按“已安装插件为可信代码”的现状运行，不能把本阶段描述为恶意 Node 插件沙箱；M2.3 将把 UniEnv 特权安装服务与普通插件 backend 拆开。

## 验收

- host：18 个测试文件，136 项通过。
- plugins：GIF Editor 35 项、UniEnv 116 项通过。
- 六插件 typecheck、clean build、自包含 renderer 静态验证通过。
- 根 typecheck、ESLint、Prettier、electron-vite production build 通过。
- Windows x64 unpacked package 成功；frame runtime 位于受控的
  `app.asar.unpacked/out/plugin-frame/runtime.js`，避免 ASAR 虚拟文件破坏协议层的 fd/inode 读取校验。
- packaged Electron 在临时 userData 中完成真实 v2 插件跨源隔离冒烟，真实用户数据未读取或修改。

## 后续入口

M2.3 只处理 backend/SDK 能力边界：普通插件迁 `utilityProcess` 仅作为故障隔离与纵深防御；UniEnv 拆成普通编排插件和宿主持有的受信安装服务；不把 Node permission model 宣称为恶意代码安全边界。

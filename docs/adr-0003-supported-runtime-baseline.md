# ADR-0003：受支持的 Electron 运行时基线

- 状态：已接受
- 日期：2026-08-10
- 里程碑：M2.2A

## 背景

宿主原先锁定 Electron 34.5.8。Electron 官方已于 2025-06-24 结束 34 系列支持，继续发布会失去 Chromium 与 Electron 的安全修复。Renderer 强隔离还依赖新版 Chromium 的 sandbox、CSP 和 MessagePort 行为，因此必须先建立受支持的运行时基线，避免在过时平台上实现并随后返工。

## 决策

- Electron 精确锁定为 43.3.0，这是 2026-08-10 的最新稳定版；不采用仍处于 beta 的 44 系列。
- electron-vite 精确锁定为 5.0.0，Vite 精确锁定为 6.4.3；不在同一里程碑迁移 Vite 大版本。
- electron-builder 精确锁定为 26.15.7，继续使用 `@electron/rebuild` 4.2.0 和 better-sqlite3 12.11.1。
- 主窗口启用 Chromium renderer sandbox，保留 `contextIsolation: true`、`nodeIntegration: false`，并显式禁用 subframe Node integration。
- 主窗口导航、popup 与 Chromium permission 均采用 fail-closed 策略；外部链接只允许
  无凭据的 HTTP(S) URL，并始终交给系统浏览器而非应用 renderer。
- sandboxed preload 必须完整打包，不把 npm 依赖 externalize 到 renderer 进程。
- 移除固定的 `electronDist` 构建配置。Electron 42 以后 npm 依赖安装不再自动下载运行时；正式打包由 electron-builder 根据锁定版本获取分发包。本地离线验收可通过 CLI 临时指定已经过校验的 `node_modules/electron/dist`，但该路径不写入发布配置。
- better-sqlite3 必须按 Electron ABI 148 重建，并由 Electron 进程执行真实 WAL 写入/查询探针。

## 后果

- 发布基线由 Electron 34 / Node 20 / ABI 132 提升为 Electron 43.3.0 / Node 24.18.1 / ABI 148。
- 开发机首次运行 Electron 需要显式获取 Electron 分发包；clean package 不再依赖 `node_modules/electron/dist` 的偶然存在。
- 主 renderer 已处于 OS sandbox，但同 document 执行插件 renderer 的旧加载器仍不是安全边界。该问题必须由 M2.2B 的跨源 iframe 与 MessagePort 隔离解决。
- Electron 43 的文件对话框在未给 `defaultPath` 时从 Downloads 或 home 开始；当前接受该上游行为，后续产品体验里程碑可持久化最后目录。

## 验证

- `npm run check`
- `npm run build`
- `npm run smoke:native`
- `npm run package:dir`
- `npm run smoke:packaged`

packaged smoke 除了数据库和页面加载，还必须确认 preload bridge 存在，且 renderer main world 中没有 Node `process` 或 `require`。

## 依据

- [Electron 43.3.0 release](https://releases.electronjs.org/release/v43.3.0)
- [Electron release schedule](https://releases.electronjs.org/schedule)
- [Electron support policy](https://www.electronjs.org/docs/latest/tutorial/electron-timelines)
- [Electron sandbox documentation](https://www.electronjs.org/docs/latest/tutorial/sandbox/)
- [electron-vite migration guide](https://electron-vite.org/guide/migration)
- [electron-builder 26.15.7](https://github.com/electron-userland/electron-builder/releases/tag/electron-builder%4026.15.7)

# CrucibleBox Tauri 迁移 PoC 报告（1.8.0）

> 状态：2026-08-14，Windows 10/11 x64 实测。
> 结论：**GO（有条件）**——插件隔离（P1）与内存量级（P4）均支持继续迁移；1.8.1 起按 `docs/tauri-migration-plan.md` 推进。

## 1. PoC 项与结果

| # | PoC 项 | 结果 | 证据 |
|---|---|---|---|
| P1 | 插件 iframe + 自定义协议 + MessagePort RPC（Windows） | ✅ **PASS** | `http://openbox-plugin.localhost/plugin/index.html` 经自定义协议 handler 加载成功；子资源 `plugin.js` 同协议加载并执行；跨源 postMessage 双向往返；`contentDocument=null`（隔离生效）；**#11505 未命中** |
| P2 | sidecar 独立进程 + 帧协议 | ⏳ 未做（1.8.2） | 计划内，lib-1 确认 sidecar 为独立子进程 |
| P3 | rusqlite 打开现有 v3 openbox.db | ⏳ 未做（1.8.1） | 计划内 |
| P4 | 内存 A/B 基准 | ✅ **量级明确** | 见 §2 |

### P1 细节（PoC 工程 `poc-tauri/`，已合入 main）

- 宿主页在 `http://tauri.localhost`（生产 origin）；iframe 指向 `http://openbox-plugin.localhost/plugin/index.html`（Windows 上 Tauri 自定义协议的 origin 形式）。
- Rust 侧 `register_uri_scheme_protocol("openbox-plugin", ...)` 处理 index.html 与 plugin.js 两个资源，响应带 `Cross-Origin-Resource-Policy: cross-origin` + `Access-Control-Allow-Origin: *`。
- 前端 report_status 命令把检测结果写日志，证据链：`[protocol-request] openbox-plugin://localhost/plugin/index.html` → 子资源请求 → `plugin-subresource`/`plugin-reply` 消息往返 → `contentDocument=null`。
- **适配点**：Electron 的 `openbox-plugin://<token>` 在 Windows 映射为 `http://<scheme>.localhost`；`scheme://` 形式 fetch 被 WebView2 拒绝（预期，迁移时按 http.localhost 形式改造）。

## 2. P4 内存基准（同机实测，2026-08-14）

| 框架 | 采样 | Working Set |
|---|---|---|
| **Tauri 2 骨架**（debug，空壳+前端） | Rust core 进程 | **36.4 MB**（Private 4.2 MB） |
| | WebView2 进程组（与 Edge 共享） | 73.9 / 126.3 / 39.3 MB |
| **Electron 43 dev**（完整应用） | 8 个进程合计 | **701.1 MB**（Private 417.4 MB） |

**口径说明（诚实标注）**：
- Tauri 侧为 debug 骨架，未加载插件系统/数据库；Electron 侧为完整 dev 应用（含 6 插件 runtime、DB、IPC）。二者非严格同构，但量级差异显著。
- WebView2 与 Edge 共享进程组（同 user data folder），Tauri 侧"真实新增"内存应以 core 进程 + 应用专属 webview 进程计，需 1.8.1 起用 release 打包 + 进程树归属精确测量。
- **方向性结论**：Rust core 进程远轻于 Electron 多进程（≈1/19），迁移后内存占用有明确下降空间；正式数字待 1.8.4 打包版复测。

## 3. 结论与后续

- **GO**：P1（插件隔离，命门）与 P4（内存量级）均通过；#11505 未命中，自定义协议以 `http://<scheme>.localhost` 形式可行。
- **条件**：P3（rusqlite 迁移）与 P2（sidecar）须在 1.8.1/1.8.2 独立验证；命名与 L3 数据路径迁移按计划并入。
- 1.8.1 起：主进程/DB/IPC → Rust（rusqlite + tauri commands）+ `%APPDATA%\openbox → cruciblebox` 数据路径搬迁；1.8.4 打包版复测内存后发布首个 Tauri 正式版。

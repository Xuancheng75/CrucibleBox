# CrucibleBox

一个基于 **Tauri 2 + Rust** 的可扩展工具箱桌面应用（Windows 10/11 x64），支持插件系统、主题系统与全局快捷键。

> **运行线**：Tauri 2.11.x（Rust core + WebView2）。Electron 43 历史运行线已冻结
> （tag `electron-1.7.3-production`，`docs/electron-legacy-registry.md` 为逐文件映射）。
> 迁移复盘：`docs/tauri-migration-review.md`。

## 功能特性

- **插件系统**：通过插件包（`.zip`）或插件目录导入，backend 运行于独立 **Rust sidecar**（quickjs-ng），安装时确认
- **权限模型**：每位插件声明所需权限（网络、文件、数据库、剪贴板、通知、快捷键等），由宿主侧 `PermissionGuard` 逐调用门控；插件为可信代码，权限声明用于功能控制
- **命令系统**：插件可注册全局命令，通过 `Cmd/Ctrl+Shift+P` 唤起
- **全局快捷键**：插件可注册系统级快捷键（如 `Cmd/Ctrl+I` 唤起插件导入）
- **主题系统**：内置 14 套明暗、护眼、极简、暖色与赛博风格预设，运行时切换并下发 CSS 变量，插件可实时感知主题变化
- **插件排序**：普通模式长按卡片约半秒拖动排序，或使用卡片操作区的上移/下移按钮（键盘可操作）；批量管理模式支持像手机多选应用一样拖动整组插件；顺序持久化于数据库 schema v3 `plugins.sort_order`，插件激活顺序跟随列表
- **批量插件管理**：主页批量管理支持批量启用、批量禁用、批量删除和多选组拖拽，操作按插件串行执行并汇总失败项
- **生命周期恢复**：导入、升级、启停、卸载和崩溃恢复按插件单飞，目录替换使用可恢复事务，避免频繁操作导致进程或会话残留
- **更新检查**：更新请求有超时、网络抖动重试和状态复位；更新服务暂时不可达时不会无限加载
- **插件日志**：日志入库（`plugin_logs` 表）并支持按插件、级别筛选与实时刷新
- **自定义协议**：`cruciblebox-plugin://`（Windows path 型 `http://cruciblebox-plugin.localhost/<token>/`）安全地服务插件 renderer 静态资源（内置路径穿越防护 + MIME 白名单）
- **配置中心**：`settings` 表持久化应用配置（rusqlite bundled WAL）

## 技术栈

- **Tauri 2.11.x**（Rust core：rusqlite / sidecar 进程管理 / renderer 会话）+ WebView2
- React 18 + Ant Design 5 + zustand（`tauri-frontend/`）；根目录 Electron 冻结线继续使用 React 19 + Ant Design 6
- **rusqlite（bundled SQLite 3.53.x）**——与 better-sqlite3 文件格式零迁移兼容
- 插件 backend：**quickjs-ng**（Rust sidecar，独立进程 + 帧协议）
- Vitest 单元测试（Electron 冻结线）/ **cargo test + clippy + fmt**（Tauri 线）

## 快速开始

```bash
# Tauri 线（当前发布线 1.9.24）
cd src-tauri && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cd tauri-frontend && npm install && npm run build
npm run build:frame              # 插件 frame runtime（out/plugin-frame/runtime.js）

# 插件（独立自包含工程，1.9.0 起）
cd plugins/<id> && npm run clean && npm run build

# 发布链（Electron 冻结线脚本仍可用作插件打包/签名参考）
npm run package:plugins && npm run generate:sbom
```

## 插件开发

插件是一个包含 `plugin.json` 清单的目录或压缩包，**自包含工程**（独立 build/test/发布，宿主只消费 dist）。结构示例：

```jsonc
{
  "name": "demo",
  "version": "1.0.0",
  "displayName": "示例插件",
  "description": "一个示例插件",
  "author": "cruciblebox",
  "main": "dist/main.js",
  "renderer": "dist/renderer.js",
  "manifestVersion": 2,
  "backendApiVersion": 2,
  "rendererApiVersion": 2,
  "permissions": ["storage:read", "storage:write", "notification"],
  "config": {
    "greeting": { "label": "问候语", "type": "string", "default": "你好" }
  }
}
```

- `main`：backend 插件入口（CommonJS，导出 `activate(ctx)/deactivate()/onMessage()`），由 **Rust sidecar**（quickjs-ng，无 Node builtin）加载，经 stdin/stdout 帧协议 + 信封 v2 与宿主通信
- `renderer`：自包含 browser renderer（IIFE，React 内联），在跨源 sandboxed iframe + MessagePort RPC 中运行
- **信任模型**：backend 是**可信代码**；sidecar 进程 + 最小能力面 + 宿主侧 PermissionGuard 提供故障隔离与纵深控制，但权限声明不是恶意代码安全边界。安装插件时会弹出确认框展示插件名/版本/作者，由你确认其可信后才写入。
- 新插件使用 `ctx.storage` 与 `storage:read/write`；`database:read/write` 只用于旧 SDK 兼容
- `file:read/write` 表示整个文件系统能力，请谨慎授予
- 再次导入更高版本的同一插件即可完成更新（保留其 ID、配置与启用状态）
- **脚手架/版本/签名**：`cli/bin/openbox` —— `create-plugin` / `bump` / `sign`（Ed25519）

## 目录结构

```
src-tauri/                 # Tauri 主进程（Rust）
  src/main.rs              # 装配点（updater/协议/DB/L3 迁移/命令注册/退出清理）
  src/commands.rs          # IPC 命令组（settings/app/plugin 读写/session/日志）
  src/db.rs                # rusqlite 引擎 + v1-v3 迁移 + storage/log 读写层
  src/backend_process.rs   # 插件 backend sidecar 管理器（spawn/崩溃恢复/权限）
  src/envelope_host.rs     # host 方法分发（storage/log/db）
  src/permissions.rs       # PermissionGuard（15 权限）
  src/plugin_session.rs    # renderer session registry
  src/plugin_protocol.rs   # cruciblebox-plugin 协议 handler
  cruciblebox-plugin-host/ # 插件 backend sidecar crate（quickjs-ng）
tauri-frontend/            # React 渲染层（App / PluginHost / themeCache）
plugins/                   # 11 个正式插件（自包含工程）
shared/                    # 跨进程共享契约（types / themes / RPC）
electron/ database/ plugin-system/   # Electron 冻结线（1.7.3，只读参照）
```

## 插件 backend（Rust sidecar）

生产插件声明 `backendApiVersion: 2`。backend 运行于独立 `cruciblebox-plugin-host` 进程
（quickjs-ng 内嵌，纯 JS + 宿主注入 ctx），经 stdin/stdout 长度前缀帧 + 信封 v2
（token/requestId/预算/方法白名单）与宿主通信。宿主侧对每个 host 方法做 PermissionGuard
逐调用校验（storage/log/db 已实现；dialog/network/file/shortcut/trusted 暂回 NOT_ALLOWED）。
UniEnv 的进程、文件、下载和解压实现属于宿主固定摘要可信服务（`trusted-service-policies.json`，
digest 钉死），发布插件只包含受限代理。架构、兼容规则和构建方式见 `docs/security-model.md`
与 `docs/plugin-sdk.md`。

## 插件渲染隔离

生产插件使用 `rendererApiVersion: 2`。每次打开插件时，宿主签发唯一 origin（Windows path 型
`http://cruciblebox-plugin.localhost/<token>/index.html`），在 sandboxed iframe 中加载自包含
browser renderer，并通过受校验的 MessagePort RPC 提供配置、主题、通知和 backend 消息能力。
插件 frame 无 Node/Rust 访问能力，不能访问宿主 DOM 或宿主进程面。契约与构建说明见
`docs/plugin-sdk.md` 与 `docs/security-model.md`。

## 发布与诊断

- **Tauri 发布链**（`tauri-release.yml`，`tauri-v*` tag）：NSIS 安装器（WebView2
  downloadBootstrapper 兜底）+ tauri-plugin-updater（minisign 强制签名 JSON）+ cargo-cyclonedx
  Rust SBOM + GitHub artifact attestation。首个 Tauri 正式版为 **v1.9.2**；当前发布版本与
  验证基线为 **v1.9.24**（详见下方“当前验证基线”）。
- 插件发布会生成确定性 ZIP、逐文件 SHA-256 清单，并支持仓库外 Ed25519 密钥的强制签名验签；
  宿主和 11 个正式插件可生成 CycloneDX SBOM。
- 运行时在 `%APPDATA%\cruciblebox\logs` 写诊断信息（进程内存探针已于 1.9.3 移除）。
- 完整发布环境变量和验收步骤见 `docs/release-runbook.md`。

## 架构与文档

- 当前模块、进程、数据流和信任边界：`docs/architecture.md`（Tauri 2 基线）
- 安全模型与信任边界：`docs/security-model.md`（Tauri 基线）
- 插件 SDK v2 契约与私有存储：`docs/plugin-sdk.md`
- 安装事务与崩溃恢复：`docs/install-recovery.md`
- 发布与自动更新 runbook：`docs/release-runbook.md`
- Tauri 迁移计划与复盘：`docs/tauri-migration-plan.md` / `docs/tauri-migration-review.md`
- Electron 冻结层逐文件映射：`docs/electron-legacy-registry.md`

## Windows releases and automatic updates

GitHub publishing is optional. `tauri build` produces a standalone Windows x64 NSIS installer.
Online updates use tauri-plugin-updater (minisign-signed `latest.json`); the updater frontend
`check()` is wired in `tauri-frontend`. Repository owners enable GitHub Releases by pushing a
`tauri-v*` tag (see `docs/release-runbook.md`).

CrucibleBox supports Windows 10/11 x64. Stable and beta releases publish NSIS installer +
updater JSON + plugin signatures + CycloneDX SBOMs + SHA-256 checksums + GitHub provenance
attestation. Windows installers are currently unsigned and can display Unknown publisher or
SmartScreen warnings.

## 当前验证基线（1.9.24）

- Tauri 线：`cargo test --workspace --locked`、`cargo clippy --workspace --all-targets --locked -D warnings`、
  `cargo fmt --check`、`tauri-frontend` vite build；插件独立 `clean && build`（11/11）。
- 版本一致性：Tauri 版本以 `src-tauri/tauri.conf.json` 为唯一来源，运行
  `npm run verify:tauri-version` 校验 Cargo、前端 package/lockfile 与发布制品；根目录
  `package.json` 的 `1.7.3` 仅属于冻结 Electron 遗留线。
- 数据库 schema v3（rusqlite bundled WAL）；`%APPDATA%\cruciblebox` 数据路径（L3 已迁移）。
- 正式插件清单以 `scripts/plugin-catalog.json` 为准，目前包含 Document Engine（0.3.0，支持最多 2000 页 PDF、流式任务预览、单调 OCR 进度、按页/章节切分、可选择切分输出文件夹、内置 CPU OCR 默认模型并支持镜像回退）、
  Diary、Dice Roller、GIF Editor、Theme Manager、Turntable、UniEnv、JSON/文本工具箱、
  剪贴板管理器、系统信息面板和实时汇率，共 11 个；UniEnv（0.9.0）额外提供 Ruby、Zig、Deno、Bun 及现代 TypeScript/Ruby Web/Zig 原生组合包。
- 插件 backend 宿主集成：惰性 spawn / 30s 超时 / 按插件激活单飞 / 崩溃 backoff+隔离 / PermissionGuard /
  storage/log/db host 方法（e2e：真实 sidecar + gif-editor dist 全链路）。
- Electron 冻结线测试（宿主 36 文件 263 项 + 六插件 199 项 + 供应链 16 项）作为参照保留。
- 历史发布文档已归档至 `docs/history/`，记录的是当时状态。

## License

MIT

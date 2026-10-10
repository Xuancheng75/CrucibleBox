# CrucibleBox

一个基于 **Tauri 2 + Rust** 的可扩展工具箱桌面应用（Windows 10/11 x64），支持插件系统、主题系统与全局快捷键。

> **运行线**：Tauri 2.11.x（Rust core + WebView2）。Electron 43 历史运行线已冻结
> （tag `electron-1.7.3-production`，`docs/electron-legacy-registry.md` 为逐文件映射）。
> 迁移复盘：`docs/tauri-migration-review.md`。
> **当前状态**：宿主集成基线为 2.1.0-beta.4；独立 Next beta.1 契约已冻结为 Manifest/API 5、wire 3、data 1。Next 官方七插件为文档与知识库、主题管理、开发环境管理、日记与笔记、随机决策、GIF 动画编辑、压缩与解压缩。Next 旧 v2-v4 执行与 SQL RPC 已退役；原始用户数据、旧包及程序/数据库配对回滚保留。架构进展不等于 OCR 精度或 beta.3 全计划完成。

## 功能特性

- **插件系统与市场**：既可导入 `.zip`/插件目录，也可从侧边栏的双栏插件市场浏览官方插件；列表、详情与安装状态统一呈现，backend 运行于独立 **Rust sidecar**（quickjs-ng）
- **2.1 工作台**：现代化导航、可辨识的插件专属图标/分类/发布者信息、全局任务中心、清晰的运行状态反馈，以及测试版角标
- **权限模型**：插件按契约声明权限并由宿主校验；Next 插件只通过有界 capability API 访问命名空间 storage、主题、对话框、通知和任务，不提供 SQL 或任意本地路径接口。插件按可信代码管理，renderer 隔离不等于 backend 恶意代码沙箱
- **命令系统**：插件可注册全局命令，通过 `Cmd/Ctrl+Shift+P` 唤起
- **全局快捷键**：插件可注册系统级快捷键（如 `Cmd/Ctrl+I` 唤起插件导入）
- **主题系统**：内置 16 套明暗、护眼、极简、暖色与赛博风格预设，运行时切换并下发 CSS 变量，插件可实时感知主题变化
- **插件排序**：普通模式长按卡片拖动排序，批量管理模式支持多选组拖动；顺序持久化于当前数据库 schema v10，插件激活顺序跟随列表
- **批量插件管理**：主页批量管理支持批量启用、批量禁用、批量删除和多选组拖拽，操作按插件串行执行并汇总失败项
- **生命周期恢复**：导入、升级、启停、卸载和崩溃恢复按插件单飞，目录替换使用可恢复事务，避免频繁操作导致进程或会话残留
- **双更新通道**：稳定版与测试版分别读取独立签名元数据，测试版不会覆盖稳定版；请求有超时、重试和状态复位
- **插件日志**：日志入库（`plugin_logs` 表）并支持按插件、级别筛选与实时刷新
- **自定义协议**：`cruciblebox-plugin://`（Windows path 型 `http://cruciblebox-plugin.localhost/<token>/`）安全地服务插件 renderer 静态资源（内置路径穿越防护 + MIME 白名单）
- **配置中心**：`settings` 表持久化应用配置（rusqlite bundled WAL）

## 技术栈

- **Tauri 2.11.x**（Rust core：rusqlite / sidecar 进程管理 / renderer 会话）+ WebView2
- React 18 + Ant Design 5 + zustand（tauri-frontend/）
- **rusqlite（bundled SQLite 3.53.x）**——与 better-sqlite3 文件格式零迁移兼容
- 插件 backend：**quickjs-ng**（Rust sidecar，独立进程 + 帧协议）
- Vitest（宿主、插件与 Next SDK）/ cargo test + clippy + fmt（Rust/Tauri）

## 快速开始

```bash
# Tauri 线（当前本地集成基线 2.1.0-beta.4）
npm run check
cd src-tauri && cargo fmt --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked
cd tauri-frontend && npm install && npm run build
npm run build:frame              # 插件 frame runtime（out/plugin-frame/runtime.js）

# 插件（独立自包含工程，1.9.0 起）
cd plugins/<id> && npm run clean && npm run build

# Tauri Next 官方插件产物
npm run package:plugins:tauri && npm run verify:plugins:tauri
```

## 插件开发

新架构使用冻结的 Next beta.1 契约。Manifest/API 为 5、wire 为 3、data schema 为 1；新插件在各自目录 vendor 精确版本 SDK 5.0.0-beta.1 和独立构建 CLI 1.0.0-beta.1。

```json
{
  "id": "example-plugin",
  "version": "0.1.0",
  "displayName": "示例插件",
  "description": "Next beta.1 renderer-only 插件",
  "author": "CrucibleBox",
  "manifestVersion": 5,
  "sdkApiVersion": 5,
  "wireVersion": 3,
  "dataSchemaVersion": 1,
  "renderer": "dist/renderer.js",
  "permissions": ["storage:read", "storage:write"]
}
```

renderer 导出 ESM mount(context)，只使用 SDK 声明的能力；没有 Node、宿主 DOM、SQL 或任意文件路径 API。Next backend 可选，入口使用 dist/main.js 字符串并在受限 QuickJS sidecar 中运行。官方目录以 contracts/next/official-plugins.json 为准。七个官方 Next 插件可独立构建、测试和打包；其他既有插件包与用户数据保留，未要求迁移的包不会因此被删除。完整契约见 docs/plugin-sdk.md。

## 目录结构

```
src-tauri/                 # Tauri 主进程（Rust）
  src/main.rs              # 装配点（updater/协议/DB/L3 迁移/命令注册/退出清理）
  src/commands.rs          # IPC 命令组（settings/app/plugin 读写/session/日志）
  src/db.rs                # repository 宿主适配；v1-v10 迁移在独立 crate 内
  src/backend_process.rs   # 插件 backend sidecar 管理器（spawn/崩溃恢复/权限）
  src/envelope_host.rs     # host 方法分发（storage/log/db）
  src/permissions.rs       # PermissionGuard（15 权限）
  src/plugin_session.rs    # renderer session registry
  src/plugin_protocol.rs   # cruciblebox-plugin 协议 handler
  crates/                  # repository、Next contract/data、Document 与统一任务核心
  cruciblebox-plugin-host/ # Next backend sidecar crate（QuickJS，wire 3）
workers/document/          # PDFium 与 Document IR 的独立按需 worker
tauri-frontend/            # React 渲染层（App / PluginHost / themeCache）
plugins/                   # 2.1 beta 聚合插件与旧插件源码（自包含工程）
shared/                    # 跨进程共享契约（types / themes / RPC）
旧 Electron 源码不在活动工作树；历史参照见 docs/electron-legacy-registry.md。
```

## 插件 backend（Next beta.1）

Next backend 是可选能力，使用 Manifest/API 5、wire 3 和 Next sidecar。受限 CJS loader 只解析插件包内文件，没有 Node builtin、数据库连接或任意文件读写；host capability 仍由宿主逐请求授权。七个官方 Next 插件当前为 renderer-only，GIF 包含单独声明的 renderer worker。

v2-v4 旧 backend 执行入口、旧 loader 与 SQL RPC 已从生产路径退役。旧安装包、用户文件、配置和 storage 原值保留用于迁移诊断；需要回滚时必须使用 scripts/next-paired-rollback.mjs 配对恢复旧程序与旧数据库。

## 插件渲染隔离

Next 插件在 sandboxed iframe 中运行，通过专用 MessagePort 和 opaque origin 与宿主通信。wire 3 单帧上限 64 KiB、每会话最多 32 个在途请求；大结果经有界分块引用传输。宿主绑定会话身份、storage owner、任务 owner 和权限。具体约束与 fixture 见 docs/plugin-sdk.md 和 docs/security-model.md。

## 发布与诊断

- **Tauri 发布链**（`tauri-release.yml`，`tauri-v*` tag）：NSIS 安装器 + tauri-plugin-updater（minisign 强制签名 JSON）+ SBOM 与 GitHub artifact attestation。首个 Tauri 正式版为 v1.9.2；当前本地集成基线为 2.1.0-beta.4。稳定/测试通道分别使用 `tauri-stable` / `tauri-beta`。
- 插件发布会生成 ZIP、逐文件 SHA-256 清单及 SBOM。Next 官方插件另以独立 renderer-only 包和冻结 catalog 验证；文档 worker/PDFium 是独立固定摘要运行时。
- 运行时在 `%APPDATA%\cruciblebox\logs` 写诊断信息（进程内存探针已于 1.9.3 移除）。
- 完整发布环境变量和验收步骤见 `docs/release-runbook.md`。

## 架构与文档

- 当前模块、进程、数据流和信任边界：`docs/architecture.md`（Tauri 2 基线）
- 安全模型与信任边界：`docs/security-model.md`（Tauri 基线）
- Next beta.1 冻结插件契约与命名空间存储：`docs/plugin-sdk.md`
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

## 当前架构与验证基线

Tauri 宿主本地集成基线为 2.1.0-beta.4；Next 架构契约冻结为 beta.1。Next 官方范围为文档与知识库、主题管理、开发环境管理、日记与笔记、随机决策、GIF 动画编辑、压缩与解压缩。其源码迁移和独立构建不代表 beta.3 完整发布验收，也不改变 OCR 准确率门槛。

- 当前数据库 schema v10；repository crate 独占生产连接、SQL、事务迁移和 WAL checkpoint。Next 卸载/重装保留原始配置、storage、迁移标记和插件目录；旧程序回滚必须配对恢复旧数据库。
- PDFium 与 Document IR 在 document-worker 独立进程，使用按需固定摘要包；基础宿主不要求 OCR worker。OCR 模型、公式精度、长文档和安装器 UI 有各自门禁。
- Next SDK、CLI、两个示例和七个官方插件按根生成契约独立验证。旧插件不因官方目录收敛而删除。
- 基础检查：`npm run check`；Rust：`cd src-tauri && cargo fmt --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`；前端：`cd tauri-frontend && npm run build`。
- worker 验收与性能对照见 `.github/workflows/ci.yml`；发布包由 `.github/workflows/tauri-release.yml` 按 MSVC 目标同次构建 worker、pin catalog 并验收安装、PDF 解析和任务恢复。

## License

MIT

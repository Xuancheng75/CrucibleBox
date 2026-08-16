# CrucibleBox Tauri 2.0 迁移计划

> 状态：规划稿（2026-08-14），未开始实施。
> 基线：CrucibleBox 1.7.3（Electron 43.3.0 / React 19 / Ant Design 6 / better-sqlite3 / 6 插件 199 项测试）。
> 目标版本线：1.8.X（Tauri 迁移，核心目的：降低内存占用）+ 1.9.X（插件独立性 + 复杂度收敛）。
> 伴随目标：openbox → cruciblebox 全面改名；1.9.X 完成后重写 README。

## 0. 目标与硬约束

| 目的 | 对应版本线 |
|---|---|
| ① 核心：Tauri 2.0 替换 Electron 43 降低内存占用 | 1.8.X |
| ② 插件开发独立性 ↑ / 依赖 ↓ / 成本 ↓ | 1.9.X |
| ③ 复杂度 ↓ / 可维护性 ↑ | 贯穿两线 |
| ④ 品牌统一：内部标识 openbox → cruciblebox | 1.8.1–1.8.4 |

**迁移必须保留的安全不变式**（沿用 1.7.x 基线）：

- sandboxed renderer 隔离、跨源 iframe（或等价唯一 origin）、独立 backend 进程；
- 主进程权限重校验；安装 journal / 原子替换 / 崩溃恢复；
- UniEnv 固定文件集 + 摘要 + fail-closed；
- 插件 Ed25519 签名、SBOM、GitHub artifact attestation；
- 数据库 schema v3 兼容。

**必须诚实面对的两个前置风险**（lib-1 调研结论）：

1. **插件 iframe + 自定义协议在 Windows Tauri 有开放 bug**（#11505 子资源 Connection refused、#15408 跨线程崩溃、#14167 handler 列表过大）——插件隔离是迁移命门，必须 PoC 先行。
2. **内存收益无官方基准**，社区数据在 Windows 上自相矛盾——1.8.0 必须建立实测 A/B 门禁（测不过则回退决策）。

## 1. 调研结论速览（lib-1，2026-08-14）

- Tauri 2.11.5 stable；Windows 后端 = WebView2（Chromium，Win11 预装 / Win10 多数预装，需兜底）。
- 进程模型：1 个 Rust core 进程 + WebView2 进程组（同 user data folder 共享 browser 进程）。
- sidecar（tauri-plugin-shell）可替代 `utilityProcess.fork`：独立子进程、独立地址空间，但仅 stdin/stdout 管道 IPC（无原生 MessagePort），无 OS 级沙箱。
- IPC：`#[tauri::command]` + capabilities/ACL（默认全 deny）；事件 / Channel。
- 自定义协议：`register_uri_scheme_protocol` 可用；Windows origin 为 `http://<scheme>.localhost/<path>`。
- 数据库：better-sqlite3 无法驻留（Tauri 无 Node 运行时）；替代 = rusqlite（主进程同步）或 tauri-plugin-sql（sqlx，前端直查）。
- 前端：React 19 + AntD 6 + vite 可直接复用（WebView2 = Chromium，构建目标 chrome105）。
- 打包：NSIS/MSI；更新 = tauri-plugin-updater（JSON 清单 + 强制签名，与 latest.yml/blockmap 不同）；CI = tauri-action。
- SBOM：Tauri 无内置，需 cargo-cyclonedx + 现有前端 SBOM 双源；attestation 沿用 GitHub 通用能力。

## 2. 前置风险门禁（1.8.0 PoC）

| # | PoC 项 | 验收门禁 | 回退策略 |
|---|---|---|---|
| P1 | 插件 iframe + 自定义协议 + MessagePort RPC 最小场景（Windows） | iframe 隔离验证通过（自定义协议或 srcdoc opaque-origin 变通） | 若无法隔离 → 冻结迁移决策 |
| P2 | sidecar 独立进程 + stdin/stdout 帧协议 | 进程独立、RPC 往返正常 | Node sidecar 变通（重方案，~30-80MB） |
| P3 | rusqlite 打开现有 v3 openbox.db | 迁移链路等价 + 字节兼容 | 保留 better-sqlite3 侧（不可行）→ 重写 DB 层 |
| P4 | 内存 A/B 基准（Electron vs Tauri 原型，同机多轮中位） | 报告输出；不达标可中止 | 冻结迁移，保持 Electron |

> 注（1.9.5）：P4 内存基准在 1.8.0 PoC 完成后即转为"方向性结论"；随 1.9.3 移除
> `get_process_memory` 探针，不再作为后续门禁/特性维护。

## 3. 1.8.X 版本计划（Tauri 迁移 + 改名）

> 每个小版本沿用既有检查单（check/build/smoke + 相关迁移测试）；关键改动（DB/安全/发布链）走 ora 独立审查。

### 1.8.0 — 迁移 PoC + 内存基准门禁（Go/No-Go）

- 建立 `src-tauri/`（Cargo + tauri 2.11.x + vite 前端直连），保留现有 vite/React/antd/zustand 前端资产。
- 完成 §2 四项 PoC（P1 为命门）。
- 明确 L2 目标命名方案与 L3 数据路径迁移策略（见 §5）。
- 产出 Go/No-Go 报告。

### 1.8.1 — 主进程 / DB / IPC 层迁移 + L3 数据路径迁移

- Rust core：`database/` → rusqlite 重写（EngineDb 等价、MIGRATIONS v3、日志清理、`.bak-sqljs` 保留）；插件仓储；安装事务/journal 在 Rust 侧重写（保持状态机语义）。
- IPC：4 组 Electron IPC → `#[tauri::command]` 命令组（按模块拆分避免 #14167）；capabilities/ACL 默认全 deny + 白名单。
- 系统能力：dialog/fs/globalShortcut/opener → 官方插件 + 权限 scope。
- 前端桥：`window.electronAPI` → `@tauri-apps/api` invoke/event/Channel；保留 React/antd/zustand 与构建目标 chrome105。
- **L3 数据路径迁移工作包**：`%APPDATA%\openbox` → `%APPDATA%\cruciblebox` 一次性搬迁（DB 拷贝 + 插件目录迁移 + journal 兼容），沿用 smoke-release-transition 思路；升级/回滚双验证。
- 主题/更新：theme.service 等价、AppUpdateService → tauri-plugin-updater 适配（后续 1.8.4）。

### 1.8.2 — 插件 backend 迁移（utilityProcess → sidecar）+ 插件包改名

- sidecar 协议：自定帧协议替代 MessagePort RPC（信封复用 openbox-rpc 内核语义，字节格式尽量保持）。
- 生命周期：独立进程 spawn/停止/崩溃恢复/超时（无 Electron 内置策略，自建）。
- 权限：capabilities `shell:allow-spawn` + scope；主进程命令内权限重校验保留。
- 签名/安装：插件 Ed25519 验签、staging/journal/原子替换在 Rust 侧等价实现。
- **L2 改名**：插件包名 `openbox-plugin-*` → `cruciblebox-plugin-*`（含插件签名/trusted digest 重钉）。

### 1.8.3 — 插件 renderer 隔离迁移 + 协议 scheme 改名

- iframe 方案按 1.8.0 PoC 结论定案（自定义协议 or srcdoc opaque origin）；唯一 session origin + MessagePort RPC 保留。
- frame-entry / PluginFrameBridge 适配 Tauri 前端（无 Electron preload/contextBridge）。
- 主题/配置/通知链路：插件 `--ob-*` 变量、theme RPC、notify/confirm 走 Tauri 命令。
- **L2 改名**：协议 scheme `openbox-plugin` → `cruciblebox-plugin`、握手消息 `openbox-plugin-connect/port` → `cruciblebox-*`；CSP/测试同步。

### 1.8.4 — 打包 / 更新 / 发布链 + 改名收尾（已完成，发布延后）

- 打包：`tauri.conf.json`（NSIS + `webviewInstallMode` downloadBootstrapper 兜底 Win10 缺运行时）、updater 签名密钥对（minisign，公钥入库 `tauri.conf.json`，私钥在 CI secret `TAURI_SIGNING_PRIVATE_KEY`）。
- 更新：tauri-plugin-updater（JSON 清单 + 强制签名）；`@tauri-apps/plugin-updater` 前端 check() 最小接入（下载/安装 UI 随 1.9.x 前端迁移落地）。
- CI/发布：`.github/workflows/tauri-release.yml`（tauri-v* tag，与 Electron v* 链并存）；SBOM（cargo-cyclonedx，硬门禁）+ artifact attestation（SHA pin）。
- **L2/L4 改名收尾**：appId `com.openbox.app` → `com.cruciblebox.app`、SBOM 名 `openbox.cdx.json` → `cruciblebox.cdx.json`、宿主 package name/author → cruciblebox、文档产品层全量替换。红线保留：运行时代码（openbox-app scheme/metrics/owner-proof/事件名/openbox.db）、src-tauri L3 数据路径、CI 签名 secret 名、插件 author 元数据、`@openbox/ui`（已内联进 theme-manager，1.9.0）/`openbox-rpc`（1.9.x）。
- **发布决策（2026-08-14）**：1.8.4 **合入但不发布**——Tauri 壳 UI 仍为骨架（仅最小插件宿主），完整宿主前端在 1.9.x。首个 Tauri 正式版移至 **1.9.2**（前端完整后发布），届时打 `tauri-v1.9.2` tag 触发发布链。

> 发布前 checklist（H2）：首次发布前用 `tauri signer sign` 自签一个测试文件并用 conf 公钥验证，确认 CI secret 私钥与仓库公钥配对；`TAURI_SIGNING_PRIVATE_KEY` secret 未配置前发布链会在 updater 阶段失败。

## 4. 1.9.X 版本计划（插件独立性 + 复杂度收敛）

### 1.9.0 — 插件 SDK 独立化

- 插件契约精简：Manifest v2 保持，消除对宿主构建链的隐式依赖（插件 package.json 不再引用 `../../scripts/*`）→ 插件成为自包含工程（独立 build/test/发布），宿主只消费 dist。
- cruciblebox-plugin-api 唯一化：插件纯类型依赖、零宿主运行时引用（仅 invoke 面）。
- 插件后端标准化：通用 sidecar 协议模板化（新插件后端零样板）；`@openbox/ui` 已内联进 theme-manager（1.9.0）。
- 插件目录/签名工具：独立 create-plugin CLI 完善（脚手架 + 签名 + 版本管理）。

### 1.9.1 — 宿主复杂度收敛

- 清理：删除 Electron 时代 fallback/协议残留（Tauri 侧等价重写后旧 TS 层删除）；Rust 侧模块化（拆分超大 handler 表）。
- 双 RPC 收敛：明确"invoke 命令 + 插件 sidecar 帧协议"两套边界，不留第三套。
- 文档：AGENTS/architecture/security-model 更新到 Tauri 基线；历史 Electron 架构归档。
- 构建链单一：单 `tauri build` 全链路；性能预算按 Tauri 重测。

### 1.9.2 — 维护确认 + 迁移复盘 → 发布 v1.9.2

- 全量验证：check/build/smoke/installer/updater 迁移演练全绿。
- 复盘：内存/体积/启动对比报告（Tauri vs 原 Electron 版）定稿；插件开发成本前后对比。
- 冻结：明确 Tauri 为唯一运行线；Electron 分支归档。
- **重写 README**：以 Tauri 基线 + cruciblebox 命名 + 新插件开发流程为唯一事实源（见 §6）。

## 5. openbox → cruciblebox 改名（L1–L4 分层）

| 层次 | 内容 | 现状 | 时机 |
|---|---|---|---|
| L1 纯展示 | 产品名/安装器/仓库名 | 已是 CrucibleBox | 无需改 |
| L2 内部标识 | package name、appId、协议 scheme、握手消息、CSP、环境变量（OPENBOX_SMOKE_*）、指标名（openbox.startup）、包名（openbox-plugin-*/openbox-rpc；`@openbox/ui` 已内联进 theme-manager，不再是包） | 全部 openbox（215 处/129 文件） | 1.8.1–1.8.4 分批 |
| L3 用户数据路径 | `%APPDATA%\openbox`（openbox.db / plugins / logs） | openbox | **1.8.1 单独迁移工作包** |
| L4 供应链/发布 | GitHub repo、release 资产、SBOM 名、attestation subject | 已是 CrucibleBox | 无需改 |

**L3 关键决策**：采用"新路径 + 一次性搬迁"（DB 拷贝 + 插件目录迁移 + journal 兼容），不做半吊子保留旧路径；搬迁需单独小版本验证（升级/回滚 + journal），并入 1.8.1。

**改名纪律**：
- 逐文件审查而非纯 grep-replace（测试断言、CSP origin、协议常量、指标前缀各有语义）；
- 不与 rusqlite 重写逻辑混在同一 PR，按小版本隔离；
- 不影响已发布 tag（v1.5.23 ~ v1.7.3 不动）。

## 6. README 重写（1.9.2 完成时）

1.9.X 全部完成后重写 README，内容以唯一事实源为准：
- 技术栈改为 Tauri 2.11.x + Rust + React 19 + AntD 6 + rusqlite；
- 项目名/内部标识统一 cruciblebox；
- 插件开发流程改为独立工程（自包含 build/test/发布 + sidecar 协议 + 签名工具）；
- 构建/发布/更新链路改为单 `tauri build` + tauri-action + tauri-plugin-updater；
- 测试基线、目录结构、验证命令全部按 Tauri 版本更新；
- 保留：插件安全模型、可信服务（UniEnv）、供应链（SBOM/attestation）说明。

## 7. 版本节奏汇总

| 版本 | 主题 | 发布 |
|---|---|---|
| 1.8.0 | PoC + 内存基准门禁（Go/No-Go） | 内部基准 |
| 1.8.1 | 主进程/DB/IPC 迁移 + L3 数据路径迁移 | 提交 GitHub |
| 1.8.2 | 插件 backend sidecar + 插件包改名 | 提交 GitHub |
| 1.8.3 | 插件 renderer 隔离 + scheme 改名 | 提交 GitHub |
| 1.8.4 | 打包/更新/发布链 + 改名收尾 → **首个 Tauri 正式版** | ✅ Release |
| 1.9.0 | 插件 SDK 独立化 | 提交 GitHub |
| 1.9.1 | 宿主复杂度收敛 | 提交 GitHub |
| 1.9.2 | 维护确认 + 复盘 + **README 重写** → 收官 | ✅ Release |

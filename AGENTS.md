# AGENTS.md — CrucibleBox 当前状态与工作规范

> 本文件只描述**当前状态**与工作规范，不记录历史里程碑。
> 历史阶段记录已归档至 `docs/history/AGENTS-history.md`；优化计划见 `docs/maintenance-plan.md`。
> Electron 遗留层（1.9.1 起冻结）：`electron/`、`database/`、`plugin-system/` 已打 `ARCHIVED` 头标记，逐文件→Tauri 对等物/缺口见 `docs/electron-legacy-registry.md`，冻结快照 tag `electron-1.7.3-production`。

## 1. 项目简介

- **CrucibleBox**（npm 包名 `cruciblebox`，Rust crate `cruciblebox`）：**Tauri 2.11.x + Rust + React 18 + Ant Design 5 + zustand + rusqlite** 构建的 Windows 10/11 x64 可扩展工具箱（当前发布基线 **1.9.23**；Electron 43/React 19/Ant Design 6 为已冻结的历史运行线）。
- 唯一可编辑/可构建源码：`E:\CrucibleBox_Sourses`（git 仓库，workspace 含 `plugins/*`、`packages/{cruciblebox-plugin-api,openbox-rpc}`）。
- 插件模型：Manifest v2 + 自包含 browser renderer（跨源 sandboxed iframe + MessagePort RPC）+ 可选 backend（Rust sidecar 内嵌 quickjs-ng，帧协议 RPC v2）。UniEnv 为宿主固定摘要可信服务。
- 当前基线：Rust workspace `src-tauri`（主 app + `cruciblebox-plugin-host` sidecar）测试全绿；插件独立构建（自包含 `scripts/` 构建器）；数据库 schema v3（rusqlite bundled）；插件 SDK v2 已冻结；自动更新走 tauri-plugin-updater（minisign 强制签名）。
- 插件 SDK 独立化（1.9.0）：插件自包含工程（独立 build/clean/typecheck/test，无 `../../scripts` 隐式依赖），宿主只消费 `plugin.json + dist/main.js + dist/renderer.js`；`@openbox/ui` 已内联进 theme-manager（`plugins/theme-manager/src/theme-vars.ts`）。

## 2. 关键文件地图

Tauri 主进程（Rust，`src-tauri/src/`）：

- `main.rs`：装配点（tauri-plugin-updater 注册、renderer 自定义协议、DB 初始化、L3 数据路径迁移、命令注册）。
- `commands.rs`：核心 IPC 命令组（settings/app/plugin 读路径/session/db_status；`is_main_window` 校验 + settings key 白名单）。
- `unienv_catalog.rs` / `unienv_versions.rs` / `unienv_install.rs` / `unienv_task.rs`：UniEnv 可信服务四件套——制品完整性目录（静态 SHA-256）、在线版本源（node/go/java 官方端点发现，8s 硬超时，ADR-0021）、安装原语（下载/解压/junction/进程）、任务管理（单飞/进度/取消）。
- `db.rs`：rusqlite bundled 引擎（WAL + v1-v4 迁移 + legacy storage 迁移 + 日志清理；v4 修复 L3 迁移后的 installed_path 残留）。
- `data_dir.rs`：L3 数据路径迁移（`%APPDATA%\openbox` → `%APPDATA%\cruciblebox`，checkpoint + 原子 rename）。
- `plugin_session.rs` / `plugin_protocol.rs`：renderer session registry + `cruciblebox-plugin` 协议 handler（path 型 `http://cruciblebox-plugin.localhost/<token>/index.html`）。
- `capabilities/default.json`：ACL（core:default + updater:default）。
- `cruciblebox-plugin-host/`（独立 crate）：插件 backend sidecar（quickjs-ng），帧协议 + 信封 v2 + CJS loader。

Electron 遗留层（**已冻结**，勿改功能）：

- `electron/`、`database/`、`plugin-system/`：1.7.3 生产基线的 TS 实现，1.9.2 物理删除/迁移前仅作参照。详见 `docs/electron-legacy-registry.md`。

Tauri 前端（`tauri-frontend/`）：

- `src/App.tsx`：骨架 UI（内存探针 + 更新检查 + 插件宿主）。
- `src/PluginHost.tsx`：插件 iframe 宿主（invoke 创建 session + 复用 `src/plugin-runtime/PluginFrameBridge.ts` 握手）。
- 复用仓库根 `src/plugin-runtime/`（frame-entry + PluginFrameBridge，纯浏览器逻辑）与 `shared/`。

共享契约：

- `shared/plugin-backend-rpc.ts`（校验语义已移植 sidecar envelope.rs）、`shared/plugin-renderer-rpc.ts`（插件 renderer 契约，两线复用）、`shared/types/`、`shared/themes/presets.ts`、`shared/trusted-service-policies.json`。

构建与发布：

- `scripts/`：插件打包/签名/校验/SBOM 脚本（与插件自包含构建器解耦）。
- `.github/workflows/`：`ci.yml`（Electron verify + **Tauri verify 并行**）、`release.yml`（Electron v* 发布，冻结中）、`tauri-release.yml`（_*Tauri tauri-v* 发布链_*，首个 Tauri 正式版 v1.9.2）。

## 3. 安全模型（详见 docs/security-model.md）

- 插件为**可信代码**（信任模型 A）：权限声明是 SDK 能力门控，非安全边界；控制点为 安装确认 + renderer 隔离 + 帧协议校验 + 供应链可追溯。
- 渲染：跨源 sandboxed iframe + `cruciblebox-plugin://<token>.session` 唯一 origin（Tauri Windows 为 path 型 `http://cruciblebox-plugin.localhost/<token>/`）；MessagePort 版本化 RPC + JSON 预算 + 64 并发上限。
- backend：Rust sidecar（quickjs-ng 内嵌，故障隔离非安全沙箱）；帧协议 v2（token/requestId/预算）；**宿主侧 PermissionGuard 是权限边界（1.9.2 落地，不可省略）**。
- 安装：staging、journal、原子替换、崩溃恢复；manifest/ZIP 策略校验（Electron 层冻结中，sidecar 宿主侧等价待 1.9.2）。
- UniEnv：固定文件集 + 版本 + SHA-256 digest + fail-closed；构造失败即拒绝激活。
- 发布：Ed25519 插件签名、CycloneDX SBOM（cargo-cyclonedx + 前端 SBOM 双源）、GitHub artifact attestation、tauri-plugin-updater minisign 强制签名。
- 明确不承诺：Windows Authenticode（无证书）、OS 级强制沙箱（backend 必须可信）。

## 4. 验证命令（提交前必须全绿）

```bash
# Electron 遗留线（冻结中，改动 ARCHIVED 标记外需谨慎）：
npm run check          # format:check + lint + typecheck(5 层) + test（宿主 + 插件 + 供应链）

# Tauri/Rust 线（1.9.1 起新增门禁）：
cd src-tauri && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd tauri-frontend && npm run build
```

插件改动：到 `plugins/<id>/` 跑 `npm run clean && npm run build`（自包含构建器）；涉及 unienv 时需同步重钉 trusted digest（`npm run update:trusted-policy`）。

## 5. 分支与提交规范（详见 docs/maintenance-plan.md §7）

- Trunk-based：`main` 唯一长期分支，始终可发布。
- 一个工作包 = 一个短分支（`feat/<版本>-<描述>` / `refactor/<版本>-<描述>` / `fix/...` / `chore/...` / `hotfix/...`）+ 一个 PR + merge。
- Conventional Commits；全量门禁通过后才提交；不允许本地攒多个未推工作包。
- 发布：Tauri 链 `git tag -a tauri-vX.Y.Z` → push tag → `tauri-release.yml` 自动发布（Electron 链 `v*` 已冻结）。
- main 已开启分支保护（要求 PR + `Verify Windows x64` / `Verify Tauri (Rust) x64` status check + 禁 force push）。

## 6. 版本路线

1.8.X Tauri 迁移（骨架/DB/sidecar/renderer/发布链）→ 1.9.X 插件独立化 + 宿主收敛 → **1.9.2 冻结 Tauri 为唯一运行线并发布首个 Tauri 正式版**（Electron 分支归档）。当前发布基线：**1.9.23**。逐版本变更见 `docs/changelog.md`；UniEnv 在线版本源决策见 `docs/adr-0021-unienv-online-version-feeds.md`。

## 7. 文档约定

- 活文档：`docs/architecture.md`、`docs/security-model.md`、`docs/plugin-sdk.md`、`docs/install-recovery.md`、`docs/release-runbook.md`、`docs/development.md`、`docs/electron-legacy-registry.md`、`docs/tauri-migration-plan.md`。
- 历史与里程碑记录一律归档到 `docs/history/`，不写回活文档；不再新增阶段性 milestone 文档。
- **Electron 遗留层冻结纪律**：`electron/`/`database/`/`plugin-system/` 内禁止功能性改动；如确需参考或迁移逻辑，只读，改动一律先查 `docs/electron-legacy-registry.md` 的等价物是否存在。

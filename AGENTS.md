# AGENTS.md — CrucibleBox 当前状态与工作规范

> 本文件只描述**当前状态**与工作规范，不记录历史里程碑。
> 历史阶段记录已归档至 `docs/history/AGENTS-history.md`；优化计划见 `docs/maintenance-plan.md`。

## 1. 项目简介

- **CrucibleBox**（npm 包名 `openbox`）：Electron 43 + React 19 + Ant Design 6 + zustand + better-sqlite3 构建的 Windows 10/11 x64 可扩展工具箱。
- 唯一可编辑/可构建源码：`E:\CrucibleBox_Sourses`（git 仓库，workspace 含 `plugins/*`、`packages/*`）。
- `E:\CrucibleBox_Plugins` 为**只读镜像 / 发布备份**，不参与构建，不得作为编辑源。
- 插件模型：Manifest v2 + 自包含 browser renderer（跨源 sandboxed iframe + MessagePort RPC）+ 可选 backend（utility process，backend RPC v2）。UniEnv 为宿主固定摘要可信服务。
- 当前基线（1.5.26）：宿主 36 文件/261 项、六插件 190 项、供应链 16 项测试全绿；数据库 schema v3（`plugins.sort_order`）；自动更新默认关闭，配置 GitHub Release 后启用。

## 2. 关键文件地图

主进程：

- `electron/main.ts`：装配点（DB 初始化、PluginManager、IPC、窗口、关闭流程）。
- `electron/preload.ts`：contextBridge 暴露 `window.electronAPI`。
- `electron/ipc/*.ts`：plugin/settings/app/update IPC；`ipcGuard.ts` 校验可信 sender。
- `electron/theme.service.ts`：主题 get/set/list + 广播；`electron/pluginEvents.ts`：事件桥接。
- `plugin-system/PluginManager.ts`：运行时编排（生命周期/权限/日志/崩溃恢复/事件）。
- `plugin-system/PluginSandbox.ts` / `PluginProcessEntry.ts`：utility process 沙箱与 RPC 客户端。
- `plugin-system/PluginInstallationService.ts` 及事务族（DirectoryTransaction/Journal/Recovery/Preparation/ArchivePolicy）：安装/升级/卸载/崩溃恢复。
- `plugin-system/TrustedServiceRuntime.ts`：UniEnv 固定摘要可信服务（fail-closed）。
- `database/index.ts`：EngineDb 双引擎（默认 better-sqlite3 WAL，`OPENBOX_DB_ENGINE=sqljs` 显式回退）；`repositories/`、`pluginStorage.ts`。

渲染端：

- `src/plugin-runtime/`：`frame-entry.ts` + `PluginFrameBridge.ts`（iframe MessagePort 桥）。
- `src/store/`、`src/components/`、`src/pages/`、`src/api/`、`src/hooks/`、`src/theme/`。

共享：

- `shared/plugin-backend-rpc.ts`（backend RPC v2）、`shared/plugin-renderer-rpc.ts`（renderer RPC v1）、`shared/types/`（ipc/permissions/plugin/theme/rpc types）、`shared/themes/presets.ts`、`shared/trusted-service-policies.json`。
- `packages/openbox-ui`：插件共享语义 CSS 变量入口（`@openbox/ui`）。

构建与发布：

- `scripts/`：构建/校验/发布/smoke 脚本（见 package.json scripts 与 `docs/release-runbook.md`）。
- `.github/workflows/`：`ci.yml`（main push/PR 全门禁）、`release.yml`（tag v* 发布）、`release-compatibility.yml`（跨版本回滚验证）。

## 3. 安全模型（详见 docs/security-model.md）

- 插件为**可信代码**（信任模型 A）：权限声明是 SDK 能力门控，非安全边界；控制点为 安装确认 + renderer 隔离 + IPC 校验 + 供应链可追溯。
- 渲染：跨源 sandboxed iframe + `openbox-plugin://<token>.session` 唯一 origin；MessagePort 版本化 RPC + JSON 预算 + 64 并发上限。
- backend：utility process（故障隔离非安全沙箱）；backend RPC v2（token/requestId/预算）；主进程统一权限断言。
- 安装：staging、journal、原子替换、崩溃恢复；manifest/ZIP 策略校验。
- UniEnv：固定文件集 + 版本 + SHA-256 digest + fail-closed；构造失败即拒绝激活。
- 发布：Ed25519 插件签名、CycloneDX SBOM、GitHub artifact attestation、`SHA256SUMS`、Electron fuses。
- 明确不承诺：Windows Authenticode（无证书）、OS 级强制沙箱（backend 必须可信）。

## 4. 验证命令（提交前必须全绿）

```bash
npm run check          # format:check + lint + typecheck(5 层) + test(宿主 254 / 插件 190 / 供应链 16)
npm run build          # build:plugins → build:app → verify:performance
npm run release:validate   # 涉及发布链改动时：全部 smoke + manifest + checksum
```

插件改动：到 `plugins/<id>/` 跑 `npm run build` 重建 dist；涉及 unienv 时需同步重钉 trusted digest（`scripts/update-trusted-policy.mjs`，待 1.5.25 落地）。

## 5. 分支与提交规范（详见 docs/maintenance-plan.md §7）

- Trunk-based：`main` 唯一长期分支，始终可发布。
- 一个工作包 = 一个短分支（`feat/<版本>-<描述>` / `refactor/<版本>-<描述>` / `fix/...` / `chore/...` / `hotfix/...`）+ 一个 PR + squash merge。
- Conventional Commits；全量门禁通过后才提交；不允许本地攒多个未推工作包。
- 发布：`git tag -a vX.Y.Z` → push tag → release.yml 自动发布。
- main 已开启分支保护（要求 PR + `Verify Windows x64` status check + 禁 force push）。

## 6. 版本路线

1.5.X 治理与收敛 → 1.6.X 运行时简化 → 1.7.X 兼容性清理（SDK v2 冻结）。当前开发线：**1.5.24**。详见 `docs/maintenance-plan.md`。

## 7. 文档约定

- 活文档：`docs/architecture.md`、`docs/security-model.md`、`docs/plugin-sdk.md`、`docs/install-recovery.md`、`docs/release-runbook.md`、`docs/development.md`。
- 历史与里程碑记录一律归档到 `docs/history/`，不写回活文档；不再新增阶段性 milestone 文档。

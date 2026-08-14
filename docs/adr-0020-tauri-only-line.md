# ADR-0020：1.9.2 冻结 Tauri 为唯一运行线（Electron 归档）

- 状态：已接受（2026-08-14，1.9.1 收敛时固化）
- 决策背景：`docs/tauri-migration-plan.md` §4.1.9.2

## 决策

1.9.2 完成后，**Tauri 2 + Rust core + WebView2 + 插件 Rust sidecar 为唯一运行线**：

- Electron 43 宿主线（`electron/`、`database/`、`plugin-system/`）物理删除或归档到独立历史分支；
- `tsconfig` / `electron.vite.config.ts` / `vitest` / `package.json` 构建链按 Tauri 重写；
- CI 删除 Electron `verify` 任务，`tauri-verify`（cargo fmt/clippy/test + tauri-frontend build）提升为唯一门禁；
- `release.yml`（`v*` tag）退役，`tauri-release.yml`（`tauri-v*` tag）为唯一发布链；
- 插件构建/打包/签名链保留（两线共享契约），宿主只消费 dist。

## 前置条件（1.9.2 冻结前必须完成，逐项可验证）

| # | 前置 | 验证 |
|---|---|---|
| 1 | 插件写路径 Rust 命令（install/uninstall/enable/disable/reorder/updateConfig/getLogs + 对话框） | 对应 Electron handler 等价 + 测试 |
| 2 | PluginManager 运行时 + PermissionGuard 宿主守卫（sidecar HOST_METHODS 逐调用校验） | sidecar e2e + 权限拒绝测试 |
| 3 | sidecar 宿主侧 spawn/崩溃恢复/超时 + `plugin_send_message` 真路由 | 生命周期策略测试 |
| 4 | theme get/set/list + 广播；前端 store 接线（ThemeProvider → 插件 frame） | 插件 theme RPC e2e |
| 5 | 宿主测试迁移/裁剪（36 个测试文件，多数测无 Rust 等价物的 TS 逻辑） | `npm run check` 等价于 cargo + 前端门禁 |
| 6 | 文档/CI 全量切换（AGENTS/architecture/security-model 已 1.9.1 更新；构建链/CI 重写） | 两条 CI 并行绿 → 删 Electron 后仍绿 |

## 理由

1. **资源聚焦**：Electron 与 Tauri 双线维护是 1.8.x-1.9.x 的必要过渡态；冻结后消除
   双测试面、双发布链、双安全模型的心智与 CI 成本。
2. **迁移链已验证**：DB（rusqlite 零迁移）、L3 数据路径（真实数据 MATCH）、插件
   backend（sidecar 3 插件 e2e）、renderer 隔离（协议 path 型）、发布链（tauri-release
   oracle 审查通过）均已达成，冻结的技术风险已释放。
3. **插件契约不中断**：Manifest v2 / renderer RPC / backend RPC v2 / 主题 / UniEnv 均为
   两线共享契约，冻结不改变插件生态面（1.9.0 独立化已使插件自包含）。

## 不承诺 / 遗留

- 冻结不等于删除所有 TS：契约层 `shared/`、插件构建器 `scripts/` 保留；
  Electron 冻结层作为 1.9.2 迁移的"语义参考源"归档（`docs/electron-legacy-registry.md`）。
- UniEnv trusted-service 源码迁入宿主目录（`plugin-system/trusted-services/unienv/`）消除
  反向依赖，属 1.9.1 之后的优化项，不阻塞冻结（digest 钉死机制两线一致）。

## 落地纪律

- 冻结标记：1.9.1 已对 `electron/`、`database/`、`plugin-system/` 打 `ARCHIVED` 头 + 冻结
  tag `electron-1.7.3-production`；禁止功能性改动（只读参照）。
- 1.9.2 冻结动作一次性完成（物理删除 + 构建链重写 + CI 切换），不与日常开发混批。

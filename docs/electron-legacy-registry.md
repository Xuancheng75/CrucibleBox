# Electron 遗留层登记（1.9.1 冻结）

> 状态：2026-08-14。Electron 宿主线（electron/、database/、plugin-system/）已打 `ARCHIVED` 头标记并**冻结至 1.9.2**。
> 决策（ora-9）：1.9.1 **不做物理删除**（main.ts 集线引用面全、插件写路径/PluginManager/权限守卫无 Rust 等价物、tauri-release 仍被 npm run check 门禁）；物理删除 + 构建链重写是 1.9.2 冻结时的一次性工程。
> 冻结快照：tag `electron-1.7.3-production`（本仓库历史即可回滚，git 历史含全部版本）。

## 一、已完全等价（Tauri 侧有对等实现，1.9.2 可删）

| Electron 文件 | Tauri 等价 | 覆盖度 |
|---|---|---|
| database/index.ts | `src-tauri/src/db.rs`（引擎 + v1-v3 迁移 + 日志清理） | 完全 |
| database/repositories/settings.repository.ts | `commands.rs` settings_get/set/get_all | 完全 |
| database/repositories/plugin.repository.ts（读路径） | `commands.rs` plugin_list/plugin_get | 读路径完全；写路径无等价 |
| electron/ipc/settings.ipc.ts | settings 命令组 | 完全 |
| electron/ipc/app.ipc.ts | app_get_version/app_get_platform | 完全 |
| electron/ipc/ipcGuard.ts | commands.rs is_main_window + capabilities ACL | 机制不同但等效 |
| electron/ipc/ipcGuardPolicy.ts | capabilities/default.json | 概念等价 |
| electron/ipc/index.ts | main.rs invoke_handler | 装配点等价 |
| electron/preload.ts | tauri-frontend invoke 面 | 概念完全 |
| electron/HostRendererProtocol.ts | tauri.conf.json assetProtocol | 部分 |
| electron/windowSecurityPolicy.ts | capabilities + CSP | 部分 |
| plugin-system/PluginRendererSessionRegistry.ts | `plugin_session.rs` | 完全 |
| plugin-system/PluginRendererProtocol.ts | `plugin_protocol.rs` | 完全 |
| plugin-system/PluginRendererRequestOwnerProof.ts | window.label owner 绑定（方案已变） | 概念废弃 |

## 二、部分等价 / 明确缺口（1.9.2 前端迁移或宿主侧落地）

| Electron 文件 | Tauri 现状 | 缺口 |
|---|---|---|
| electron/theme.service.ts | 仅 settings 'theme' key 白名单 | ThemeGet/Set/List + PRESET_THEMES + 广播（依赖链断点在前端 store，1.9.2 接线） |
| electron/AppUpdateService.ts | tauri-plugin-updater 最小接入（check()） | 状态机/broadcast/channel 持久化未迁 |
| electron/ipc/plugin.ipc.ts | 读路径 + session 命令 | install/uninstall/enable/disable/reorder/updateConfig/getLogs + 对话框 |
| electron/ipc/update.ipc.ts | tauri-plugin-updater | 状态机 UI |
| electron/DiagnosticLog.ts / StartupMetrics.ts | eprintln + get_process_memory | 可观测性未迁 |
| electron/menu.ts / pluginEvents.ts / pluginRendererSmoke.ts | 无 | 未迁 |
| plugin-system/PluginProcessEntry.ts | cruciblebox-plugin-host（协议/加载/CJS 等价） | 宿主侧 spawn/崩溃恢复/超时未落地 |
| plugin-system/PluginSandbox.ts | sidecar（quickjs 隔离） | 进程管理不等价 |
| plugin-system/PluginManager.ts | 仅读路径 + session 创建校验 | **运行时编排（最大缺口）** |
| plugin-system/PermissionGuard.ts | 无 | 宿主权限守卫（sidecar README 明示不可省略） |
| plugin-system/PluginInstallationService / Preparation / DirectoryTransaction / Journal / Recovery / Fs / ArchivePolicy / ManifestPolicy / CrashPolicy | 无 | 安装信任决策 = 插件生态核心 |
| plugin-system/TrustedServiceRuntime + trusted-services/unienv/ | 无 | first-party 可信服务（digest 钉死） |
| plugin-system/EventBus.ts / semver.ts / pluginPaths.ts | 无 | 未迁 |

## 三、契约层（非宿主实现，保留）

- `shared/plugin-backend-rpc.ts`：校验语义已移植 `cruciblebox-plugin-host/src/envelope.rs`（sidecar crate 为准）
- `shared/plugin-renderer-rpc.ts` + `src/plugin-runtime/PluginFrameBridge.ts`：插件 renderer 契约，tauri-frontend/PluginHost.tsx 复用
- `shared/themes/*`：theme 单源（PRESET_THEMES 静态数据，前端直读）

## 四、1.9.2 冻结清单（物理删除前置）

1. 插件写路径（install/uninstall/enable/disable/reorder/updateConfig）Rust 命令落地
2. PluginManager 运行时 + PermissionGuard + 安装事务族 + TrustedServiceRuntime 等价
3. sidecar 宿主侧 spawn/崩溃恢复/超时 + HOST_METHODS 路由
4. theme get/set/list + 广播；plugin_send_message 真路由
5. tsconfig/electron-vite/vitest/package.json 重写 + 宿主测试迁移
6. CI：删除 Electron verify 任务，Tauri verify 提升为唯一门禁
7. 冻结后：Electron 分支归档，Tauri 唯一运行线

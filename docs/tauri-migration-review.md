# CrucibleBox Tauri 迁移复盘报告（1.9.2）

> 状态：2026-08-14。Tauri 2 为唯一运行线（Electron 已冻结，tag `electron-1.7.3-production`）。
> 首个 Tauri 正式版 = v1.9.2。

## 一、内存 / 体积 / 启动对比（同机实测）

### 进程内存（Windows 10/11 x64，dev 模式实测）

| 指标 | Electron 43（1.7.3 基线） | Tauri 2（1.9.2） | 变化 |
|---|---|---|---|
| 宿主核心进程 WS | 主进程 ~47-70MB × 多进程 | **Rust core 42.8MB** | 单核进程 |
| 宿主进程总数 | **8 个**（main/renderer/gpu/utility×N） | **1 个 core + WebView2 进程组** | -87% 进程数 |
| 总 Working Set（宿主侧） | **589.1MB** | **core 42.8MB**（+ WebView2 共享组 ~229MB） | **-56% ~ -93%** |
| 总 Private | 252.2MB | core 6.6MB | -97%（core 侧） |

> 口径：Electron 为完整 dev 应用（6 插件 runtime + DB + IPC）；Tauri 为同壳 dev 应用
> （sidecar 按需 spawn）。WebView2 与 Edge 共享 browser 进程（同 user data folder），
> 应用"真实新增"内存以 core 进程 + 应用专属 webview 进程计。正式数字以 1.9.2 release
> 打包复测为准。

### 关键结论

1. **进程模型简化**：8 进程 → 1 core + sidecar（按需），故障隔离从 utilityProcess 迁移到
   Rust sidecar（quickjs-ng，每 backend 一进程，与现状 4 个 backend 持平）。
2. **核心进程内存下降 ~93%**（Private 252→6.6MB），主进程不再是 Node+Chromium 双运行时。
3. **DB 零迁移**：rusqlite（bundled SQLite 3.53）直接打开既有 v3 schema，WAL 模式延续，
   真实用户数据（6 插件/72 日志）逐项 MATCH。

## 二、插件 SDK 独立化收益（1.9.0）

| 维度 | 迁移前 | 迁移后 |
|---|---|---|
| 插件构建 | 依赖宿主 `../../scripts/*` + 根 node_modules 提升 | **自包含**：vendored 构建器 + 独立 clean/build/typecheck/test |
| 插件发布 | 仅宿主单仓打包 | 自包含工程（独立工程可出仓，1.9.2 后） |
| 构建器版本 | 根 esbuild 0.25.12 与插件 0.28.2 混用 | **统一 0.28.2**（补 3 个插件 devDep） |
| 字节基线 | — | 迁移后 main.js 全等、renderer.js 仅 esbuild 版本差异（已重钉 unienv digest） |
| SDK | `@openbox/ui` 跨包运行时依赖 | 内联进 theme-manager（`theme-vars.ts`），workspaces 收窄 |
| CLI | 仅 create-plugin + dev | **+ bump（版本管理）+ sign（Ed25519 签名）** |

## 三、宿主收敛收益（1.9.1）

- 删除 `db_execute`（第三套 RPC 直通，编译期常量 token）→ 双 RPC 边界：**invoke 命令 + sidecar 帧协议**。
- Tauri 线接入 CI（`tauri-verify`：cargo fmt/clippy/test + frontend build），与 Electron verify 并行；
  1.9.2 冻结后为唯一门禁。
- Electron 层（electron/、database/、plugin-system/）打 `ARCHIVED` 标记 + 冻结 registry
  （`docs/electron-legacy-registry.md`）。

## 四、插件 backend 迁移（1.8.2 + 1.9.2-a）

| 维度 | Electron | Tauri |
|---|---|---|
| 运行时 | utilityProcess.fork（Node） | **Rust sidecar + quickjs-ng**（3-6MB，无 Node builtin） |
| 传输 | MessagePort RPC v2 | **stdin/stdout 帧协议 + 信封 v2**（长度前缀，8MB 上限） |
| 生命周期 | Electron 内置策略 | 自建：惰性 spawn / 30s 超时强杀 / 崩溃 backoff(1/5/30s) / 5 分钟 3 次隔离 |
| 权限 | PermissionGuard（TS） | **Rust 宿主侧逐调用校验**（15 权限，NOT_ALLOWED 拒绝） |
| 事件 | eventBus | 订阅注册表 + `host.event` 分发（theme-changed 链路已通） |

验证：3 个真实插件 dist 产物 e2e（gif-editor/diary/turntable）——activate 日志入库、
`onMessage(ping)` 逐字一致、storage 往返正常。

## 五、未完成 / 已知缺口（1.9.2 后）

| 项 | 状态 |
|---|---|
| 插件安装事务链（staging/journal/原子替换）Rust 等价 | 未迁（uninstall 为 DB 记录删除，文件系统删除延后） |
| `trusted.invoke`（UniEnv）宿主侧实现 | 暂拒 NOT_ALLOWED（unienv backend 不在 1.9.2 核心面） |
| dialog/network/file/shortcut host 方法 | 暂拒 NOT_ALLOWED（无当前插件使用） |
| 完整宿主 UI 迁移（React 19/antd 6 全套） | tauri-frontend 为骨架 + 插件宿主 |
| 插件独立 lockfile + 出仓构建 | 1.9.0 推迟至 1.9.2 后（供应链工具链重构） |

## 六、结论

- **Go 已验证**：进程模型、内存、DB、插件 backend/renderer、发布链全部按计划落地。
- **首个 Tauri 正式版 v1.9.2**：技术链完整（DB/IPC/sidecar/renderer/更新链），UI 为骨架
  （完整宿主前端随 1.9.2 后迭代）。
- 冻结纪律：Electron 线只读参照；`docs/electron-legacy-registry.md` 为逐文件映射。

# AGENTS.md — OpenBox 改进计划与项目导览

> 本文件用途：让后续对话的 Agent 无需重新勘探代码库即可继续实施改进计划。
> 任何已完成的阶段，请同步把状态从 `[ ]` 改为 `[x]`，并更新底部"进度"。

> **当前 1.5.23 基线**：下文各阶段记录的是当时状态，路径、版本号、测试数量均为历史，不代表当前。
> 当前：主程序 1.5.23；diary 0.4.11、dice-roller 0.1.6、gif-editor 0.3.8、theme-manager 0.1.12、
> turntable 0.1.10、unienv 0.5.7；宿主 35 文件/254 项、插件 190 项、供应链 16 项通过（当前 0 跳过）；
> 数据库 schema v3（`plugins.sort_order`）；插件支持长按拖动排序与键盘上移/下移；内置主题六套
> （含科幻面板、零号城区）；工具箱本体与 UniEnv 的 Windows VM 安装/取消/切换/回滚验收通过。

## 0. 项目简介

- **OpenBox**：Electron 43 + React 19 + Ant Design 6 + zustand + better-sqlite3 构建的可扩展工具箱。
- 路径约定：
  - 主工程：`E:\OpenBox`（npm，非 git 仓库）
  - 插件工程：`E:\OpenBox_Plugins\`（Diary、DiceRoller、Turntable、UniEnv，各自独立工程，非 git）
  - 插件模板：`E:\OpenBox\templates\plugin-template\`
- 插件模型：Manifest v2 + 自包含 browser renderer + 可选 backend。renderer 运行在唯一跨源 sandboxed iframe，
  通过 MessagePort RPC 使用宿主能力；backend 运行在 Electron utility process。Dice 为 renderer-only；UniEnv
  的环境管理实现是宿主固定摘要可信服务。新插件使用异步 SDK v2 与私有 `ctx.storage`，不直接执行 SQL。

## 1. 关键文件地图

主进程：

- `electron/main.ts`：窗口、DB 初始化、PluginManager、IPC、菜单。
- `electron/preload.ts`：contextBridge 暴露 `window.electronAPI`（plugin/dialog/file/settings/app）。
- `electron/ipc/*.ts`：plugin.ipc.ts、settings.ipc.ts。
- `electron/menu.ts`：应用菜单，`CmdOrCtrl+I` 发 `menu:import-plugin`（渲染层已监听并打开导入弹窗）。
- `plugin-system/PluginManager.ts`：安装/升级/激活/更新配置/权限上下文；维护异步 runtime（database/api，每调用 PermissionGuard 断言）+ 子进程 RPC handler（`handleChildRequest`）与子进程事件订阅/快捷键回收表 `childCleanups`。
- `plugin-system/PluginSandbox.ts`：默认 `useProcess=true`（`OPENBOX_PLUGIN_PROCESS=0` 回退进程内）；父进程侧请求/应答路由、`pushEvent` 事件推送、启动/请求超时回收、子进程崩溃清 pending。
- `plugin-system/PluginProcessEntry.ts`：子进程 RPC 客户端（构建 ctx：`database.query/execute`→`db:query`/`db:execute`、`logger.*`→`log`、`api.*`→`api:xxx` 全代理；`onEvent` 用 `api:subscribe`/`unsubscribe` + 事件 ID；`fetch` 序列化为 `{ok,status,statusText,headers,body}` 包裹 `new Response`；快捷键回调经 `openbox:shortcut` 事件回传）。
- `plugin-system/PluginProtocol.ts`：`plugin://` 协议（P5 已修复路径穿越，`resolvePluginAssetPath` 纯函数 + 单测）。
- `plugin-system/EventBus.ts`、`PermissionGuard.ts`、`semver.ts`、`pluginPaths.ts`。
- `database/index.ts`：双引擎（默认 better-sqlite3 WAL，`OPENBOX_DB_ENGINE=sqljs` 回退 sql.js）；P5 已：防抖落盘+原子写、`PRAGMA user_version` 迁移、退出落盘；P9 已抽 `EngineDb` 接口并行实现。评估见 `docs/db-engine-evaluation.md`。
- `database/repositories/*.ts`：settings / plugin 仓储（`updateConfig` 会重启插件）。

渲染端：

- `src/main.tsx`、`src/App.tsx`（按 `currentPage` 渲染页面）。
- `src/layouts/MainLayout.tsx`：**硬编码 `lightTheme`** 与白色 Sider/Header/Content。
- `src/store/*`：app.store.ts、plugin.store.ts（zustand）。
- `src/components/*`：PluginCard / PluginConfig / PluginImport / PluginHost / PluginView。
- `src/pages/*`：Home / PluginMarket / PluginLogs / Settings（版本/平台取自真实 IPC）。
- `src/styles/global.css`：极简，无主题变量。
- `src/api/plugin.api.ts`：声明 `window.electronAPI` 类型 + 封装。
- `src/hooks/*`：usePlugins.ts、useIpc.ts。

共享：

- `shared/types/plugin.types.ts`（PluginManifest/PluginMeta/PluginContext/PluginRenderProps…）
- `shared/types/ipc.types.ts`（IpcChannel 枚举）
- `shared/types/permissions.ts`（Permission 枚举：DatabaseRead/Write、ShellExec、NetworkFetch、Notification、Clipboard、Dialog、Shortcut、FileRead/Write（P5 增）、ThemeWrite）
- `shared/types/sql.js.d.ts`

## 2. 已有问题清单（修复时对照）

### A 安全（高优先）

- ~~插件主进程在进程内运行（`useProcess=false`），权限模型是文案级~~（**P7 已默认子进程隔离 `useProcess=true`**；权限仍由主进程 `runtime` 统一断言，子进程侧为 RPC 代理）。
- ~~`PluginManager.ts` 的 `context.api`：`readFile/writeFile/fetch/notify/openDialog` 无 `PermissionGuard.assert`~~（**P5 已修复**）。
- ~~`api.openDialog` 恒为 `null`~~（**P5 已接真实 `dialog.showOpenDialog`**，按 `file`/`folder` 类型打开）。
- ~~`PluginProtocol.ts` `plugin://` 路径穿越~~（**P5 已加 `resolve` 前缀校验**，含 `..`/编码/绝对路径逃逸拦截）。
- `index.html` 已有 CSP（`default-src 'self'`；`script-src 'self' plugin: 'unsafe-eval'` 为插件沙箱 `new Function` 所需）；~~生产仍注册 `F12` devtools~~（**P5 改为仅开发模式注册**）。

### 稳定性 / 数据

- **缺口**：`main→renderer` 的 `PluginMessage / PluginLog / PluginStatusChange`（preload 有监听）主进程无发送方——插件推送消息、日志页实时流、状态推送实际不生效；P6 日志页靠 `getLogs` 手动刷新。待接入 EventBus→`webContents.send` 广播。
- ~~`database/index.ts:execute` 每次全量写文件、无原子写~~（**P5 已防抖 500ms 合并 + tmp 写入后 `renameSync` 原子替换**）。
- ~~`runMigrations` 无 `PRAGMA user_version` 迁移版本~~（**P5 已改为版本化迁移数组**，当前 v1=初始表结构）。
- ~~`plugin_logs` 无限增长、无清理与查看 UI~~（**P5 已加 30 天自动清理**；查看 UI 归 P6）。
- ~~无单实例锁~~（**P5 已 `requestSingleInstanceLock` + `second-instance` 聚焦**）；**P5 已加退出显式落盘**（`will-quit` 中 `closeDatabase()`→`flushDatabase()` 冲掉防抖定时器）。

### 功能 / 产品

- ~~设置页静态 + 版本硬编码；缺真实设置项~~（**P1 已加主题设置项 + 真实版本/平台**）。
- ~~菜单 `menu:import-plugin` 无渲染层监听~~（**P6 已加 `menu.onImportPlugin` → 跳转市场并打开导入弹窗**）。
- ~~插件无更新能力（同名安装报"已安装"）~~（**P6 已支持：同名更高版本直接升级，保留 id/配置/启用态，禁止降级与同版本覆盖**）。
- ~~缺插件日志查看页~~（**P6 已加 `PluginLogs` 页 + `plugin.getLogs/clearLogs`**）。

### 工程

- ~~零测试、无 CI~~（**P6 已加 Vitest（28 用例）+ GitHub Actions CI**）。
- `plugin-system/semver.ts`（版本比较）、`plugin-system/pluginPaths.ts`（协议路径纯函数）为可单测纯模块。
- `electron-builder` 需确认 `sql-wasm.wasm` 打入安装包。
- DiceRoller 只有 `dist/` 无源码（P0 反向补齐 src）。

## 5. 主题系统设计（重点交付）

### 5.1 契约：CSS 变量 `--ob-*` + antd token + React prop

- 新 `shared/types/theme.types.ts`：
  - `ThemeMode = 'light'|'dark'`
  - `ThemeTokens`：`colorBg/colorBgLayout/colorBgContainer/colorBgElevated/colorPrimary/colorPrimaryHover/colorPrimaryBg/colorText/colorTextSecondary/colorTextTertiary/colorBorder/colorBorderSecondary/colorSuccess/colorSuccessBg/colorWarning/colorWarningBg/colorError/colorErrorBg/colorLink/borderRadius/fontFamily?`
  - `ToolboxTheme { id,name,mode,tokens }`
- 预设 `shared/themes/presets.ts`（亮=默认沿用 #555、深、2–3 组配色）；自定义 `{id:'custom',...}` 存 settings key=`theme`（JSON）。

### 5.2 主进程 `electron/theme.ts`（或并入 ipc）

- IpcChannel 新增：`ThemeGet / ThemeSet / ThemeList / ThemeChanged`（main→renderer）。
- set 时：持久化 → 广播所有窗口 → 向插件主进程发 EventBus `openbox:theme-changed`。

### 5.3 渲染层

- `src/store/theme.store.ts`（zustand）+ `src/components/ThemeProvider.tsx`：
  - 根组件挂载时 `theme.get()`；`useEffect` 把 `--ob-*` 与 `color-scheme` 写到 `document.documentElement`；
  - 计算 antd `ConfigProvider`（defaultAlgorithm/darkAlgorithm + token），替换 `MainLayout` 中硬编码。
- `MainLayout`/pages 硬编码灰白/边框改读 antd token 或 `--ob-*`。

### 5.4 插件联动（三渠道）

1. **CSS 变量**（主渠道，注入 `<style>`/内联 `var(--ob-*)`；同一 document 时切主题即时生效）。
2. **React prop**：`PluginRenderProps` 加可选 `theme?: ToolboxTheme`；`PluginHost` 从 store 注入快照。
3. **运行时订阅（进阶）**：`PluginHost` 的 `require` 追加 `'openbox-theme'`（getTheme/subscribe）。
4. 主进程插件：`ctx.api.onEvent('openbox:theme-changed', fn)`。

### 5.5 权限与 API 扩展

- `Permission` 增 `ThemeWrite='theme:write'`。
- 渲染插件 `api.theme = { get():Promise<ToolboxTheme>, set(t):Promise<boolean> }`。
- 同步更新：Preload、`plugin.api.ts`（`window.electronAPI` 类型）、`template/plugin-template/src/openbox-api.d.ts`、以及 4 个插件各自的 `openbox-api.d.ts`。

## 6. 主题插件（`E:\OpenBox_Plugins\ThemeManager`）

- `plugin.json`：name=`theme-manager`，permissions=`["theme:write"]`。
- 渲染：主题卡（内置亮/暗/预设）点击应用；自定义颜色（原生 `input[type=color]`，因插件无 antd）；恢复默认；导入/导出 JSON。
- 无 antd 等第三方必依赖；构建脚本与 Diary 同标准（esbuild，renderer 外部化 `react` + `openbox-plugin-api`）。

## 7. 插件适配清单（token 映射 例）

- `--ob-colorBgContainer`(面板) / `--ob-colorBg`(页底) / `--ob-colorPrimary`(主色) / `--ob-colorText` / `--ob-colorTextSecondary` / `--ob-colorBorder` / `--ob-colorSuccess|Warning|Error(+Bg)` / `--ob-radius`。
- **Diary**：`src/styles/diary.css` 全量硬编码 hex → `var(--ob-*)`；renderer 注入 `<style>` 逻辑不动。
- **UniEnv**：`renderer.tsx` 顶部 `COLORS` 常量 → 从 `getComputedStyle` 读 `--ob-*` + 订阅更新函数化。
- **Turntable**：面板/文字/主色内联 → var()；canvas 中轴/指针深色用 `--ob-colorText` 衍生；扇区颜色仍走用户 `themeColor`。
- **DiceRoller**：P0 先反推补齐 src，再迁移为 var() 并重建。

## 8. 分阶段实施（状态标注）

- [x] **P0** DiceRoller 反推 src 工程，重建等价 dist
- [x] **P1** 主题核心：types + presets + theme IPC + store + ThemeProvider + antd 主题 + CSS 变量注入 + MainLayout 去硬编码 + 设置页主题项
- [x] **P2** 插件 API 契约：ThemeWrite 权限、`api.theme`、PluginHost `theme` prop、`openbox-theme` require、模板与各插件 d.ts 更新
- [x] **P3** ThemeManager 主题插件（选择/自定义/恢复/导入导出）
- [x] **P4** 四插件适配 + 重建 dist（Diary CSS、UniEnv COLORS、Turntable、DiceRoller）
- [x] **P5** 安全/稳定性：权限断言、openDialog 真实实现、`plugin://` 校验、CSP、落盘防抖+原子写、user_version 迁移、退出落盘、单实例锁
- [x] **P6** 产品补齐：Cmd+I 监听、真实版本设置页、插件更新能力、插件日志页、README、vitest+CI
- [x] **P7（独立里程碑）** 插件主进程子进程隔离（`useProcess=true` + DB API 异步）、sql.js→better-sqlite3 评估（doc-only：`docs/db-engine-evaluation.md`，建议 P9 单独阶段迁移）。
- [x] **P8** main→renderer 插件事件广播：`PluginManager` 生命周期/日志/消息补齐发送方，`electron/pluginEvents.ts` 桥接 `webContents.send`，渲染侧同步 `activePlugins` + 卡片实时状态。
- [x] **P9（已完成）** sql.js→better-sqlite3（见评估文档）。

## 9. 通用做法

- 不添加解释性注释；延续现有代码风格（2 空格缩进、单引号、分号）。
- 插件改动后到对应目录跑 `npm run build` 重建 dist；OpenBox 改动后跑：
  ```
  npm run typecheck
  npm run lint
  npm run dev
  ```
- 新插件/模板同步 `openbox-api.d.ts`。
- 仓库尚未 git 初始化，禁止随意 commit；若用户要求提交/建 PR，先与用户确认。

## 10. 进度

- 初次分析（已完成）：勘察代码库并确认方案（范围=全量改进；主题=预设多套+自定义；DiceRoller=反推补源码）。
- **P1 已完成**：主题核心已落地并验证（typecheck/lint/build/冒烟全绿）。
  - 新增：`shared/types/theme.types.ts`、`shared/themes/presets.ts`、`shared/themes/css-vars.ts`、`electron/theme.service.ts`（get/set/list + 广播 `theme:changed`）、`electron/ipc/app.ipc.ts`、`src/theme/antd.ts`、`src/store/theme.store.ts`、`src/components/ThemeProvider.tsx`、`src/api/theme.api.ts`。
  - 修改：`ipc.types.ts` 增 Theme 通道；`preload.ts` 增 `theme.*`；`plugin.api.ts` 全局类型增 `theme`；`App.tsx` 包裹 `ThemeProvider`；`MainLayout.tsx` 去硬编码改 antd token；`Settings.tsx` 加主题选择 + 真实版本/平台。
  - 顺手修复（预置 bug）：`app:get-version`/`app:get-platform` 此前无 IPC handler（冒烟暴露）；`PluginProcessEntry.ts` 的 `require()` 触发 lint error → 改 `createRequire`。
  - 环境注记：`E:\OpenBox\node_modules` 于本阶段首次 `npm install`（726 包）；electron/esbuild 的 postinstall 被系统 allow-scripts 策略拦截，但二进制已在位，`npm run dev` 可直接使用。
- **P2 已完成**：插件 API 契约（typecheck/lint/build/冒烟全绿；Diary、UniEnv 插件用新 d.ts 构建通过）。
  - `Permission` 新增 `ThemeWrite='theme:write'`（含中文描述）。
  - `PluginRenderProps` 新增可选 `theme?: ToolboxTheme` 与 `api.theme.{get(),set()}`（共享类型 + 模板 + Diary/Turntable/UniEnv 的 `openbox-api.d.ts` 四份已同步）。
  - `PluginHost`：从 theme.store 注入 `theme` prop；`api.theme.set` 按 `permissions.includes(Permission.ThemeWrite)` 门控；`require` 沙箱新增 `'openbox-theme'`（`getTheme/subscribe`，基于 zustand）。
  - `PluginView` 透传 `plugin.permissions` 给 PluginHost。
  - 主进程链路：`registerThemeIpc(undefined, cb)` → `PluginManager.notifyThemeChanged()` → EventBus 发 `openbox:theme-changed`（插件主进程用 `ctx.api.onEvent('openbox:theme-changed', fn)` 订阅）。
  - [x] P0/P4 内联在进度小节（见下）。
- **P3 已完成**：主题管理插件建成并构建通过（tsc exit 0）。
  - 工程：`E:\OpenBox_Plugins\ThemeManager\`（package/tsconfig/esbuild.config.mjs，构建方式同 Diary）。
  - `plugin.json`：name=`theme-manager`、displayName=`主题管理`、permissions=`["theme:write"]`、config 空。
  - `src/main.ts`：订阅 `api.onEvent('openbox:theme-changed')` 记日志，deactivate 时退订。
  - `src/renderer.tsx`：内置主题卡片（亮/深/清新绿/海洋蓝，点击 `api.theme.set` 应用并高亮当前）；自定义主题（亮/深切换 + 5 个原生 `input[type=color]`：主色/页面背景/面板背景/文字/边框 → 组装 `id:'custom'`）；重置；导出当前主题 JSON / 导入 JSON（FileReader）。UI 全部用 `var(--ob-*)` 跟随主题。
  - 打包：`E:\OpenBox_Plugins\releases\theme-manager-0.1.0.zip`（根含 `plugin.json + dist/`，已验证）。
- 注意：插件内 `PRESETS` 与 `shared/themes/presets.ts` 存在小量数据重复（沙箱无法 require 共享模块导致），如需消除可在 P6 后用 `openbox-theme` 扩展 `listPresets()`。
  - 2026-08 微调：设置页移除主题选择 UI（含「当前主题」标签），主题功能全部交由 ThemeManager 插件；`theme.store` 精简（删 `applyTheme`/`presets`，仅保留 `theme`/`init`/`onChanged`）；`shared/themes/presets.ts` 的 `ocean` 与插件 `PRESETS` 同步改为深蓝配色（`#1677ff` 主色 + `#0f172a`/`#1e293b` 底），与 `dark` 区分；ThemeManager 升 0.1.1 重建 dist 并重打包 `releases/theme-manager-0.1.1.zip`。
  - 验证方式：`npm run dev` → 插件管理导入 zip/目录 → 打开「主题管理」切主题，工具箱整体即时换肤；重启保持。
- **P0/P4 已完成**：DiceRoller 反推源码工程 + 四插件全部主题化并重建 dist（typecheck 全绿，`var(--ob-*)` 已进产物）。
  - DiceRoller：原只有 `dist/`，现补齐 `package.json`（tsc 构建，同 Turntable）、`tsconfig.json`、`src/main.ts`（生产原样：activate 日志 + ping）、`src/renderer.tsx`（反推 dist 行为，逻辑等价）与本地 `src/openbox-api.d.ts`；`npm install` 后 `npm run build` 产出等价 dist，`tsc --noEmit` exit 0。
  - Diary：`src/styles/diary.css` 全量改写，顶部 `.diary-app` 定义 `--d-*` 别名映射到 `--ob-*`（带原色 fallback），60 余处硬编码色全部替换；esbuild 构建通过。
  - UniEnv：`COLORS` 常量全部改为 `var(--ob-color-*, 原值)` 字符串；Toast/btnDanger/内联 code 残留色同步处理；tsc 构建通过。
  - Turntable：内联样式批量换 `var()`；canvas 中轴/指针/扇区分隔线/外圈的 `#fff/#333/#ddd/#ff4d4f` 改为 `cssVar()` 运行时读 `getComputedStyle`（canvas 不解析 CSS 变量）；增加 MutationObserver 监听 `documentElement.style` 变化以在切主题时重绘转盘；tsc 全绿。
  - 主题一致性修正：`shared/themes/presets.ts` 深色主题主色由 `#9a9a9a`（白字对比弱）改为 `#4096ff`（hover `#69b1ff`、primaryBg `#1e3a5f`）；同理同步 ThemeManager 插件内 PRESETS 深色主色并重建。根工程 `npm run typecheck` 通过。
  - 验证方式：`npm run dev` → 插件管理导入各插件（更新/重装）→ 打开「主题管理」切深色/海洋蓝等 → Diary/骰子/转盘/环境工具界面整体换肤；转盘 canvas 随主题重绘。
- **P5 已完成**：安全/稳定性加固（typecheck exit 0、lint 0 errors / 2 条 P6 待清警告、路径校验单测式冒烟通过）。
  - 权限：`Permission` 新增 `FileRead('file:read')/FileWrite('file:write')` 及中文描述；`PluginManager` 的 `context.api` 全部加 `PermissionGuard.assert`：notify→Notification、openDialog→Dialog、fetch→NetworkFetch、readFile→FileRead、writeFile→FileWrite（registerShortcut/database 原有）；`PluginSandbox` 子进程代理同样补 FileRead/FileWrite（P7 备用）。现有插件未直接使用这些 API，无破坏。
  - openDialog：`PluginManager` 内 `async () => null` → 真实 `dialog.showOpenDialog`（`file`/`folder`）。
  - `plugin://` 协议：`join` → `resolve(pluginsDir, pluginName)` + `resolve(pluginDir, '.'+pathname)` 双前缀校验，`../`、编码 `%2e%2e`、绝对路径、宿主名逃逸均 404（node 冒烟 7 用例全过）。
  - 数据库：`execute` 每次全量同步落盘 → `persistDatabase()` 防抖 500ms 合并；`saveDatabase` 写 `*.tmp` 后 `renameSync` 原子替换；`flushDatabase()` 立即落盘；迁移改为 `PRAGMA user_version` 版本化 `MIGRATIONS` 数组（v1=建表）；启动时清理 30 天前 `plugin_logs`。
  - 主进程：单实例锁（`requestSingleInstanceLock` 失败即 quit + `second-instance` 聚焦）；`F12` devtools 仅开发模式（`app.isPackaged` 时跳过）；`will-quit` 中 `closeDatabase()` 冲掉防抖定时器兜底落盘。
  - 注：Turntable 主进程注释提到"每次写入后 export 会重置 last_insert_rowid"——防抖后该现象缓解，其按 `sort_order` 定位的新增项逻辑仍兼容，未改动。
- **P6 已完成**：产品补齐（typecheck/lint/test/build/dev 冒烟全绿；未 git 提交）。
  - Cmd+I 菜单监听：`preload.menu.onImportPlugin` 桥接 `menu:import-plugin`；`app.store` 增 `pluginImportOpen`（原 PluginMarket 本地状态上移）；`App.tsx` 监听菜单事件 → 跳转市场页并打开导入弹窗。
  - 真实版本设置页：P1 已完成（`Settings.tsx` 读取 `app:get-version/platform`），本阶段复核。
  - 插件更新能力：`PluginManager.installFromDirectory`（现为 async）同名时比较版本——更高版本走 `upgradePlugin`（停旧沙箱→替换文件→`PluginRepository.updatePluginVersion` 保留 id/配置/启用 → 若原启用则重新激活）；同版本报"已安装（如需覆盖请先卸载）"、更低版本报"无法降级"。IPC 安装失败回滚仅在全新安装时卸载（升级失败不回滚删除原插件）。
  - 插件日志页：新增 `PluginLogs` 页（筛选插件/级别、实时订阅 `plugin:log` 追加、清除全部或清当前插件）；`PluginManager.getLogs/clearLogs`；IPC `plugin:get-logs` / `plugin:clear-logs`；preload/类型/`plugin.api.ts` 同步。
  - README.md：项目简介、技术栈、快速开始、插件开发说明、目录结构。
  - Vitest + CI：`tests/`（permission/semver/pluginPaths/themes 28 用例全绿）、`vitest.config.ts`（node 环境 + `@shared/@database` 别名）、package.json `"test": "vitest run"`、`.github/workflows/ci.yml`（win-latest：typecheck→lint→test→build）。
  - 顺手清理：`semver.ts`/`pluginPaths.ts` 纯函数抽取；`plugin.ipc.ts` 安装回滚区分升级；`ipc.types.ts` 空接口 lint 错误、`plugin.repository.ts` 未用 `ConfigField`、`PluginConfig.tsx` exhaustive-deps 三处 lint 警示清零。【此前 P5 记录的 2 条待清 lint 除外，本阶段已清】
- **P7 已完成**：插件主进程子进程隔离 + DB API 异步（typecheck/lint/vitest 28 用例/build/dev 冒烟全绿；未 git 提交）。
  - 隔离架构：默认 `useProcess=true`（`OPENBOX_PLUGIN_PROCESS=0` 回退进程内），插件 main 由 `fork` 独立子进程运行。
    - 子进程端 `PluginProcessEntry.ts` 重构为 RPC 客户端：`init` 只发 `{id, config}`（不再序列化函数），本地构建 ctx；`database.*`/`logger.*`/`api.*` 逐项代理到主进程；`onEvent` 以 `api:subscribe/unsubscribe`（带 subId）订阅，主进程经 `pushEvent` 把事件推回；快捷键回调经 `openbox:shortcut` 事件回传；`fetch` 在主进程返回泛化 `{ok,status,statusText,headers,body}` 再 `new Response` 还原。
    - 父进程端 `PluginSandbox.ts`：`handleChildMessage` 统一路由 `response/rpc-response/error/started/rpc`；超时（启动 30s、调用 30s）与崩溃清理 pending；`deactivate`→3s 强杀；`pushEvent` 支持事件推送。
    - 权限：主进程权威（`PluginManager.handleChildRequest` 直接走带 `PermissionGuard` 的 async `runtime`），子进程请求发起即被断言。
  - DB API 异步（破坏性变更）：`PluginContext.database.query/execute` 改 `Promise`；Turntable（`ensureTable/pickColor/handleMessage` 全 async）、Diary（`getEntryByDate/getEntriesInMonth/handleSave/Delete/Export*` 全 async）、两插件 `openbox-api.d.ts` + 模板 + 全部 5 插件 d.ts 同步；`dist` 已重建。注意 Diary tsc --noEmit 的 `katex.css/diary.css` 模块声明缺失为预置问题，不影响 esbuild 构建。
  - 其他：`handleChildRequest` 覆盖 db/table/log/notify/open-dialog/fetch/read-file/write-file/register-shortcut/emit-event/subscribe/unsubscribe；子进程异常码 `exit` 记录 error 并 `runPluginCleanups` 回收订阅/快捷键。
  - 评估：`docs/db-engine-evaluation.md`——better-sqlite3 全面优于 sql.js（ACID/WAL/事务/无全量序列化），成本在 native ABI 构建与打包；建议独立 P9 迁移，P7 的异步接口与子进程协议与之无关无返工。
  - 冒烟证据：dev 启动 `Database initialized → Plugin manager ready → 插件已激活: turntable/dice-roller/diary/unienv/theme-manager（子进程隔离）`，stderr 空；各插件 `activate` 内 `await db.execute(CREATE TABLE)` 成功后子进程才回 `started`，证明 DB RPC 全链路通。
  - 验证附加物：dev 时为确认隔离模式添加 `插件已激活: <name> (<隔离/进程内>)` 控制台行。
  - 已知缺口（供给）：~~main→renderer `PluginMessage/PluginLog/PluginStatusChange` 主进程无发送方~~（**P8 已补**）。
- **P8 已完成**：插件事件广播（typecheck/lint/vitest 28 用例/build/dev 冒烟全绿）。
  - 发送方：`PluginManager` 新增 `emitStatus`（`activating/active/deactivate→inactive`、异常→`error`），`sandbox.on('message')` 增加通用 `plugin:message`（`{pluginId,message}` 供广播；原 `plugin:message:${id}` 保留）；退出码非 0 才发 error 状态（避免正常停用污染）。
  - 桥接：新增 `electron/pluginEvents.ts` `registerPluginEventBridge`——`plugin:log`→`IpcChannel.PluginLog`、`plugin:status`→`PluginStatusChange`、`plugin:message`→`PluginMessage`，`webContents.send` 广播到所有窗口（初始化即注册，早于窗口）。
  - 渲染侧：`plugin.store` 新增 `setPluginStatus`；`App.tsx` 全局订阅 `onStatusChange` 同步 `activePlugins`；`PluginCard` 增加实时状态 Tag（运行中/启动中/停用中/异常/已停止），Home 与市场页传入；日志页 P6 直连的 `onLog` 现可收到真实流。
  - 说明：`activateAllEnabled` 在 `createWindow` 前执行，启动期状态事件无窗口可广播——渲染侧靠 `fetchPlugins` + 启用后事件补齐，无功能影响。
- **P9 已完成**：sql.js→better-sqlite3 双引擎迁移（typecheck/lint/vitest 28 用例/build 全绿；better/sqljs 双引擎 dev 冒烟均通过，stderr 0 错误；未 git 提交）。
  - 双引擎：`database/index.ts` 抽出 `EngineDb` 接口（close/version/exec/run/all/get），`BetterEngine`（better-sqlite3，WAL）与 `SqlJsEngine`（sql.js，A/B fallback）并行；`OPENBOX_DB_ENGINE=sqljs` 环境变量选引擎，默认 better。仓储层 `database/repositories/*` 与插件 RPC 协议零改动。
  - 数据兼容：better 打开既有 `openbox.db`（sql.js 导出即完整 SQLite 文件）无需搬迁；启动时若 `openbox.db` 存在且无 `.bak-sqljs` 则先做字节级备份（`copyFileSync`）；`PRAGMA user_version` 迁移、30 天日志清理在 better/sqljs 下同路径执行。
  - native ABI 关键坑：better-sqlite3 **12.x 起无 N-API 通用 prebuild，走 per-ABI V8 prebuild**（node-v137=Node24 / 对应 Electron 需 132）。13.x 的 prebuild 要求 Node≥22（NAPI 版本过高），在 Electron 34（Node 20.19，NAPI 9）下 `new Database()` 直接崩溃（exit -36861）；12.11.1 engines 支持 20.x，但 prebuild 需按运行时分别下载。**结论：Electron 环境必须 electron-rebuild 编译 Electron ABI（132）**，仅靠 prebuild 在 Electron 会崩或 ABI 不匹配。
  - 版本锁定：better-sqlite3 由 10.1.0 升级到 `^12.11.1`（engines 覆盖 Node 20.x，electron-rebuild 可用 MSVC 编译通过）；`@types/better-sqlite3@^9.6.0` 保持。package.json `allowScripts` 更新为 `better-sqlite3@12.11.1: true`（放行其 install 脚本）。
  - 构建自动化：新增 `scripts/rebuild-native.js`（CI 环境跳过，本地执行 `electron-rebuild -f -w better-sqlite3`，**必须以项目根为 cwd**），挂到 `"postinstall"`；`@electron/rebuild@^4.2.0` 加入 devDependencies。注意 electron-rebuild 产物是 Electron ABI，普通 Node 无法加载（CI 的 vitest 不 import database 模块，无影响）。
  - 打包：`electron-builder.yml` 已有 `asarUnpack: node_modules/better-sqlite3/**`；electron-builder 打包时自动 rebuild native。
  - 冒烟证据：better 引擎 dev 启动输出 `[DB] engine: better-sqlite3 (WAL) → Database initialized → Plugin manager ready`，5 插件全激活，stderr 0 行；sqljs 引擎输出 `[DB] engine: sqljs (A/B fallback) → Database initialized → Plugin manager ready`，插件激活同 better；两种引擎下 `openbox.db` 均正常读写（WAL 模式 better 引擎关闭时自动 checkpoint，切换 sqljs 无数据丢失）。
  - 清理：根目录调试脚本（fix-vcxproj*.ps1 / patch-_.ps1 / probe_.ps1）已删除。
- **P10 已完成**：插件兼容性修复 + 卸载 EBUSY 修复 + 版本号更新（typecheck/lint/vitest 28 用例/build/冒烟全绿；未 git 提交）。 - **插件旧同步 API 问题**：%APPDATA% 已安装副本曾为 7/1 旧版 dist（P7 之前的同步 DB API，`database.query()` 无 await 返回 Promise）——Turntable `spin` 中 `items.reduce` 因 items 是 Promise 崩溃（"items.reduce is not a function"）、Diary 加载失败。已核实当前开发目录 src/dist（8/7）与已安装副本均为新 async API（turntable await=14、diary await=19），业务代码无 bug，问题来自历史旧版。
  - **EBUSY 修复**：`PluginSandbox.stop()`（PluginSandbox.ts）原发送 `deactivate` 后仅 setTimeout 3s 兜底 kill、**不等待子进程退出**；子进程 fork 时 `cwd: pluginDir` 占用目录，`uninstall`（PluginManager.ts）`stopPlugin` 后立即 `rmSync` → `EBUSY: resource busy or locked`。改为 stop() 发送 deactivate 后 `await` 子进程 `once('exit')`（已退出立即 resolve），3s 兜底 kill + 8s 总保险，退出后再清理 pendingRequests。已用临时 esbuild bundle + Electron 实测：stop() 后立即 rmSync 成功（52ms，无 EBUSY）。
  - **插件版本升级**（P6 同版本禁止覆盖）：turntable 0.1.2→0.1.3、diary 0.4.3→0.4.4、dice-roller 0.1.0→0.1.1、theme-manager 0.1.1→0.1.2、unienv 0.3.2→0.3.3。全部重建 dist（Turntable/DiceRoller/UniEnv=tsc，Diary/ThemeManager=esbuild），打 zip 到 `releases/*-vX.zip`（结构=plugin.json + dist/main.js + dist/renderer.js）。已同步已安装副本（%APPDATA% 的 plugin.json + dist，hash 校验一致）并同步 `openbox.db` 的 `plugins.version` 记录。
  - **清理**：历史 EBUSY 卸载留下的 theme-manager 残留目录（DB 记录已删但 231 文件空壳）已删除；theme-manager 如需恢复可重新从 `releases/theme-manager-0.1.2.zip` 安装。
  - 版本号：主工程 0.9.1→0.9.2，重新打包 `release\openbox-0.9.2-setup.exe`（native 模块验证 PACKED-NATIVE-OK）。
- 待办：无（P10 完成）。
- 预置 lint 警告（非错误，P6 已清理）：`plugin.repository.ts` 未用 `ConfigField`；`PluginConfig.tsx` useEffect 依赖 `plugin`。
- 后续接入插件联动时复用：CSS 变量单源在 `shared/themes/css-vars.ts`（`themeToCssVars`），antd 映射在 `src/theme/antd.ts`（`antdThemeConfig`），主题变化广播通道为 `IpcChannel.ThemeChanged`。

## 11. P11 安全修复（计划，信任模型 A）

> 信任模型 A：插件为**可信代码**，权限声明不再是安全边界；安全控制点改为 **安装时用户确认 + 渲染隔离 + IPC 校验 + 供应链可追溯**。插件即完全可信，但 UI 不应拥有主进程/全局能力，插件间不应相互干扰。
>
> 版本节奏：每批主工程升 0.0.1（构建不打包）。1.1.2 → **1.1.3（批1）→ 1.1.4（批2）→ 1.1.5（批3）**。
>
> 每批验证：`npm run typecheck` + `npm run lint` + `npm run test`（vitest）+ `npm run dev` 冒烟（导入/卸载/主题/插件消息/日志）+ 6 个插件 dist 重建加载验证。

### 批 1（S1-S3，先做，高危链路）

- [x] **S1 渲染插件受限作用域隔离**（审计 H1；最终采用**受限作用域 new Function + window Proxy**，非 iframe——iframe 自渲染会丢主题 CSS 变量继承）
  - `src/components/PluginHost.tsx`：`new Function('module','exports','require','window','globalThis', code)` 传入**受限 `window`/`globalThis` Proxy**：拦截 `electronAPI` 等敏感属性返回 undefined，标准浏览器 API（addEventListener/confirm/setTimeout/devicePixelRatio）正常放行
  - 插件代码仍在父文档执行（React 渲染、主题 CSS 变量继承不变）；无法访问任何全局 IPC/主进程能力
  - 6 插件 renderer 已确认仅用 `api.*` props，无全局 electronAPI 依赖，无需改 renderer 代码；dist 已重建
- [x] **S2 IPC 调用方/参数校验**（审计 H3）
  - 新增 `electron/ipc/ipcGuard.ts`：`assertTrustedSender`（校验 event.sender 为主窗口 webContents）、`assertAllowedSettingsKey`（theme 白名单）
  - `electron/ipc/*.ts`（plugin/settings/app）+ `theme.service.ts` 全部 handler 加 `assertTrustedSender`
  - `plugin:install`：`assertInstallSource`——路径须来自 dialog 白名单（`trustedInstallPaths`）+ 拖拽路径经 `PluginRegisterImportPath` 注册
  - `settings:set`：key 白名单（theme）
- [x] **S3 安装确认 + 信任文档**
  - `plugin.ipc.ts` PluginInstall 安装前 `dialog.showMessageBox` 确认（插件名/版本/作者/描述 + "插件即可信代码"提示）
  - `PluginManager.previewInstall(source)`：读取待安装插件 plugin.json（不解压安装）
  - `PluginImport.tsx` 拖拽路径先 `plugin.registerImportPath` 再 install
  - 信任模型 A 已文档化到本 AGENTS.md

### 批 2（S4-S9，主进程加固）

- [x] S4 CSP 收紧（`index.html`）：加 `object-src 'none'`、`base-uri 'none'`、`frame-src 'none'`、`font-src 'self' data:`（`'unsafe-eval'` 保留——受限作用域 new Function 需要）
- [x] S5 zip 限制：`PluginManager.validateZipArchive`——压缩包 ≤200MB、条目 ≤5000、解压总量 ≤600MB
- [x] S6 `main.ts`：`setWindowOpenHandler` 仅 http/https；加 `will-navigate` 拦截外部导航
- [x] S7 `theme.service.ts`：`sanitizeTheme` token 白名单校验（hex/rgb 颜色格式、长度 ≤128、fontFamily 禁 url()/;）
- [x] S8 `menu.ts`：生产（`app.isPackaged`）移除 reload/forceReload/toggleDevTools 菜单项
- [x] S9 权限语义文档化（README：信任模型 A + `file:read/write`/`database:*` 语义）

### 批 3（S10-S16，健壮性）

- [x] S10 事件总线按插件隔离：`runtimeApi.emitEvent/onEvent` 加 `plugin:${id}:` 前缀
- [x] S11 fetch 限制：`runtimeApi.fetch` 30s 超时 + 响应 ≤50MB
- [x] S12 `PluginSandbox.stop()` 兜底 `child.kill('SIGKILL')`
- [x] S13 日志限制：`log()` 截断 message ≤4000，每插件最多 2000 条（trimPluginLogs）
- [x] S14 `copyRecursive`/`resolvePluginRoot` 用 `lstatSync` 跳过 symlink
- [x] S15 `PluginProtocol` CORS 收紧（仅应用自身 origin）；`PluginCard` icon 协议白名单（plugin://、data:、相对路径）
- [x] S16 临时目录 `.tmp-${randomUUID()}`（替代 Date.now）

- **P11 已完成（1.1.5）**：安全修复三批全部落地（typecheck/lint/vitest 28/build 全绿；未 git 提交、未打包）。S1-S3 批1（渲染受限作用域隔离、IPC sender 校验、安装确认）、S4-S9 批2（CSP/zip/openExternal/主题token/DevTools/文档）、S10-S16 批3（事件隔离/fetch限制/SIGKILL/日志/symlink/CORS/tmp）。关键取舍：信任模型 A=插件即可信代码，权限声明为功能门控非安全边界；'unsafe-eval' 保留（受限作用域 new Function 需要）。

## 12. P12 UI 现代化 + 赛博朋克主题（1.3.0，已完成）

> 版本：1.2.5 → **1.3.0**。验证：typecheck/lint/vitest 28/build 全绿 + dev 冒烟（6 插件子进程隔离激活，stderr 空）+ Portable 冒烟（主窗口 CrucibleBox，10 进程稳定，停止零残留）。

### 主工程 UI 现代化

- **品牌主色**：light/dark 主色改 **indigo**（light `#6366f1`/hover `#818cf8`/bg `#eef2ff`；dark `#818cf8`/`#a5b4fc`/`#2a2b52`）；背景现代灰阶（light `#f5f6fa`、dark `#0b0d11`/`#171a21`/`#1e2230`）；`borderRadius` 8→10。
- **硬编码清零**：约 21 处硬编码色（PluginCard/PluginMarket/Home/PluginView/PluginConfig/PluginImport/滚动条）全部改 antd token，暗色/自定义主题下不再错色。
- **MainLayout**：Header 加当前页标题+副标题 + 右侧当前主题徽标（主色发光圆点+主题名）；Sider logo 加渐变 accent 线；Content 加柔和阴影。
- **PluginCard**：渐变图标底、柔和阴影、hover 上浮（translateY + 阴影加深）。
- **Settings**：卡片化 + 主色图标。
- **global.css**：滚动条走 `--ob-*` 变量；新增 `[data-ob-theme='cyber']` 特效块（切角 clip-path、主按钮霓虹辉光、选中菜单辉光、Header 底部数据流动画、`prefers-reduced-motion` 降级）。
- `ThemeProvider` 写 `root.dataset.obTheme = theme.id` 供 cyber 选择器。

### cyber 赛博朋克预设（完整版）

- `presets.ts` 新增 `cyber`（dark）：底 `#020204/#0a0a0d`、主色青 `#00e5ff`、酸黄 `#fce205`、玫红 `#ff003c`、monospace 字体、radius 6。
- 主程序 + 全插件（走 `--ob-*` 变量）切换 cyber 自动同步颜色与切角/辉光特效。

### 插件联动修复

- `css-vars.ts` 派生补 3 个变量：`--ob-color-{success,warning,error}-border`（=对应主色）——**UniEnv 用到但此前未定义**，cyber 下回退到亮色 fallback 刺眼，现已随主题正确变化。

### ThemeManager 插件 0.1.3 → 0.1.4

- PRESETS 同步加 cyber + light/dark 主色改 indigo；FALLBACK 默认主色 `#1677ff`→`#6366f1`；主题卡片加柔和阴影/hover 提升，cyber 卡片加霓虹角标。
- 重建 dist，重打包 `releases/theme-manager-0.1.4.zip`（结构 plugin.json + dist/main.js + dist/renderer.js 已验）；已同步 %APPDATA% 已安装副本 + openbox.db plugins.version 记录（sqlite3 CLI 更新，普通 Node 因 Electron ABI 无法加载 better-sqlite3）。

### 发布产物

- 主程序 1.3.0：`release/openbox-1.3.0-setup.exe` + win-unpacked；`app.asar` 已同步 Portable 版（E:\OpenBox-Portable\CrucibleBox\resources\app.asar）。
- 插件：`releases/theme-manager-0.1.4.zip`。

### 待办 / 说明

- 验证 UI 视觉需人工：启动 Portable → 打开「主题管理」→ 依次切 5 主题，确认工具箱+各插件换肤、cyber 下切角/辉光/扫描线生效。
- 其它插件（Diary/Turntable/DiceRoller/UniEnv/gif-editor）本次零代码改动（架构自动同步），gif-editor 选区玫红、turntable 扇区色为业务语义色保留。

## 13. P13 启动器式 UI 重构 + cyber 主题深度强化（1.4.0，已完成）

> 版本：1.3.0 → **1.4.0**。验证：typecheck/lint/vitest 28/build 全绿 + dev 冒烟（6 插件子进程激活，渲染窗口正常）+ Portable 冒烟（主进程+6 插件激活，stderr 空，持续运行）。

### 布局结构重构（告别管理后台骨架）

- **64px 图标栏**（`src/components/IconRail.tsx` 新增）：垂直图标导航（首页/插件管理/插件日志/设置），选中 = 主色渐变药丸 + 发光；tooltip。替代原 220px 文字 Sider。
- **全局命令面板 Cmd+K**（`src/components/CommandPalette.tsx` 新增）：渲染层 keydown 监听 Ctrl+K；居中浮层 + backdrop blur；搜索插件/页面，↑/↓ 选择 + Enter 直达 + Esc 关闭；由 `app.store.commandOpen` 控制。
- **启动器首页**（`Home.tsx` 重写）：CSS Grid 大卡网格（`repeat(auto-fill, minmax(200px,1fr))`），LauncherCard 悬浮卡 hover 上浮 + 边框发光 + 状态点。
- **渐变字母图标**（`src/components/PluginGlyph.tsx` 新增）：插件无 icon（6 插件均无 icon 字段），name hash → 8 组双色渐变 + 首字母，56px 圆角方块。
- **启动器大卡**（`src/components/LauncherCard.tsx` 新增）：Home 与 PluginMarket 共用；hover 显示启用/配置/删除操作；PluginCard.tsx 已无引用删除。
- **插件全屏沉浸**（`PluginView.tsx` 调整）：顶部轻量返回栏（返回按钮 + 插件名 + 状态 Tag），Content 透明无白卡包裹。
- **MainLayout** 重写：64px Sider + 顶栏（页面标题/副标题 + 搜索触发按钮 Ctrl K + 主题徽标）+ Content 透明（margin 0 28px 28px）。

### cyber 主题深度强化（仅 `[data-ob-theme='cyber']`，global.css）

- **透视网格地平线**：布局背景双层网格线（80px/20px）+ 底部青色径向光晕。
- **HUD 角标**：Content 四角 L 形 bracket（左上青、右下玫红发光）。
- **hex 数据流**：图标栏底部 monospace hex 字符流渐隐漂移动画。
- **切角扩展**：clip-path 覆盖 antd 组件 + .ob-launcher/.ob-palette/.ob-rail-btn。
- **三色霓虹**：主按钮青辉光、danger 按钮玫红辉光、warning Tag 酸黄辉光。
- **扫描线**：launcher 卡 hover 底部青色扫描线；主按钮 hover 辉光增强（弃用伪元素扫过方案避免遮挡 antd 按钮文字）。
- **glitch**：Sider logo / 搜索按钮 hover 时 RGB 错位抖动。
- 全部纳入 `prefers-reduced-motion` 降级。

### 发布产物

- 主程序 1.4.0：`release/openbox-1.4.0-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。
- 插件本轮无改动。

### 已知取舍

- cyber 下 `.ob-launcher` 切角会裁掉 hover 阴影，用 `filter: drop-shadow` 补偿（filter 不随 clip-path 裁切）。
- 命令面板 Cmd+K 为渲染层监听（非 globalShortcut），窗口失焦时无效——符合"仅应用内"预期。
- 人工验证项：切 cyber 主题查看六大特效是否到位、命令面板搜索/直达、启动器 hover 动效、插件全屏沉浸。

## 14. P14 工作台 IA 合并 + 深色质感 + cyber 辨识度强化（1.4.1，已完成）

> 版本：1.4.0 → **1.4.1**。验证：typecheck/lint/vitest 28/build 全绿 + dev 冒烟（10 electron 进程、6 插件子进程激活、stderr 空）+ Portable 冒烟（主进程+6 插件激活，exited=False，stderr 空）。

### 信息架构合并（核心：解决页面冗余）

- **删除 `src/pages/PluginMarket.tsx`**——"插件管理"与"已安装插件"功能重叠，合并。
- **Home.tsx 重写为「工作台」**：欢迎区（品牌标题 + 「X 个工具 · Y 个运行中」统计）+ 右侧导入插件主按钮/刷新 + 全宽搜索条（点击打开 Cmd+K）+ 插件大卡网格 + 空态导入引导。导入能力复用 PluginImport 弹窗。
- **App.tsx**：删 market 分支，currentPage 剩 home/logs/settings + 内部 pluginView；菜单 `menu:import-plugin` 改跳 home。
- **IconRail.tsx**：导航 4→3（工作台/插件日志/设置）。
- **CommandPalette.tsx**：页面项 4→3（去掉插件管理）。
- **MainLayout.tsx**：PAGE_META 更新（home→工作台），删 market。

### 深色质感风（dark/ocean/cyber 观感统一）

- `presets.ts`：dark 背景分层（colorBgLayout `#08090d`/colorBg `#0a0c10`，容器 `#171a21` 浮起）；cyber 底改深蓝黑 `#060a10/#0a0e14/#0d121a`、border 青色系、radius 8。
- `global.css`：深色系 `.ant-layout` 背景微妙渐变（`linear-gradient` 180deg 分层 + fixed）；`.ant-layout-sider` 半透明毛玻璃（`color-mix` 72% + `backdrop-filter: blur(14px)`）；`.ob-launcher` 细腻边框 `rgba(255,255,255,.06)` + 顶部微光。

### cyber 辨识度强化（从"深色换色"→"一眼赛博朋克"，透明度拉高）

- **网格+透视地平线**：双层网格线透明度 0.06→0.12/0.06（80px/20px）+ 底部青色地平线光晕 0.2 + 左上青/右下玫红环境光晕，静态可见。
- **HUD 角标**：Content 四角 L 形 bracket 透明度提到 0.9 + 发光。
- **hex 数据流**：透明度 0.22→0.5，加 text-shadow 霓虹。
- **切角静态化**：卡片/按钮/面板/搜索框/Select 全部 `clip-path` 12px 切角（不靠 hover）；Sider/Header 因流光/内容裁切风险移出切角列表。
- **霓虹静态化**：主按钮青辉光 `rgba(0,229,255,.45)`、danger 玫红、warning Tag 酸黄文字+辉光、rail 选中药丸青边框 `[data-active='true']`。
- **文字霓虹**：Sider logo + 工作台标题静态 `text-shadow`；glitch 保留 hover 触发（含 glow 回归）。

### 发布产物

- 主程序 1.4.1：`release/openbox-1.4.1-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。
- 插件本轮无改动。

### 已知取舍

- `.ant-layout-sider` 用 `color-mix`（Electron 34/Chromium 130 支持，安全）。
- IconRail 按钮移出切角列表（44px 圆角按钮切 12px 会破坏形态），改用选中态青边框 + 辉光表达赛博感。
- 人工验证项：工作台合并后操作是否顺滑、切 cyber 是否一眼可见网格/切角/霓虹/hex流/文字辉光、搜索条点击弹出 Cmd+K。

## 15. P15 视觉四优化（1.4.2，已完成）

> 版本：1.4.1 → **1.4.2**。验证：typecheck/lint/vitest 28/build 全绿 + dev 冒烟（10 electron 进程、6 插件子进程激活、stderr 空）+ Portable 冒烟（6 插件激活，exited=False，stderr 空）。

### 品牌标题移到页面左上角

- `MainLayout.tsx`：Header 左上角品牌区 = **CrucibleBox**（22px/800 主色）+ 当前页 subtitle（12px 次要色）并列；搜索按钮/主题徽标右移。
- `Home.tsx`：工作台欢迎区**删除大标题**，仅保留「X 个工具 · Y 个运行中」统计行 + 刷新/导入按钮。

### 侧边栏重设计（`IconRail.tsx` + Sider 72px）

- Sider 宽 64→**72px**；品牌区 "CB" 保留主色 22px/800。
- 导航按钮 44→**56px 高、全宽、圆角 12**，改**垂直布局 icon(20px) + 图标名 label(10px)**，替代纯 tooltip 提升辨识度。
- 选中态：主色实底渐变（`colorPrimary → colorPrimaryHover`）+ **白色图标/文字** + 主色外发光 `0 4px 16px ${primary}66`；未选中透明 + 次要色。
- cyber 下选中态加青边框 `[data-active='true']`（与 JS 渐变底配合）。

### 插件图标统一风格 + 随主题（`PluginGlyph.tsx` 重写）

- 弃用 8 组固定彩虹渐变 → **从 `theme.useToken().colorPrimary` 派生**：`linear-gradient(angle, primary, darken(primary, tone))`，angle=100+hash%80、tone=0.55+hash%20/100 仅做细微区分（保证可区分又色系统一）。
- **切换主题时所有插件图标自动换色**（indigo→cyber 青→leaf 绿），风格完全统一；文字白色 + text-shadow + 主色投影。
- `darken()` 工具函数：hex→rgb 按比例压暗（处理 3/6 位 hex）。

### cyber 网格换方案（global.css）

- 弃用平铺方格网格 → **45° 斜纹警示网格**（`repeating-linear-gradient(45deg, #060a10 0 14px, #0a1018 14px 28px)`）+ 横扫描线叠加（`repeating-linear-gradient(0deg, rgba(0,229,255,.035) 0 1px, transparent 1px 3px)`）+ 底部青色地平线光晕 0.22 + 左上青/右下玫红环境光。
- 效果：工业警示感 + 全息扫描，比平铺方格更"赛博朋克"，实现稳定（无 CSS 透视兼容风险）。

### 发布产物

- 主程序 1.4.2：`release/openbox-1.4.2-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。
- 插件本轮无改动。

### 已知取舍

- IconRail 选中态 JS 内联渐变与 cyber CSS 青边框叠加（CSS !important 边框 + JS 背景共存，无冲突）。
- 图标统一方案为纯主色系（最协调）；如需少量色相区分可退化为「主色 + 少量语义变体」。
- 人工验证项：品牌标题是否位于左上角、侧栏图标名可读性、插件图标随主题换色、cyber 斜纹网格观感。

## 16. P16 cyber 主题一致性与视觉强化（当前工作区，已完成）

> 本阶段未调整版本号、未打安装包。验证：typecheck/lint/vitest 30/build 全绿 + 1200×800 离屏视觉验收。

- **预设单源化**：新增 `getPresetTheme()`；宿主读取或写入内置主题 ID 时统一解析为 `shared/themes/presets.ts` 的完整 Token。旧版 ThemeManager 即使携带不完整 cyber Token，也会得到宿主内置的荧光绿/酸黄/玫红语义色。
- **视觉语言强化**：cyber 背景改为深蓝黑网格、扫描线与青/玫红环境光；新增 HUD 状态带、侧栏警示斜纹、竖向 hex 数据流、全屏克制扫描束、全息玻璃面板、金属反光、聚焦霓虹和工业切角。
- **字体与可读性**：全局字体改为读取 `--ob-font-family`；cyber 字体栈补 `Microsoft YaHei UI`/`Noto Sans SC`，标题采用 Bahnschrift/Rajdhani 风格，HUD/数据使用 Cascadia Mono/Consolas；动效继续受 `prefers-reduced-motion` 控制。
- **语义色贯通**：LauncherCard、PluginView、PluginLogs 的状态/日志颜色改为 antd 主题 Token，不再硬编码通用绿/橙/红；cyber 下成功=`#00ff9d`、警告=`#fce205`、错误=`#ff003c`。
- **布局修复**：HUD 角标收回滚动容器内部，避免伪元素制造横向滚动条；补 `ob-main-surface` 最小宽度约束和 820px 窄屏降级。

## 17. P17 独立主题「零号城区」（当前工作区，已完成）

> 完全依据原始赛博朋克需求重新设计，不复用 cyber 的构图或专属 CSS；保留 cyber 作为另一套可选主题。未调整主程序版本号、未打安装包。

- **独立 Token**：新增 `createNeonDistrictTokens()` 与预设 `neon-district`（零号城区）；深蓝黑 `#040711/#080d19`、电光青 `#00e5ff`、荧光绿 `#39ff88`、酸黄 `#fce205`、霓虹玫红 `#ff2b78`、radius 2。
- **独立视觉语言**：纵向城区数据脊线、非对称模块缺口、模块编号、分段三色信号轨、酸黄警戒标识、八边形工具字标与城市控制台标题；未使用 cyber 的对称 HUD 角标、全屏扫描束、六边切角或同套卡片构图。
- **动效与可用性**：使用克制的数据脊线上移、卡片侧边扫描和离散信号闪烁；全部纳入 `prefers-reduced-motion`；聚焦态为酸黄轮廓 + 青色外框。
- **切换渠道**：`themes/neon-district.json` 可由 ThemeManager 直接导入；ThemeManager 0.1.6 增加内置「零号城区」主题卡片；宿主按预设 ID 解析为完整 Token。
- **视觉验收**：1200×800 离屏生产渲染通过，未发现裁字、遮挡或横向滚动条；预览见 `artifacts/neon-district-preview.png`。

## 18. P18 双主题转交包（1.5.0，已完成）

> 主程序版本 1.4.2 → 1.5.0；ThemeManager 保持 0.1.6。验证：typecheck/lint/vitest 32/build 全绿。

- 新增 `themes/cyber.json`，与既有 `themes/neon-district.json` 组成两份可独立导入的主题文件。
- `tests/themes.test.ts` 增加分发 JSON 与内置 Token 同步测试，防止二次开发时发生配置漂移。
- ThemeManager 0.1.6 描述更新，发布包保留两套主题卡片。
- 新增 `docs/theme-release-1.5.0.md`，包含架构、构建、应用方式和兼容性矩阵。
- 生成可整体转交的 `openbox-theme-bundle-1.5.0.zip`：完整源码、插件、两份主题 JSON、两张预览、更新日志和 SHA-256 校验值。

## 19. P19 1.5.0 源码迁移落地（本机 E:\OpenBox，已完成）

> 2026-08 从 `C:\Users\hjc\Desktop\openbox-src-1.5.0` 迁移 1.5.0 源码到本机主工程。验证：typecheck/lint/vitest **32**/build 全绿 + Portable 冒烟（主进程+6 插件激活，exited=False，stderr 空）。

- **迁移方式**：哈希对比确定变更集（13 改 + 3 增），备份旧 13 文件到 `%TEMP%\openbox-1.4.2-backup` 后覆盖，新增 `themes/cyber.json`、`themes/neon-district.json`、`docs/theme-release-1.5.0.md`。16 文件与桌面版 SHA-256 全一致。
- **核心变更**：`presets.ts` 新增 `neon-district` 预设 + `getPresetTheme()`（6 套主题）；`theme.service.ts` 用 `getPresetTheme()` 统一解析内置主题完整 Token（修复旧插件携带不完整 cyber Token 问题）；`global.css` 312→911 行（cyber 对称 HUD + 零号城区城区脊线双主题专属视觉）；LauncherCard/PluginView/PluginLogs 语义色走 antd token。
- **发布**：`release/openbox-1.5.0-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。
- **注意**：ThemeManager 插件本机仍为 **0.1.5**（无内置「零号城区」卡片）。1.5.0 文档推荐 0.1.6（含 neon-district 卡片）。当前 0.1.5 + 主程序 `getPresetTheme()` 下：cyber 可直接选择；零号城区需在「备份/恢复」导入 `themes/neon-district.json`。如需内置卡片，需另建 ThemeManager 0.1.6。

## 20. P20 ThemeManager 0.1.6 迁移（neon-district 主题卡，已完成）

> 从 `C:\Users\hjc\Desktop\theme-manager-0.1.6`（仅编译产物 plugin.json + dist，无 src）迁移。采用**反推 src + 重建 dist**（方案 A），保证源码可维护。

- **反推**：桌面 renderer.js 13726B vs 当前 11190B，仅新增 neon-district 预设（token 与主程序 `createNeonDistrictTokens()` 一致：primary `#00e5ff`/hover `#7df6ff`/bg `#052b38`/container `#0a1220`/layout `#040711`/text `#e6f7fa`/secondary `#91adb6`/border `#175064`/isDark 1）；main.js 无变化；无专属卡片视觉逻辑（同 cyber 卡片结构）。
- **改动**：`src/renderer.tsx` PRESETS 数组追加 neon-district 条目；`plugin.json` 0.1.5→0.1.6 + 描述「内置亮/暗、赛博朋克与零号城区」；`package.json` 0.1.6。
- **重建验证**：`npm run build` 产物 renderer.js 11453B，含 `neon-district` + 完整 token（与桌面一致）。
- **发布**：`releases/theme-manager-0.1.6.zip`（plugin.json + dist/main.js + dist/renderer.js）；已同步 %APPDATA% 已安装副本 + openbox.db plugins.version 0.1.6（enabled 保持）。Portable 冒烟 theme-manager 正常激活。

## 21. P21 五项 UI/功能优化（1.5.1，已完成）

> 版本：1.5.0 → **1.5.1**。验证：typecheck/lint/vitest 32/build 全绿 + dev 冒烟（10 electron 进程、6 插件激活、stderr 空）+ Portable 冒烟（exited=False，stderr 空）。

### 1. 删除顶部短搜索栏

- `MainLayout.tsx`：删 Header 内 `ob-search-btn`（右上角短搜索框）；移除 `SearchOutlined`/`setCommandOpen` 引用。Cmd+K + Home 全宽搜索条仍可用。

### 2. 侧栏顶部 CB → 图标

- 复制 `openbox-neon-64.png`（桌面图标包）→ `src/assets/`；`IconRail.tsx` monogram 区 "CB" 文本 → `<img>`（32px contain）。
- 新增 `src/vite-env.d.ts`（`/// <reference types="vite/client" />`）补 png 导入类型。
- `global.css`：cyber/neon 的 `.ob-monogram` 文字特效（text-shadow/glitch）对 img 无效，改为 `img { filter: drop-shadow(...) }`；monogram hover glitch 改 img glow 增强（brand-wordmark 保留 glitch）。

### 3. cyber 主题清理

- 删 `.ob-rail-shell::before`（左下黄黑条纹）。
- 删 `#root::after` 全屏蓝色扫描动线 + `@keyframes ob-screen-scan`。
- 删 `.ob-main-content::before`（左上蓝色 L 角标），**保留** `::after` 右下玫红角标。
- `.ob-hud-strip` `margin: 0 8px 14px` → `0 0 14px`（左缘与下方内容/搜索对齐）。
- modal 高光：`.ant-modal-content` 从全息表面组拆出，去掉斜向白色高光渐变 `linear-gradient(115deg, rgba(255,255,255,.035)...)`。

### 4. neon-district 主题清理

- 删 `.ant-layout-header::before`（右上黄红条纹）。
- 删 `.ob-main-content::after`（右下黄黑斜纹）。
- `.ob-rail-shell::before` 三条分隔短线（蓝/透明/黄/透明/玫红）→ **单条蓝黄红渐变流光**（`linear-gradient(180deg,#00e5ff,#fce205,#ff2b78)` + `background-position` 流动动画 `ob-district-flow`）。
- modal 高光：拆出 `.ant-modal-content`，去掉 145deg 斜向渐变，保留细线纹理 + 深底。
- `.ob-launcher::before` data-module 编号 `top:9;right:17` → `bottom:12px; right:14px`（右下角，与底部运行中标识下对齐），解决与右上 hover 配置按钮重叠。

### 5. 启动运行计数为 0 bug

- `plugin.store.ts` `fetchPlugins`：获取 plugins 后初始化 `activePlugins`（enabled 插件 → `PluginLifecycleStatus.Active`）。此前 `activePlugins={}` 空对象导致启动时「0 个运行中」，须手动禁用/启用才更新。与启动时 `activateAllEnabled` 已激活全部启用插件一致；个别失败由 `onStatusChange` 覆盖为 Error。

### 发布产物

- 主程序 1.5.1：`release/openbox-1.5.1-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。
- 插件本轮无改动。

### 人工验证项

- 工作台仅剩一个全宽搜索条；侧栏顶部显示霓虹图标（非 CB）；cyber 下无黄黑条纹/扫描动线/左上蓝角标、OBX 条与内容左对齐、弹窗标题右侧无浅色长条；neon 下无黄黑/黄红标识、侧栏为蓝黄红渐变流光、插件卡编号在右下不遮挡配置按钮；启动后「运行中」计数正确。

## 22. P22 双主题视觉统一（1.5.2，已完成）

> 版本：1.5.0 → **1.5.2**（1.5.1 为上一批五项优化）。验证：typecheck/lint/vitest 32/build 全绿 + dev 冒烟（6 插件激活）+ Portable 冒烟（exited=False，stderr 空）。

### 1. OBX → CRUCIBLEBOX 全大写

- `MainLayout.tsx`：hud-strip 文本 `OBX // {hud}` → `CRUCIBLEBOX // {hud}`（cyber + neon 共用，一次修改两主题生效）。

### 2. 弹窗标题底部浅色色块（cyber + neon）

- **根因**：`.ant-modal-content` 背景用 `color-mix(colorBgContainer 88%, transparent)` 半透明层，与页面卡片 `colorBgContainer` 不一致产生突兀浅色块。
- **方案**：两主题 `.ant-modal-content` 背景统一为纯 `var(--ob-color-bg-container)`；neon 去掉 repeating 细线纹理浅色干扰。

### 3. neon-district 主题统一为黄色系

- **hud-strip**：clip-path 右上切角 → **左下切角**（`polygon(0 0, 100% 0, 100% 100%, 16px 100%, 0 calc(100% - 16px))`）。
- **搜索栏**：左侧蓝 `rgba(0,229,255,.7)` → **黄 `#fce205`**；右侧玫红 `rgba(255,43,120,.45)` → 整体一致细框线 `rgba(23,80,100,.6)`。
- **插件卡片左侧粗线**：`border-left` 青 → 黄 `#fce205`（含 hover 态）；卡片左侧光线动效 `.ob-launcher::after` `#00e5ff` → `#fce205` + 黄 glow。
- **插件图标**：`.ob-plugin-glyph` 渐变 `#fce205 → #c99414`（主色偏黄，保留渐变多色层次）+ 黄阴影。
- **侧栏 monogram '00'**：删除 `.ob-monogram::after`。
- **流光动效**：`ob-district-flow` 周期 4s→**9s**、`linear`→`ease-in-out`、加 opacity 0.55→1 柔和过渡。
- **侧栏选中态**：clip-path 改**左下+右上切角**（`... 10px 100%, 0 calc(100% - 10px)`）。
- **主页右上呼吸灯**：`.ob-theme-status > span:first-child` background 加 `#fce205 !important` 覆盖 JS 内联 + glow 黄。
- **导入按钮**：`.ant-btn-primary` 青渐变 → **黄渐变** `#fff3a0 → #fce205 → #d4a017`，文字深色保留。

### 发布产物

- 主程序 1.5.2：`release/openbox-1.5.2-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。
- 插件本轮无改动。

### 环境注记

- 本轮发现 `E:\OpenBox-Portable\CrucibleBox` 曾被运行中进程占用导致 Copy/Get-Item 误报"不存在"；实际结构完整。同步 asar 前须确保无 CrucibleBox/electron/node 残留进程（dev 冒烟进程树需一并清理）。

## 23. P23 neon 渐变与对齐微调（1.5.3，已完成）

> 版本：1.5.2 → **1.5.3**。验证：typecheck/lint/vitest 32/build 全绿 + Portable 冒烟（6 插件激活，exited=False，stderr 空）。

### 1. hud-strip（CRUCIBLEBOX 栏）切角

- `neon .ob-hud-strip` clip-path：左下切角 16px → **8px**，右上加 **8px 对称切角**：`polygon(0 0, calc(100% - 8px) 0, 100% 8px, 100% 100%, 8px 100%, 0 calc(100% - 8px))`。

### 2. 插件图标改浅

- `neon .ob-plugin-glyph` 渐变 `#fce205 → #c99414` → **`#fde68a → #d9a12e`**（柔和浅黄，白字可读）+ 阴影 `rgba(253,230,138,0.25)`。

### 3. 渐变（类似 CRUCIBLEBOX 栏）

- **搜索栏** `.ob-search-btn`：背景叠加与 hud-strip 一致的渐变 `linear-gradient(90deg, rgba(252,226,5,0.07), transparent 28%, rgba(255,43,120,0.055))`（黄→透明28%→玫红微光）。
- **插件标签** `.ob-launcher`：背景叠加渐变 `linear-gradient(90deg, rgba(252,226,5,0.04), transparent 33%)`——**仅黄色渐淡至 33%，无玫红微光**（用户指定）。

### 4. data-module 对齐

- `neon .ob-launcher::before`：`bottom: 12px` → **`bottom: 20px`**（与卡片底部运行中标识行下边缘对齐），`right: 14px` 保持。

### 发布产物

- 主程序 1.5.3：`release/openbox-1.5.3-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。
- 插件本轮无改动。

## 24. P24 零号城区图标主色 #F5ED00（1.5.4，已完成）

> 版本：1.5.3 → **1.5.4**。验证：typecheck/lint/vitest 32/build 全绿 + Portable 冒烟（6 插件激活，exited=False，stderr 空）。

- `neon .ob-plugin-glyph` 渐变改为主色 **#F5ED00**：`linear-gradient(135deg, #F5ED00, #C9B400)` + 阴影 `5px 5px 0 rgba(245, 237, 0, 0.28)`。
- 发布：`release/openbox-1.5.4-setup.exe` + win-unpacked；`app.asar` 已同步 Portable。插件无改动。

## 25. R1 现代化重构 M1 可复现基线（已完成）

> 2026-08-09。六个 `*-src.zip` 已落盘为 `plugins/*` 独立工程；M1 不引入
> workspace 依赖提升，保留各插件 lockfile 作为历史可复现边界。

- 根工程固定 Node 24.15.0 / npm 11.12.x，新增非写入式 `check`、宿主/插件 clean
  build、确定性插件 ZIP、产物白名单验证和 packaged Electron 隐藏窗口冒烟。
- CI 现在安装六个插件依赖、执行 40 个 Vitest、生产构建、插件打包校验、unpacked
  桌面打包与真实启动；native ABI 重建失败不再被吞掉。
- 六个插件的 package/lock/manifest 版本已一致；GIF Editor 为 0.3.1，UniEnv 为
  0.3.4。连续打包哈希稳定，详见 `docs/development.md`。
- 宿主 ASAR 明确排除插件源码、测试、文档、脚本、历史 ZIP 与制品；本阶段没有数据库
  schema 或用户数据迁移。
- 设计决策与延后项见 `docs/adr-0001-reproducible-baseline.md`。下一步优先处理 GIF
  Editor 正确性/资源预算与 UniEnv 输入/路径/shell/长任务问题，再进入宿主安全边界。
- 验收：`npm run check`、`npm run build`、`package:plugins + verify:plugins`、
  `package:dir`、`smoke:packaged` 全部通过；桌面冒烟确认数据库与 renderer 初始化成功。

## 26. R1.1 GIF Editor 正确性与资源安全（0.3.2，已完成）

> 2026-08-09。保持完整画布帧模型，不引入 Worker 或巨型 UI 重写。详细设计与阈值见
> `docs/gif-editor-m1.1.md`。

- 修复非方图 90°/270° 旋转；帧合成改为纯 RGBA 算法，disposal=2 仅清局部矩形，
  disposal=3 恢复前态。
- 新增文档画布不变量守卫；旋转/镜像/裁剪原子作用全部帧，自动裁剪使用当前帧边界
  或所有帧 union，画布变化清理旧选区/mask/图层会话。
- 导入/导出建立 64 MiB 文件、4096 单边、500 帧、256 MiB RGBA 工作预算；网格及
  分层输出有数量和投影内存上限，乘法使用 BigInt。
- 修复 resize 监听、样式卸载、滤镜重复提交；历史保留 frame ID、单次深拷贝并限制
  50 次/256 MiB；缩略图随新增、编辑、undo/redo 增量刷新。
- GIF Editor 新增独立 Vitest；4 文件 35 项通过。根门禁合计宿主 40 项 + 插件 35 项，
  全量 clean build、确定性 ZIP、Electron package:dir 与启动冒烟通过。
- 产物：`artifacts/plugins/gif-editor-0.3.2.zip`，SHA-256
  `07721b81a0cb2eef617a5c456f410711c073171e0e67e4981debfd71892bbefb`。

## 27. R1.2 UniEnv 受控安装任务与无 Shell 执行（0.4.0，已完成）

> 2026-08-09。详细契约、阈值、兼容策略和延后风险见 `docs/unienv-m1.2.md`。

- 严格解析消息/config/custom combos；统一工具版本目录；Windows 安装根路径拒绝 UNC、
  设备 namespace、相对/穿越、保留名和不安全路径段，版本目标做 containment 复核。
- 安装与组合安装立即返回 taskId；TaskManager 提供五态、进度、取消、同一 installation
  资源互斥及有界保留。renderer 每秒轮询、最长 90 分钟，支持取消和卸载清理。
- 五个工具的 12 处字符串 Shell 全部迁为 executable+argv、`shell:false`；进程有超时、
  1 MiB 输出上限、AbortSignal 与等待式 Windows 进程树终止。
- 下载限定 HTTPS、512 MiB、30 秒 body idle，流式 `.part`+fsync+原子 rename；ZIP 使用
  唯一受验证 staging，原子提升且不覆盖已有 runtime；junction 拒绝删除普通目录。
- 权限声明与实际行为对齐为 shell/network/file read/write；不修改数据库/config schema，
  不删除已有版本目录，本轮未运行真实安装器。
- UniEnv 9 文件 116 项、宿主 40 项、GIF 35 项全绿；整仓 check/build、确定性 ZIP、
  package:dir 与 packaged 启动冒烟通过。
- 产物：`artifacts/plugins/unienv-0.4.0.zip`，SHA-256
  `95c14fced9a603d1ceadb08e5159c1443632c7b969e4469f69d41aea3289f04c`。
- 未完成：下载哈希/签名与主机 allowlist、ZIP bomb 二次审计、真实 Windows VM 安装 E2E、
  renderer 重载任务发现，以及宿主层真正的插件能力隔离。

## 28. R2.1 插件生命周期与可恢复事务（已完成）

> 2026-08-09。详细架构、事务边界和兼容说明见
> `docs/adr-0002-plugin-lifecycle-transactions.md` 与 `docs/plugin-platform-m2.1.md`。

- 激活、停止和停用使用 per-plugin single-flight；启动超时/失败会终止并等待子进程退出，
  意外崩溃清理 runtime 引用并允许显式重启。已启用插件在首窗创建后并行恢复。
- `PluginManager` 拆出 `PluginInstallationService` 和 `PluginInstallPreparation`；IPC 公共接口
  不变。用户确认与提交消费同一不可变 stage token，不重新读取原始 ZIP/目录。
- manifest、ZIP 和目录复制建立严格字段、SemVer、权限、路径、符号链接、条目数和容量
  边界；超大 ZIP 在解析前拒绝，候选不能携带宿主 transaction marker。
- install/upgrade/uninstall 使用同卷 rename、补偿事务和持久 journal；启动恢复幂等处理
  prepared/applied/committed 崩溃窗口，歧义状态保留现场并阻止插件激活。
- 升级、卸载、停用和配置写入由维护租约协调；配置启动失败恢复旧配置/runtime，已开始的
  用户停用优先，禁用插件升级后仍保持禁用。
- 原子域仅覆盖代码目录、manifest、宿主元数据和宿主管理配置；插件自建数据迁移、renderer
  跨源隔离和 backend 强制能力边界分别留给 SDK v2、M2.2 和 M2.3。
- 验收：宿主 12 文件/103 项、GIF 35 项、UniEnv 116 项全绿；`npm run check`、生产构建、
  Windows unpacked 打包和 packaged 数据库/renderer 冒烟全部通过。

## 29. R2.2A 受支持 Electron 运行时基线（已完成）

> 2026-08-10。决策与验收见 `docs/adr-0003-supported-runtime-baseline.md` 和
> `docs/plugin-platform-m2.2a.md`。

- Electron 34.5.8 升级并精确锁定为 43.3.0；electron-vite 5.0.0、Vite 6.4.3、
  electron-builder 26.15.7 采用精确版本，better-sqlite3 保持 12.11.1。
- 主窗口启用 renderer sandbox，保留 context isolation、关闭 Node integration，并显式关闭
  subframe Node integration；sandboxed preload 由 electron-vite 完整打包。
- Electron 42+ 不再由依赖 postinstall 自动下载二进制，因此发布配置移除固定 `electronDist`；
  electron-builder clean package 按锁定版本获取分发包。
- better-sqlite3 已按 Electron ABI 148 重建；`smoke:native` 在 Electron 43.3.0 / Node
  24.18.1 下完成 WAL 写入、查询和清理。
- packaged smoke 额外验证 contextBridge 存在，并确认 renderer main world 中没有 Node
  `process`/`require`。
- M2.2A 不改变 manifest、SDK、数据库 schema 或用户数据。同 document `new Function` 插件
  renderer 风险仍存在，下一步 R2.2B 必须迁移为跨源 sandboxed iframe + MessagePort。

## 30. R2.2B 插件 renderer 跨源隔离（已完成）

> 2026-08-10。决策、renderer API v2、兼容路径与验收见
> `docs/adr-0004-plugin-renderer-isolation.md` 和 `docs/plugin-platform-m2.2b.md`。

- 删除宿主 `PluginHost` 的同 document `new Function` 路径；每次打开插件签发唯一
  `openbox-plugin://<token>.session` origin，在 sandboxed iframe 中运行 renderer。
- 自定义协议会话绑定主窗口 owner，index 单次消费、TTL 回收；请求 owner 由 Electron
  `webRequest` 注入的进程内 HMAC 证明，不信任 renderer 自报字段。资源读取拒绝穿越、symlink、
  超限和读取中变化的文件。
- 宿主与 frame 使用专用 MessagePort 和严格版本化 RPC；参数、结果、事件、JSON 预算、请求 ID
  与 64 个并发请求均验证。通知和主题写入在宿主 bridge 再检查权限。
- 宿主页 CSP 已移除 `unsafe-eval` 和 `plugin:` script/connect。v2 frame 只允许 self script、禁止
  connect；启动时断言 `electronAPI`/Node 全局不存在且父 document 不可达。
- 六插件均迁为 `rendererApiVersion: 2` 自包含 browser IIFE；UniEnv 的同步 `window.confirm`
  迁为受控异步 `api.confirm`。旧 manifest 缺版本时仍可在隔离 frame 内走 v1 兼容 loader。
- 验收：宿主 18 文件/136 项，GIF 35 项，UniEnv 116 项；六插件 typecheck/clean build/静态产物
  验证、根 lint/typecheck/production build、Windows unpacked package 和真实 v2 插件跨源 packaged
  冒烟全部通过。测试只使用临时 userData，不迁移或改写真实用户数据。
- backend 仍是可信 Node 代码；M2.3 下一步拆分 UniEnv 受信安装服务，并为普通 backend 建立
  故障隔离与纵深能力控制，不能把 Node permission model 描述为恶意代码安全边界。

## 31. R2.3 backend SDK v2 与 UniEnv 可信服务（已完成）

- backend 从 `child_process.fork` 迁为 Electron 43 `utilityProcess.fork`；删除进程内回退，worker 只继承
  SystemRoot/临时目录/区域/时区等最小环境，完整 PATH、HOME 与云凭证不再进入插件进程。
- 新增 backend RPC v2：随机 token、requestId、精确方法/字段、JSON 深度/节点/字节预算、64 并发上限；
  DB、日志、通知、对话框、网络、文件、快捷键、事件和可信服务均通过版本化方法分派。
- 六插件 manifest 均声明 `backendApiVersion: 2`；新版本为 Diary 0.4.7、Dice 0.1.3、GIF 0.3.4、
  ThemeManager 0.1.8、Turntable 0.1.6、UniEnv 0.5.0。旧 manifest 缺字段仍按 v1 SDK 语义兼容。
- UniEnv 插件包只含 `plugin.json + dist/main.js + dist/renderer.js`；进程/文件/下载/解压实现编入宿主
  trusted-service chunk。`trusted:unienv` 还必须匹配固定版本、文件集合和 SHA-256 摘要，否则激活失败。
- 插件模板已加入双 API v2 与独立 esbuild main CJS / renderer browser IIFE 构建。
- 验收：host 20 文件/147 项、GIF 35 项、UniEnv 116 项；全量 typecheck/lint/test/build、六插件产物、
  trusted digest、Electron 43 unpacked package、真实 utility RPC ping、UniEnv 五工具响应和 renderer 跨源
  隔离冒烟均通过。测试使用临时 userData，不修改用户数据。
- 安全措辞：utility process 不是恶意 Node 代码沙箱；普通 backend 仍只允许用户明确安装的可信代码。
  完整决策见 `docs/adr-0005-backend-runtime-and-trusted-services.md` 与 `docs/plugin-platform-m2.3.md`。

## 32. R2.4 可观测性、性能预算与供应链（已完成）

> 2026-08-10。决策与验收见 `docs/adr-0006-observability-and-supply-chain.md` 和
> `docs/plugin-platform-m2.4.md`。

- 宿主与六插件 npm audit 为 0；`adm-zip` 升 0.6.0，三份插件本地 esbuild 升 0.28.2，所有锁文件
  通过 clean install。六插件补丁版本为 Diary 0.4.8、Dice 0.1.4、GIF 0.3.5、ThemeManager 0.1.9、
  Turntable 0.1.7、UniEnv 0.5.1。
- 插件打包生成版本/API/ZIP/逐文件 SHA-256 清单；正式 `release` 强制使用仓库外 Ed25519 密钥签名
  和验签。根工程与六插件各自产出 CycloneDX SBOM。
- 新增结构化启动耗时、Electron 总 working set 与 plugin utility 数量；打包态冒烟执行宽松时间/内存
  预算。构建对 main、renderer、frame runtime 与六插件 renderer 执行独立体积门禁。
- 本地 JSONL 诊断日志限制为当前 2 MiB + 一个轮转备份；session marker 识别异常退出；renderer 崩溃
  每 60 秒至多自动恢复一次。
- CI check/build/audit 覆盖 Windows、Ubuntu、macOS；Windows 继续执行插件制品、SBOM、Electron ABI、
  unpacked package 和真实打包态冒烟。
- 验收：宿主 22 文件/153 项、GIF 35 项、UniEnv 116 项、供应链 Node 3 项；生产构建、体积预算、
  六插件清单、trusted digest、SBOM、Windows package/smoke 全绿。

## 33. R2.5 插件私有存储与最终交接（已完成）

> 2026-08-10。主工程 1.5.5；数据设计与兼容策略见 `docs/adr-0007-plugin-storage.md` 和
> `docs/plugin-platform-m2.5.md`。

- backend RPC v2 与 SDK 新增 `storage.get/set/delete/list`，宿主按插件 ID 绑定 namespace；键、JSON、
  负载和读写权限均验证。模板与六插件 `openbox-api.d.ts` 已同步。
- 数据库 schema v2 新增 `plugin_storage` 与迁移标记；所有 schema migration 使用
  `BEGIN IMMEDIATE`，失败回滚并保留旧 `user_version`。
- Diary `diary_entries` 与 Turntable `turntable_items` 迁移只复制、不删除；已有新值优先，marker 防止
  删除后旧数据复活。数据库升级时未安装的旧插件会在首次激活执行同一幂等事务。
- Diary 0.4.9、Turntable 0.1.8 已完全改用私有存储并移除 `database:*` 权限；旧原始 SQL API 只保留
  SDK v1 兼容。行为测试显式让 database API 调用失败，覆盖日记保存/导出和转盘 CRUD/排序/抽奖。
- 最终文档：`docs/architecture.md`、`docs/plugin-sdk-migration.md`、`docs/refactor-summary.md`、
  `docs/development.md`。
- 验收：宿主 24 文件/160 项、GIF 35 项、UniEnv 116 项、供应链 3 项；根与六插件 audit 0；
  check/build/性能预算/插件制品/摘要/SBOM/package 全绿。Windows packaged smoke 为 1,087 ms、
  451,412 KiB，使用临时 userData。

## 34. R2.6 数据库 fail-closed 与真实迁移冒烟（已完成）

> 2026-08-10。主工程 1.5.6；验收说明见 `docs/plugin-platform-m2.6.md`。

- `initDatabase` 不再吞掉引擎或 migration 失败；失败会回滚、关闭数据库、清空全局引用并抛错。
- 主进程在数据库失败时写 fatal diagnostic、显示错误并在 PluginManager/窗口创建前非零退出。
- packaged smoke 预置真实 schema v1 文件及 Diary/Turntable Unicode 旧数据，验证 `.bak-sqljs` 与原文件
  逐字节相同、better-sqlite3 迁移到 v2、namespace JSON/marker 正确且旧表仍存在。
- 单测另注入不兼容 schema，确认失败后 `getDatabase` 不可用、`user_version=1`、旧行保留、事务 DDL
  被回滚。
- 打包态迁移、renderer iframe、utility backend、UniEnv 可信服务和性能门禁同时通过：1,375 ms、
  452,628 KiB working set。

## 35. R2.7 宿主页面级加载（已完成）

> 2026-08-10。主工程 1.5.7；验收说明见 `docs/plugin-platform-m2.7.md`。

- 工作台、日志、设置、插件详情改为类型安全的 `React.lazy` 页面入口，保留 React 19、antd 5 与 zustand。
- Vite manifest 驱动体积分析；总 JS、静态入口、默认首页启动闭包分别设 3.4 MB、1.3 MB、2.3 MB 门禁。
- 静态入口 2,777,573→1,091,990 B，默认首页启动闭包降至 2,018,469 B；总 JS 为 2,790,229 B。
- 新增页面注册表 2 项测试；全量宿主基线为 25 文件/163 项。
- Windows packaged smoke 验证动态 chunk、旧库迁移、插件 iframe、两个 utility backend 与 UniEnv 可信服务：
  1,090 ms、447,532 KiB working set。

## 36. R2.8 三平台 unpacked 冒烟门禁（已落地，远端首次运行待确认）

> 2026-08-10。主工程 1.5.8；验收说明见 `docs/plugin-platform-m2.8.md`。

- packaged executable 解析覆盖 Windows/Linux/macOS 与 macOS 架构目录，3 项 Node 测试通过。
- packaged smoke 不再限制 Windows；三平台使用同一 v1 数据库备份/迁移、renderer iframe、utility backend
  与 UniEnv 可信服务断言。
- CI 三个矩阵系统均 package + launch；Linux 使用 Xvfb，macOS 禁止证书自动发现。
- Windows 本机 1.5.8 最终冒烟通过：1,003 ms、441,824 KiB。当前机器无 WSL/macOS，首次远端 Ubuntu/macOS
  运行仍是明确外部验收项，不冒充已通过。

## 37. R2.9 GIF Editor 后台残影分析（已完成）

> 2026-08-10。主工程 1.5.9、GIF Editor 0.3.6；验收说明见 `docs/plugin-platform-m2.9.md`。

- 残影检测和修复从插件 renderer 主线程迁到一次性 Blob Worker；Worker 源码在构建阶段独立 bundle 后嵌入
  `dist/renderer.js`，不增加发布文件或协议资源白名单。
- 文件在读取前执行 64 MiB 上限，client/worker 严格校验请求和结果；ArrayBuffer/RGBA 使用 transferable，
  完成、异常、切换文件、卸载或用户停止均 terminate Worker 并 revoke Blob URL。
- 修复复用检测得到的污染帧索引，只解码一次并跳过第二轮 O(n²) 分析；旧 `applyResidueFix(file)` 调用保持兼容。
- 验收：宿主 25 文件/163 项、GIF 5 文件/42 项、UniEnv 9 文件/116 项、供应链 6 项；全量 check/build、
  自包含 renderer、体积预算、确定性 ZIP/摘要/SBOM 通过。GIF renderer 318,504/360,000 bytes，制品 SHA-256
  `6e2c1e7d6a808099c423a3349d25f9fea2e7a1b3e666a2c77755be034155ffaf`。
- Windows 本机 1.5.9 packaged smoke 使用临时 userData 通过：1,081 ms、447,288 KiB；不修改真实用户数据。

## 38. R2.10 UniEnv 上游制品完整性（已完成）

> 2026-08-10。主工程 1.5.10、UniEnv 0.5.2；验收说明见 `docs/plugin-platform-m2.10.md`。

- 新增完整类型覆盖的制品目录：每个受支持工具版本固定 Windows x64 文件名、官方 HTTPS URL 与
  SHA-256；没有目录项的版本关闭失败。
- 下载器边写 `.part` 边计算 SHA-256，只在摘要匹配后 fsync 并原子提升；摘要错误、取消和网络失败均
  清理临时文件。镜像摘要不匹配会回退官方源，官方源仍不匹配则不会解压或执行。
- 修复 Temurin 17.0.12（build 7）与 21.0.5（build 11）的错误 release tag/文件名。
- 验收：宿主 25 文件/163 项、GIF 5 文件/42 项、UniEnv 10 文件/123 项、供应链 6 项；全量
  check/build、插件摘要、七份 SBOM、依赖审计与 Windows packaged smoke 通过。UniEnv 制品 SHA-256 为
  `ccddbc52f6caa042562ab4e93407c23d5602e438a806ef8ff966d383105ab10d`；未运行真实工具安装器或修改用户目录。
- Windows 本机 1.5.10 packaged smoke 使用临时 userData 通过：1,062 ms、447,968 KiB。

## 39. R2.11 UniEnv 版本生命周期提示（已完成）

> 2026-08-11。主工程 1.5.11、UniEnv 0.5.3；验收说明见 `docs/plugin-platform-m2.11.md`。

- 为 21 个固定版本建立完整类型覆盖的维护状态目录，区分 maintained branch、EOL 与 Git legacy，记录
  2026-08-10 官方依据日期和来源 URL。
- 修复 renderer 默认选择列表最老版本的问题；每个工具把目录内风险最低的兼容项置顶，但明确不称其为
  上游最新版本。
- 下拉框展示维护状态；单项安装显示版本专属风险并二次确认，组合安装逐项列出维护状态。取消发生在任务
  创建前，不改变 backend 协议或已安装数据。
- 验收：宿主 25 文件/163 项、GIF 5 文件/42 项、UniEnv 11 文件/128 项、供应链 6 项；全量
  check/build、插件摘要、七份 SBOM、依赖审计与 Windows packaged smoke 通过。UniEnv 制品 SHA-256 为
  `6e75e24f678e5d5b91aa630de09ae4d40977089f7ca37cbb810f8a29e155358c`；未运行真实安装器或修改用户目录。
- Windows 本机 1.5.11 packaged smoke 使用临时 userData 通过：1,377 ms、449,544 KiB。

## 40. R2.12 UniEnv 当前维护制品目录（已完成）

> 2026-08-11。主工程 1.5.12、UniEnv 0.5.4；验收说明见 `docs/plugin-platform-m2.12.md`。

- 新增 Node.js 24.18.1、Git for Windows 2.54.0、Go 1.26.5、Temurin 17.0.20/21.0.12/25.0.4
  的官方 Windows x64 文件名、release tag、URL 与 SHA-256，旧版本不删除。
- 生命周期增加 current 状态；Node 24、Git 2.54、Go 1.26 和 Temurin 21.0.12 成为目录首选。当前制品
  不再弹旧版二次警告，maintained/EOL/legacy 保持任务创建前确认。
- 内置组合迁到新制品；Python 仍保留 3.12.5 传统安装器并显示固定旧补丁风险。Python Install Manager
  因安装语义不同，明确留给独立里程碑。
- 验收：宿主 25 文件/163 项、GIF 5 文件/42 项、UniEnv 11 文件/131 项、供应链 6 项；生产构建与
  性能预算通过。UniEnv ZIP SHA-256 为
  `8cb2f38d68ce980c3a15647784fd722c1ed9c5184b88a67c8de2b8183e0c3784`。
- Windows 本机 1.5.12 packaged smoke 使用临时 userData 通过：1,441 ms、449,568 KiB；Electron
  43.3.0 / Node 24.18.1 / ABI 148 native smoke 通过。

## 41. R2.13 Python 当前稳定版过渡（已完成）

> 2026-08-11。主工程 1.5.13、UniEnv 0.5.5；验收说明见 `docs/plugin-platform-m2.13.md`。

- Python 3.14.7 官方 x64 安装器加入摘要目录并成为首选；Python 内置组合同步升级，旧版本继续保留。
- 官方 Install Manager 26.3 需要 Store/MSIX/WinGet 注册、会自动更新并占用全局别名。本阶段不在后台
  静默修改 AppX/系统状态，继续使用官方承诺在 3.14/3.15 提供的传统安装器作为兼容过渡。
- UniEnv 11 文件/131 项定向测试通过；可信服务摘要为
  `96a601fab8ea8f392397e140cf81ec098ec2f0c6d3bb8384842bd4fb38d9dc46`，ZIP SHA-256 为
  `8a9dd83de1364a25729b42de5bbf17a4582e04383a9b826382ac1caf2b8955f1`。
- Windows 本机 packaged smoke 通过：1,390 ms、449,796 KiB；Electron 43.3.0 / Node 24.18.1 /
  ABI 148 native smoke 通过。标准下载超时后使用已验证的本地同版本 Electron dist 打包，未改正式配置。

## 42. R2.14 M2A fuses 与宿主协议收口（已完成）

> 2026-08-11。决策与验证见 `docs/adr-0008-electron-fuses-and-app-protocol.md`。

- 锁定 `@electron/fuses` 2.1.3，afterPack 对 Electron 43 的 9 位 fuse wire 全量显式配置并回读；
  新增未知 fuse 会 fail-closed。
- 禁用 RunAsNode、NODE_OPTIONS、Node inspector 与 file protocol extra privileges；启用 cookie 加密、
  ASAR integrity、only-load-app-from-ASAR 和 WASM trap handlers。browser-specific V8 snapshot 因官方
  Windows 分发包缺少对应文件而显式关闭。
- 生产宿主页从 `file://` 迁至 `openbox-app://app/index.html`；资源协议只服务 renderer 根内的固定
  静态类型，IPC sender、导航和 legacy plugin CORS 同步绑定新 origin。
- 验收：宿主 27 文件/172 项、GIF 5 文件/42 项、UniEnv 11 文件/131 项、供应链 9 项全绿；完整
  check/build、Electron 43 package、fuse 回读及临时 userData packaged smoke 通过（2,033 ms、
  461,000 KiB）。

## 43. R2.15 M2B TypeScript 6 与 ESLint 10（已完成）

> 2026-08-11。决策与验证见 `docs/adr-0009-typescript-eslint-modernization.md`。

- 宿主及六插件统一锁定 TypeScript 6.0.3；移除废弃 `baseUrl`，backend emit 工程使用 Node16 解析，
  esbuild renderer 工程使用 bundler 解析。
- ESLint 升至 10.8.1 并迁移 `eslint.config.mjs`；旧 `eslint-plugin-react` 替换为支持 ESLint 10 的
  `@eslint-react/eslint-plugin`，保留严格但受控的 React correctness 基线。
- `lint` 与 `lint:fix` 明确分离，CI 检查不会修改工作树；共享文件 API 从 Node `Buffer` 收敛为跨运行时
  `Uint8Array`，模板和六插件声明同步。
- 六插件重建后没有运行产物 Git 差异；完整 check/build、Electron 43 ABI 148 原生冒烟、Windows unpacked
  package、fuse 回读和临时 userData packaged smoke 全绿（2,075 ms、464,072 KiB）。

## 44. R3 生命周期崩溃恢复收口（已完成）

> 2026-08-11。决策与验证见 `docs/adr-0010-plugin-crash-recovery.md`。

- 首窗创建后后台恢复插件的并发度默认限制为 2（测试可配置、最大 8），不再对任意数量插件直接
  `Promise.all`；已有 per-plugin single-flight 与维护租约保持不变。
- backend 运行期 RPC 超时现在会终止失去响应的 utility process，并作为异常退出进入恢复策略，不再只
  拒绝单次调用后遗留僵尸进程。
- 异常退出按 1s/5s/最多 30s 有界指数退避；5 分钟内 3 次崩溃即隔离并持久化为 disabled。用户显式
  重新启用会清除隔离与崩溃历史；停用、卸载、维护和退出会取消待执行恢复。
- 新增纯策略测试及生命周期集成测试，覆盖并发上限、自动恢复、崩溃循环隔离和运行期超时强杀。

## 45. R4/R5 Manifest v2 与权限差异确认（已完成）

> 2026-08-11。决策与验证见 `docs/adr-0011-manifest-v2-and-permission-diff.md`。

- manifest 严格契约新增 `manifestVersion: 1 | 2`；v2 强制 backend/renderer API 均为 2，未知版本关闭失败。
  缺字段或 v1 继续走明确标注的 Legacy Full Trust 兼容路径，不改写既有用户插件。
- 不可变安装 preview 现在携带 schema/API 模式、旧版本和精确权限增减；升级新增权限或 Legacy 包会使用
  原生 warning 确认框，用户在提交同一 stage token 前可看到差异。
- 模板与六生产插件迁到 Manifest v2：Diary 0.4.10、Dice 0.1.5、GIF 0.3.7、ThemeManager 0.1.10、
  Turntable 0.1.9、UniEnv 0.5.6；宿主为 1.5.14，UniEnv trusted digest 同步重钉。
- 确定性制品清单加入 manifestVersion，并继续由现有 Ed25519 release 签名/验签门禁覆盖；本地 unsigned
  sideload 仍明确属于 Full Trust，不伪装为已认证来源。
- 验收：宿主 28 文件/177 项、GIF 5 文件/42 项、UniEnv 11 文件/131 项、供应链 9 项全绿；UniEnv
  trusted digest=`8e14a88683239a5f97917ecadd251f101bb2b87819306151a1804b9777ddb5c5`；六插件确定性 ZIP/清单验证、
  Windows unpacked package、fuse 与临时 userData smoke 通过（1,489 ms、523,316 KiB）。正式下载超时后
  本次打包复用本机已验证 Electron 43.3.0 dist，未改正式配置。

## 46. R6 Theme v2 单一注册表与预览回滚（已完成）

> 2026-08-11。主工程 1.5.15、ThemeManager 0.1.11；决策见
> `docs/adr-0012-theme-v2-registry-and-preview.md`。

- 六套内置主题只保留在 `shared/themes/presets.ts`；隔离插件通过 renderer API v2 的 `theme.list` RPC 读取，
  ThemeManager 删除重复 `PRESETS`。
- ThemeManager 切换改为宿主代理的可逆预览：frame bridge 记录原主题并串行操作，保留后提交，撤销或 bridge
  销毁时恢复，即使插件 MessagePort 已关闭也不会遗留预览状态。
- 自定义/旧主题统一补齐语义 token，拒绝未知字段与 CSS 注入；旧 camel-case CSS 变量在迁移期与新 kebab-case
  变量同时输出。
- 新增无 Electron 能力的 `@openbox/ui` workspace，作为插件共享语义 CSS 变量与后续组件的稳定入口。
- 验收覆盖六主题唯一性、文字对比度、旧数据迁移、RPC 与预览竞态；宿主 29 文件/183 项、GIF 42 项、
  UniEnv 131 项、供应链 9 项全绿。完整 check/build、0 漏洞、六插件确定性制品、七份 SBOM、Electron 43
  ABI 148、fuse 与临时 userData packaged smoke 通过（1,951 ms、465,512 KiB）。ThemeManager ZIP SHA-256 为
  `bb75d77fe7119c224bebbf3832a3c5098542ee4f2baeed32f5c25a944c9b246a`。

## 47. R7A Dice renderer-only 与 Web Crypto（已完成）

> 2026-08-11。主工程 1.5.16、Dice 0.1.6；决策见
> `docs/adr-0013-renderer-only-plugins-and-crypto-rng.md`。

- Manifest v2 新增可选 `backend: false`；缺省保持既有 backend 行为。renderer-only 激活不创建 utility
  process，但继续参与启停、配置重启、退出清理和活跃状态查询。
- 为兼容现有安装元数据与 ZIP 文件集，`main` 仍是必需的占位入口；宿主只校验、不加载。向 renderer-only
  插件发送 backend 消息会得到明确错误。
- Dice 使用 `crypto.getRandomValues` 和 uint32 rejection sampling，替换 `Math.random`，避免模偏差；纯函数
  测试覆盖上下界、拒绝尾部、均匀周期和非法输入。
- 验收：宿主 29 文件/187 项、Dice 1 文件/5 项、GIF 42 项、UniEnv 131 项、供应链 9 项全绿；完整
  check/build、0 漏洞、确定性 ZIP/清单、七份 SBOM、Electron ABI 148 与 fuse 通过。Dice ZIP SHA-256 为
  `351c19c33cf9f0c53969eef1037e00f468ad5ee4a9dbf091cf705c07925f28da`；临时 userData packaged smoke 为
  1,228 ms、395,720 KiB，验证 Dice 不创建 utility process 且 UniEnv 可信服务正常。

## 48. R7B GIF Worker 与增量撤销（已完成）

> 2026-08-11。主工程 1.5.17、GIF Editor 0.3.8；决策见
> `docs/adr-0014-gif-worker-and-incremental-history.md`。

- 已有残影分析/修复继续使用可取消一次性 Blob Worker、transferable、资源预算和严格结果校验；直接 RGBA
  算法不引入会增加整帧复制的 OffscreenCanvas，等待能真实替换 CPU pass 的绘制/缩放场景。
- 同尺寸、同帧 ID 的编辑历史改为可逆 XOR 字节区间与 delay 差异，只复制变化帧；结构变化继续使用受
  256 MiB/50 项预算约束的深快照，无操作副本不进入历史。
- 验收：宿主 187、GIF 44、Dice 5、UniEnv 131、供应链 9 项测试全绿；六插件 clean build、确定性
  ZIP/摘要、七份 SBOM、Electron 43 ABI 148、fuse 与 packaged smoke 全部通过。GIF 0.3.8 ZIP
  SHA-256=`eecdc203e000f25332d0711b43a91cc9750ac25df3cd5e384fc15f2ce6ae53dd`；最终 packaged
  初始化 1414 ms、working set 409540 KiB，并通过旧数据事务迁移/字节备份断言。

## 49. R7C/R7D ThemeManager 覆盖确认与 Diary 事务草稿（已完成）

> 2026-08-11。主工程 1.5.18、Diary 0.4.11；数据决策见
> `docs/adr-0015-diary-transactional-drafts.md`。

- R7C 不重复实施：R6 已交付宿主单一主题注册表、`theme.list`、ThemeManager preview/commit/rollback、卸载
  自动回滚和旧主题迁移；ThemeManager 源码无重复 `PRESETS`。
- SDK/backend RPC v2 新增 `storage.batch`：1-64 个严格 JSON set/delete，全部预校验后在宿主
  `BEGIN IMMEDIATE` 中提交；namespace 仍由宿主绑定，模板与六生产插件声明同步。
- Diary 正文和草稿分别使用 `entry:<date>` / `draft:<date>`；显式保存/删除原子清理草稿，自动草稿按后端
  队列串行，打开时恢复。保存 Result 为判别联合，失败或编辑 revision 已变化时保持 dirty 且禁止离开。
- 日期按 `YYYY-MM-DD` 数字段解析，不再把 date-only key 当 UTC instant 转成本地日期。旧全局表迁移保持
  原样，新增 draft key 对旧版本可忽略，不改写用户数据。
- 验收：宿主 188、Diary 4、Dice 5、GIF 44、UniEnv 131、供应链 9 项测试全绿；0 漏洞、六插件 clean
  build/确定性 ZIP、七份 SBOM、ABI 148、fuse 与 packaged smoke 通过。Diary 0.4.11 ZIP SHA-256=
  `545c2cd31ef672193db303e28ebdbb94578f2ae7f2b88e4567cb3268b877d14b`；packaged 初始化 1323 ms、
  working set 406968 KiB，并通过旧数据事务迁移/字节备份断言。

## 50. R7E Turntable 安全 RNG、几何与原子重排（已完成）

> 2026-08-11。主工程 1.5.19、Turntable 0.1.10；决策见
> `docs/adr-0016-turntable-rng-and-geometry.md`。

- backend winner 与 renderer 动画均改用 Web Crypto；加权选择采用 `[0,1)` 样本和半开累计区间，拒绝
  空列表、非有限/非正权重，源码不再包含 `Math.random`。
- 转盘扇区与指针统一以顶部 `-pi/2` 为契约；纯函数计算最小正向补角再增加整圈，属性测试覆盖不等权重、
  全部 winner 和任意历史 rotation，最终扇区中心始终落在顶部指针。
- backend mutation queue 串行化 add/update/delete/reorder 的 read-modify-write，完整 items 数组通过
  `storage.batch` 原子提交；重排必须包含且仅包含全部现有 ID，并对数量、ID、label、weight、color 做边界校验。
- `items` key/JSON shape 不变，旧表兼容迁移不变；延迟存储并发测试证明三个同时 add 不丢失/不复用 ID，
  deactivate/activate 后顺序和 sort_order 保持。
- 验收：宿主 188、Diary 4、Dice 5、GIF 44、Turntable 5、UniEnv 131、供应链 9 项全绿；0 漏洞、六插件
  clean build/确定性 ZIP、七份 SBOM、ABI 148、fuse 与 packaged smoke 通过。Turntable 0.1.10 ZIP
  SHA-256=`c70b0ab99c1d9653f60b160a69f58d6195fbf4aa6707193c3810dde266487c5a`；packaged 初始化
  1457 ms、working set 406820 KiB，并通过旧数据事务迁移/字节备份断言。

## 51. R7F UniEnv 中断安装恢复（已完成）

> 2026-08-11。主工程 1.5.20、UniEnv 0.5.7；决策见
> `docs/adr-0017-unienv-interrupted-install-recovery.md`。

- `trusted:unienv` 保持为兼容的 manifest wire 名称，并明确映射为只允许宿主固定摘要第一方实现调用的
  `environment.manage` 能力；普通 backend 不获得进程、下载、解压或环境修改能力。
- 可信服务启动时从规范化安装根和固定工具/版本目录推导扫描范围，只清理直属、非 symlink 的
  `.unienv-staging-*` 中断安装目录，不触碰已安装 runtime。
- 恢复或配置检查失败会阻断安装、组合安装、卸载和版本切换；任务不伪恢复，重新执行时仍重新下载并校验
  固定 SHA-256 后通过同卷 staging 原子提升。
- UniEnv 11 文件/132 项测试通过；可信服务摘要为
  `1b6b94c57174e075faf41c383f2e544237e1e44f206e69b40184eeb8c31f1b17`，插件 ZIP SHA-256 为
  `ebc3c113d3ddb05728ca913d8256d13242121dfa22a22749da0ccd72ef6f8963`。
- 完整 check/build、0 漏洞、六插件确定性制品、七份 SBOM、Electron ABI 148、fuse 与临时 userData
  packaged smoke 全绿；初始化 1477 ms、working set 410260 KiB，并通过旧数据事务迁移/字节备份断言。

## 52. R8 Ant Design 6 与 semantic DOM（已完成）

> 2026-08-11。主工程 1.5.21；决策见 `docs/adr-0018-ant-design-6-semantic-styling.md`。

- 按官方升级路径先验证 Ant Design 5.29.3，再精确锁定 Ant Design 6.6.0 与 `@ant-design/icons` 6.3.2；
  React 19、zustand、Theme v2 和现有组件选型不变。
- ThemeProvider 用 v6 semantic `classNames` 为 layout/card/table/modal/input/select/alert/tag/statistic 注入稳定
  `ob-*` 类；全局主题 CSS 零 `.ant-*` selector，两套赛博主题不再依赖内部 DOM 结构。
- Alert/Spin/Space/Descriptions 弃用 API 已迁移；Ant Design App context 取代静态 message。插件卡入口使用
  独立原生 button，命令面板补 dialog/listbox/active-option 语义。
- 新增静态门禁锁定依赖 major、拒绝内部 selector/旧 props，并验证六主题 × 六生产插件的完整 token/frame
  契约矩阵。宿主 30 文件/193 项、全部插件与供应链测试全绿。
- 生产总 renderer 2,732,147 bytes，入口 1,277,762 bytes，启动闭包 1,979,873 bytes，均在原预算内；
  0 漏洞、确定性插件制品、七份 SBOM、ABI 148、fuse 与 packaged smoke 通过。最终 packaged 初始化
  1512 ms、working set 423716 KiB，并通过隔离 frame 与旧数据迁移断言。

## 53. M9 Windows 发布与自动更新闭环（已完成；远端首发待仓库接入）

> 2026-08-11。主工程 1.5.23；决策见 `docs/adr-0019-trusted-cross-platform-release.md`，操作手册见
> `docs/trusted-release.md`。

- 支持范围收敛为 Windows 10/11 x64；CI、NSIS 打包和 compatibility workflow 均只运行 Windows，已删除
  macOS/Linux targets、entitlements 和 runners。Windows ARM64 同样不在当前支持范围。
- Windows 安装器明确不使用 Authenticode 证书；设置页和发布文档提示 Unknown publisher/SmartScreen。
  正式工作流不读取任何 Windows 证书 secret，但继续校验 Electron fuses、插件 Ed25519 签名，并生成七份
  CycloneDX SBOM、`SHA256SUMS` 与 GitHub artifact attestation。
- `electron-updater` 采用 stable/beta 显式状态机：仅正式包启用；自动检查但不自动下载/安装；只有用户从
  beta 切回 stable 时允许一次降级检查，任意同通道降级继续禁止。公开 GitHub Release 分别发布
  `latest.yml`/`beta.yml`、NSIS installer 与 blockmap，客户端不内置 GitHub token。
- 新增 Windows 更新制品校验器，在发布前核对 channel 元数据版本、installer basename、blockmap 与实际
  installer SHA-512；不一致即 fail-closed，不更新 Release。
- 新安装和升级在 preview/direct 两条边界均拒绝 Manifest v1；现有 v1 插件仍可读取、激活和迁移，绝不
  删除插件目录、配置或用户数据。
- Windows `Release Compatibility` 工作流从两个真实 tag 分别构建 unpacked 应用，并在同一临时 userData
  上执行 old -> candidate -> old，断言 `PRAGMA user_version` 与持久化哨兵在升级和回滚后均保留。
- 本地验收：宿主 32 文件/208 项、六插件 190 项、供应链 15 项通过（另 1 项 Windows 无开发者符号链接
  权限时按预期跳过）；生产 build/性能预算、正式 Windows NSIS、`latest.yml`/`beta.yml`、blockmap、元数据
  SHA-512、Electron ABI 148、fuses 与 packaged smoke 全绿。1.5.23 packaged 初始化为 1237 ms / 427988 KiB。
- 1.5.23 NSIS 已在含中文和空格的随机临时目录完成静默安装、真实 packaged 启动和静默卸载；该流程固化为
  `npm run smoke:installer` 并加入 tag 发布门禁。
- 剩余外部输入仅为公开 GitHub 仓库、推送权限、首次插件 Ed25519 secrets/variable，以及两个真实 tag 的
  在线更新演练。本地仓库当前没有 remote，机器未安装 `gh`。

## 54. M9.1 GitHub 自动更新可选化（已完成）

> 2026-08-12。GitHub 接入交接见 `docs/github-auto-update-handoff.md`。

- 普通 `electron-builder.yml` 不包含 publish provider；`npm run package` 是无需仓库、GitHub Token、插件发布密钥或
  Windows 证书的默认本地成品路径。
- 宿主仅在 packaged build 且 `resources/app-update.yml` 存在时启动 `electron-updater`。未配置的正式安装包返回
  `disabled` 更新状态，不注册 updater 事件、不启动延迟网络检查；插件、主题、数据库和全部离线功能保持可用。
- GitHub Release 配置和 stable/beta 工作流继续保留在 `electron-builder.release.yml` 与 `.github/workflows/`，由后续
  仓库所有者按交接文档启用，无需修改应用业务代码。
- 验收：宿主 32 文件/210 项、六插件 190 项、供应链 15 项通过（1 项按权限条件跳过）；生产 build/性能预算通过；
  repository-free NSIS 明确不含 `app-update.yml`，Electron ABI 148、packaged smoke、真实静默安装/启动/卸载全绿。

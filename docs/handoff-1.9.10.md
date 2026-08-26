# CrucibleBox 工具箱修复交接（1.9.10）

> 本文档是为接手修复的工程师准备的交接提示词。**上一轮交接存在严重问题：所有修复均"测试通过/产物验证通过"，但用户卸载重装 1.9.10 后全部 bug 仍在。** 请严格遵守文末"验证纪律"。

## 0. 项目概况

**仓库**：`E:\CrucibleBox_Sourses`（GitHub: `Xuancheng75/CrucibleBox`，main 分支）

**技术栈**：
- 前端：Tauri 2 + React 19 + antd 5.24 + zustand + dnd-kit，位于 `tauri-frontend/`
- 后端：Rust（rusqlite、tauri 2.11.x），位于 `src-tauri/`
- 插件 backend：Rust sidecar（`cruciblebox-plugin-host`，quickjs 沙箱）
- 插件 renderer：共享运行时 `src/plugin-runtime/frame-entry.ts`（构建成 `out/plugin-frame/runtime.js`，Electron 与 Tauri 共用）
- 插件本体：`plugins/`（diary、dice-roller、gif-editor、theme-manager、turntable、unienv）

**发布链**：tag `tauri-vX.Y.Z` → `.github/workflows/tauri-release.yml` → NSIS 安装器 + `.sig` + `latest.json`（updater 滚动清单，端点在固定 tag `tauri-latest/latest.json`）。当前版本 1.9.10（Latest）。

## 1. 先看哪些文件（按优先级）

**总览（必读，建立全貌）**
- `src-tauri/tauri.conf.json` —— 版本、updater endpoint、bundle/nsis 图标、CSP
- `.github/workflows/tauri-release.yml` —— 发布链（含 1.9.9 加的图标资源清理、tauri-latest 同步）
- `docs/plugin-sdk.md` + `docs/plugin-dev-guide.md` —— 插件契约与开发规范
- `docs/electron-legacy-registry.md` —— 明确哪些 Electron 代码已冻结

**Bug A 图标（先看）**
- `src-tauri/tauri.conf.json`（`bundle.icon`、`bundle.windows.nsis.installerIcon`）
- `src-tauri/icons/icon.ico`（当前 82978B，应与 `C:\Users\hjc\Desktop\openbox-icon\openbox-neon.ico` 一致）
- `.github/workflows/tauri-release.yml`（"Force rebuild of app icon resources" 步骤）
- 验证用产物：`src-tauri/target/release/cruciblebox.exe`、`src-tauri/target/release/bundle/nsis/CrucibleBox_1.9.10_x64-setup.exe`

**Bug B UniEnv（先看）**
- `src-tauri/src/unienv_service.rs` —— 阶段 A 现状（只有只读方法，install 返回 not-supported）
- `src-tauri/src/envelope_host.rs`（`trusted.invoke` 分发）+ `src-tauri/src/permissions.rs`（host 方法白名单）
- `src-tauri/cruciblebox-plugin-host/src/envelope.rs`（HOST_METHODS）+ `ctx.rs`（`__buildCtx` 的 `invokeTrustedService`）
- `plugins/unienv/src/main.ts` + `renderer.tsx`（插件侧调用/轮询协议）
- `plugin-system/trusted-services/unienv/` —— **Electron 完整 TS 实现（冻结），是 Rust 移植的唯一参照**
- `src-tauri/Cargo.toml`（当前无 HTTP 客户端库）

**Bug C 日记 INTERNAL_ERROR（先看）**
- `src-tauri/src/commands.rs`（`plugin_send_message`、`create_renderer_session`）
- `tauri-frontend/src/components/PluginHost.tsx`（bridge `sendToBackend`、session 生命周期 useEffect）
- `src/plugin-runtime/PluginFrameBridge.ts`（`toRpcError`，1.9.9 已透出真实 message）+ `frame-entry.ts`
- `src-tauri/src/plugin_protocol.rs` + `plugin_session.rs`（session registry）
- `src-tauri/src/backend_process.rs`（sidecar 管理 + 我加的 `e2e_diary_backend_initialize_and_message` 测试，backend 链路证明是通的）
- `plugins/diary/src/utils/db.ts` + `renderer.tsx`（renderer 请求）

**Bug D Modal 白条（先看）**
- `tauri-frontend/src/theme/antd.ts`（`components.Modal.headerBg`，1.9.9 加的，**未验证是否被 antd 5.24 消费**）
- `tauri-frontend/src/styles/cyber.css` + `neon.css`（`ob-modal-surface` 背景是 `colorBgContainer`）
- `tauri-frontend/src/components/PluginImport.tsx`（Modal 用法）

**Bug E 深色白屏（先看）**
- `src/plugin-runtime/frame-entry.ts`（1.9.9 把 colorScheme 硬编码 light 改 dark + 深色兜底，**需确认是否真正进 runtime.js**）
- `tauri-frontend/src/components/PluginHost.tsx`（iframe 加载）
- `scripts/build-plugin-frame-runtime.mjs` + `out/plugin-frame/runtime.js`（构建产物；改 frame-entry 必须重建）
- `.github/workflows/tauri-release.yml`（`npm run build:frame` 步骤是否在 CI 执行）

## 2. 未解决 Bug 详情

### Bug A：任务栏/桌面快捷方式图标仍旧图标

**现象**：应用内 monogram 是新图标，但 Windows 任务栏、桌面/开始菜单快捷方式仍旧。

**已尝试但未验证真机**：
1. 1.9.10 设 `bundle.windows.nsis.installerIcon/uninstallerIcon` → `icons/icon.ico`
2. 1.9.9 CI workflow 构建前 `rm -rf target/release/build/cruciblebox-*`（强制重建图标资源）

**验证结论**：本地 + CI 产物的 **setup.exe 安装器图标**已确认与新源图标像素一致。**但**用户看的是安装后的**应用 exe（CrucibleBox.exe）图标 / 快捷方式图标**，二者可能来自不同机制：
- 应用 exe 图标 = `tauri-codegen` 的 `default_window_icon`，从 `bundle.icon` 选 `.ico`（`icons/icon.ico`）编译进 exe。**本地 release exe 图标已确认是新的**，但 **CI 产物未直接验证**（清理步骤可能没覆盖 tauri-codegen 的缓存路径）。
- 快捷方式图标：NSIS 安装器创建的 `.lnk` 指向 exe，Windows 可能**缓存旧图标**（iconcache），需重启资源管理器。

**排查方向**：
1. 下载 1.9.10 的 `CrucibleBox_1.9.10_x64-setup.exe`，用 7-Zip/NSIS 解包，提取内嵌的应用 exe，检查其 `default_window_icon`（用 `ExtractIconEx` + 像素对比源 `openbox-neon-32.png`）。
2. 若应用 exe 图标仍旧 → tauri-codegen 缓存未清（需 `cargo clean -p cruciblebox` 或删除 `src-tauri/target/release/build` 整个目录）。
3. 若应用 exe 图标已新但用户仍旧 → 属 Windows 图标缓存，让用户重启资源管理器/注销；或调整 NSIS 安装器强制刷新。
4. 检查 `src-tauri/binaries/`：本地 stage 的是 `cruciblebox-plugin-host.exe`（无 triple 后缀），CI stage 的是 `cruciblebox-plugin-host-x86_64-pc-windows-msvc.exe`（带 triple）。确认 Tauri externalBin 打包用哪个。

### Bug B：UniEnv 插件无法下载/安装

**现象**：打开 UniEnv 只显示框架（"工具与组合包"几个字），无法真正下载/安装工具。

**已尝试（1.9.9）**：Rust 实现 `trusted.invoke` host 方法（`src-tauri/src/unienv_service.rs`，阶段 A：listTools/listVersions/listCombos/detect）。**但 install/uninstall/switchVersion/installCombo 明确返回 "not-supported-yet"**——下载/安装功能**完全未实现**。

**根因**：UniEnv 的下载/安装依赖 `api.invokeTrustedService('unienv','message',...)`。Electron 侧的完整 TS 实现在 `plugin-system/trusted-services/unienv/`（已冻结，含 5 个工具：python/node/git/go/java 的下载 URL+SHA-256、解压、junction 版本切换、进程运行、组合包）。**Tauri 侧只有只读元数据，无实际下载/安装**。

**方向**（这是最大工作量）：
1. 继续在 `unienv_service.rs` 实现阶段 B：`downloadWithProgress`（HTTP + SHA-256 校验 + 进度）、`extractZip`（PowerShell Expand-Archive 或 zip crate）、`createJunction`（版本切换）、工具各自的下载 URL/SHA（从 `artifact-integrity.ts` 移植）。
2. 需在 Cargo.toml 加 HTTP 客户端（reqwest 或 ureq）——注意项目目前无网络库。
3. **进度回传**：UniEnv renderer 用 `pollTask` 轮询 taskId（`getTask`）。目前 taskId 未实现。需在 Rust 侧维护任务状态表 + `getTask` 返回快照。
4. renderer 端 `renderer.tsx` 的 `installTool` 走 `send({type:'install',...})` → `readStartedTaskId` → `monitorTask`。后端返回 `{success, taskId, message}` 才符合契约。

### Bug C：日记插件打开记录时 INTERNAL_ERROR: Plugin renderer request failed

**现象**：打开日记插件，点击某天记录时前端报 `INTERNAL_ERROR: Plugin renderer request failed`。

**已尝试**：
1. 新增 `e2e_diary_backend_initialize_and_message` 测试（backend_process.rs）——证明 sidecar backend 的 initialize + getMonthEntries（storage 链路）**通过**。
2. `PluginFrameBridge.ts` 的 `toRpcError` 改为对 string 错误透出真实 message（1.9.9）。

**关键**：backend 链路 OK，但**前端 renderer → bridge → `plugin_send_message` 命令**这一段有问题。**根因未定位**。

**排查方向**：
1. 前端 `PluginHost.tsx:126`：`sendToBackend: (message) => tauriApi.plugin.sendMessage(sessionTokenRef.current ?? session.token, message)`。传的是 **session token**（64 hex）。
2. 后端 `commands.rs plugin_send_message`：若 id 是 64 hex，用 `registry.get(id, "main")` 反查 plugin_id。**可能失败点**：
   - session 是否已存在（PluginHost useEffect 清理时 dispose 旧 session，若 effect 重跑会导致 token 失效）
   - **重点**：`createRendererSession` 后 session 状态是 "issued"，index 加载时 `consume_index` → "active"。若用户打开日记时 iframe 未成功加载 index，session 未消费，`registry.get` 仍返回 ok（get 不要求 active）。需验证。
   - `plugin_backend_record(&plugin_id)` 依赖 DB 中 diary 的 enabled 状态。
3. **最可能根因候选**：`PluginHost.tsx` useEffect 依赖 `[pluginId, pluginName, rendererEntry, permissionKey]`，每次这些变化会 dispose 旧 session 并重建。若打开日记记录时组件重挂载，旧 session token 失效，而 bridge 还在用旧 token 发请求 → `registry.get` 返回 NotFound → `plugin_backend_record("旧token")` 失败 → 前端 INTERNAL_ERROR。
4. 建议：让 `plugin_send_message` 在反查失败时**回退到用原始 plugin_id**（而不是报错），或前端始终传 pluginId 而非 session token；或前端确保 session 生命周期稳定。
5. **务必真机复现**：用 `cargo tauri dev` 打开日记 → 点击记录 → 抓前端 console（现在能看到真实 message 了）+ sidecar stderr。

### Bug D：科幻面板 / 零号城区主题下，Modal 标题背景有白色/浅色长条

**现象**：切到科幻面板（cyber）或零号城区（neon-district）主题，打开导入插件或插件配置弹窗，标题栏出现与主题不符的浅色长条。

**已尝试（1.9.9）**：`tauri-frontend/src/theme/antd.ts` 加 `components.Modal.headerBg: tokens.colorBgElevated`。**未验证真机，且可能没生效**。

**方向**：
1. antd 5.24 Modal header 背景 = `token.headerBg`（默认 `colorBgElevated`）。
2. `ob-modal-surface`（cyber.css:121 / neon.css:228）设 `background: var(--ob-color-bg-container)`（#0d121a），而 `headerBg` 默认 `colorBgElevated`（#131a24）——**两者色差**形成可见长条。
3. 正确修法：让 header 与 content 同色，或把 headerBg 设为 `colorBgContainer`。需确认 `components.Modal.headerBg` 是否被 antd 5.24 正确消费（可能是 `theme.components.Modal.headerBg` 或需 `ConfigProvider theme={{components:{Modal:{headerBg}}}}` 结构）。
4. **注意**：主题改动（1.9.6 曾把 cyber 网格从 body 移到 main-content、标题区加背景）在 1.9.8 被**回退**了（恢复 1.9.5 原样）。当前 cyber.css 是 1.9.5 版本（body 有 48px 网格背景）。确认这不是 1.9.6 遗留。

### Bug E：深色主题加载插件时短暂白屏

**现象**：使用深色主题，打开插件时 iframe 先闪白再变深色。

**已尝试（1.9.9）**：`src/plugin-runtime/frame-entry.ts` 把 `document.documentElement.style.colorScheme = 'light'` 改为 `'dark'` + body 深色兜底背景。**未验证**——且这是**共享运行时**（Electron+Tauri 共用），改动需重建 `out/plugin-frame/runtime.js`（`npm run build:frame`）才生效，**发布链是否重建了 runtime.js 未确认**。

**方向**：
1. 确认 `out/plugin-frame/runtime.js` 是否由 CI 每次重建（`npm run build:frame` 在 release workflow 有执行）。
2. frame-entry.ts:102 的改动要真正进入 runtime.js 并被 Tauri 的 iframe 加载。验证路径：`createRendererSession` 的 `runtime_path` 指向 `out/plugin-frame/runtime.js`。
3. 白屏根因可能是：iframe 加载时主题数据未到（bridge 握手前），body 默认白底。需在 frame-entry 初始即设深色背景（用 `--ob-color-bg` 变量，但变量由宿主注入，iframe 初始可能读不到）。

### Bug F：插件开发规范文档

**已做（1.9.9）**：新建 `docs/plugin-dev-guide.md`（实操指南）。契约基准 `docs/plugin-sdk.md` 已存在。此项应已完成，可向用户确认是否满足。

## 3. 关键架构说明（必须知道）

1. **Electron 线冻结**：`src/`（Electron renderer）、`electron/`、`plugin-system/`、`plugins/*/src` 在 1.9.2 冻结。**但 `src/plugin-runtime/frame-entry.ts` 和 `PluginFrameBridge.ts` 是共享运行时，Tauri 和 Electron 都用，可改**（已确认 Tauri 前端引用它们）。
2. **插件 backend 架构**：插件 main.js 在 Rust sidecar（quickjs）中运行，通过 `__hostRequest(method, paramsJson)` 同步往返调用宿主 host 方法（`envelope_host.rs`）。host 方法白名单在 sidecar `envelope.rs HOST_METHODS` 和宿主 `permissions.rs is_host_method_implemented` 两侧维护。**两侧必须同步**。
3. **renderer iframe 加载链**：`create_renderer_session` → `index_url = http://cruciblebox-plugin.localhost/<token>/index.html`（WebView2 自定义协议 workaround）→ `generated_index` 生成 HTML（脚本用**相对路径** `runtime.js`/`renderer.js`）→ runtime.js 内 `frame-entry` 通过 MessagePort 握手 → `PluginFrameBridge`（宿主侧）→ `plugin_send_message` 命令 → sidecar。**CSP frame-src 必须允许 `http://cruciblebox-plugin.localhost`**（已在 1.9.6 修复）。
4. **updater 滚动清单**：endpoint 固定 `https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-latest/latest.json`，每次发布 workflow 用 `gh release upload tauri-latest ... --clobber` 覆盖。任何版本都能发现最新版。
5. **图标嵌入**：应用 exe 窗口图标由 tauri-codegen（`generate_context!`）从 `bundle.icon` 首个 `.ico` 编译；tauri-build 用 `tauri_winres` 嵌 PE 资源。**两者都可能被 cargo 缓存**。NSIS 安装器图标由 `nsis.installerIcon` 配置。

## 4. 构建/测试命令（CI 门禁）

```bash
cd src-tauri && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd tauri-frontend && npx tsc --noEmit && npm run build
npx eslint "src/**/*.{ts,tsx}" "tauri-frontend/src/**/*.{ts,tsx}" --max-warnings=0   # 仓库根
```

**CI 用 Rust 1.97（clippy 更严），本地 1.92 可能漏报**（例：`as u8` 冗余强转，1.9.3 曾因此在 CI 失败）。

## 5. 验证纪律（务必遵守，防重蹈覆辙）

- **每个修复必须真机验证**（`cargo tauri dev` 或安装 release），不能只靠单元测试/像素对比。
- 单元测试通过 ≠ 问题解决。Bug C/D/E 尤其需要实际运行确认。
- 发布前用 `gh run watch` 确认 CI 双线（Windows CI + Tauri Release）绿，且 `tauri-latest/latest.json` version 正确。
- 改共享运行时（frame-entry/PluginFrameBridge）后必须重建 `out/plugin-frame/runtime.js` 并确认 CI 包含。

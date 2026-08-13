# CrucibleBox

一个基于 Electron 的可扩展工具箱桌面应用，支持插件系统、主题系统与全局快捷键。

## 功能特性

- **插件系统**：通过插件包（`.zip`）或插件目录导入，backend 运行于独立 Electron utility process，安装时确认
- **权限模型**：每位插件声明所需权限（网络、文件、数据库、剪贴板、通知、快捷键等），由 `PermissionGuard` 门控；插件为可信代码，权限声明用于功能控制
- **命令系统**：插件可注册全局命令，通过 `Cmd/Ctrl+Shift+P` 唤起
- **全局快捷键**：插件可注册系统级快捷键（如 `Cmd/Ctrl+I` 唤起插件导入）
- **主题系统**：内置亮色（默认）/深色/清新绿/海洋蓝/科幻面板/零号城区六套预设，运行时切换并下发 CSS 变量，插件可实时感知主题变化
- **插件排序**：长按卡片约半秒拖动排序，或使用卡片操作区的上移/下移按钮（键盘可操作）调整顺序；顺序持久化于数据库 schema v3 `plugins.sort_order`，插件激活顺序跟随列表
- **插件日志**：日志入库并支持按插件、级别筛选与实时刷新
- **自定义协议**：`plugin://` 协议安全地服务插件静态资源（内置路径穿越防护）
- **配置中心**：`settings` 表持久化应用配置

## 技术栈

- Electron + electron-vite + TypeScript
- React 19 + Ant Design 6 + zustand
- SQLite（sql.js + better-sqlite3 驱动抽象）
- Vitest 单元测试 / ESLint / Prettier

## 快速开始

```bash
npm install
npm run dev          # 开发模式
npm run typecheck    # 类型检查
npm test             # 单元测试
npm run lint         # ESLint 零警告检查
npm run build        # 生产构建
npm run package:plugins # 生成插件 ZIP 与逐文件哈希清单
npm run generate:sbom   # 生成宿主与六插件 CycloneDX SBOM
npm run package      # 打包安装程序（electron-builder）
```

## 插件开发

插件是一个包含 `plugin.json` 清单的目录或压缩包，结构示例：

```jsonc
{
  "name": "demo",
  "version": "1.0.0",
  "displayName": "示例插件",
  "description": "一个示例插件",
  "author": "openbox",
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

- `main`：backend 插件入口（CommonJS，导出插件生命周期钩子），由 utility process 加载
- **信任模型**：Node backend 是**可信代码**；utility process、最小环境和严格 RPC 提供故障隔离与纵深控制，但权限声明不是恶意代码安全边界。安装插件时会弹出确认框展示插件名/版本/作者，由你确认其可信后才写入。
- 新插件使用 `ctx.storage` 与 `storage:read/write`；`database:read/write` 只用于旧 SDK 兼容。
- `file:read/write` 表示整个文件系统能力，请谨慎授予。
- 再次导入更高版本的同一插件即可完成更新（保留其 ID、配置与启用状态）

## 目录结构

```
electron/      # 主进程、IPC、菜单、主题服务、preload
src/           # React 渲染层组件、页面、store、hooks、api 封装
plugin-system/ # 插件沙箱、权限防护、协议、生命周期管理
database/      # SQLite 数据库与仓储层
shared/        # 跨进程共享的 types / themes 纯函数
tests/         # Vitest 单元测试
```

## 插件 backend v2

生产插件声明 `backendApiVersion: 2`，通过带随机会话 token、请求 ID、精确方法契约和负载预算的 RPC 与宿主通信。UniEnv 的进程、文件、下载和解压实现属于宿主固定摘要可信服务，发布插件只包含受限代理。架构、兼容规则和构建方式见 `docs/security-model.md` 与 `docs/plugin-sdk.md`。

## 发布与诊断

插件发布会生成确定性 ZIP、逐文件 SHA-256 清单，并支持仓库外 Ed25519 密钥的强制签名验签；
宿主和六插件可生成 CycloneDX SBOM。运行时会在 `userData/logs` 写有界 JSONL 诊断日志，并输出
可机器读取的启动耗时/working-set 指标。完整发布环境变量和验收步骤见
`docs/release-runbook.md`。

## 插件渲染隔离

生产插件使用 `rendererApiVersion: 2`。每次打开插件时，宿主签发唯一 `openbox-plugin://<token>.session` origin，在 sandboxed iframe 中加载自包含 browser renderer，并通过受校验的 MessagePort RPC 提供配置、主题、通知和 backend 消息能力。插件 frame 不包含 Electron preload，不能访问宿主 DOM、`window.electronAPI`、Node `process` 或 `require`。契约与构建说明见 `docs/plugin-sdk.md` 与 `docs/security-model.md`。

## 架构与文档

- 当前模块、进程、数据流和信任边界：`docs/architecture.md`
- 安全模型与信任边界：`docs/security-model.md`
- 插件 SDK v2 契约与私有存储：`docs/plugin-sdk.md`
- 安装事务与崩溃恢复：`docs/install-recovery.md`
- 发布与自动更新 runbook：`docs/release-runbook.md`
- 构建、测试、发布与打包冒烟：`docs/development.md`
- 2026 重构结果、技术选型与剩余边界：`docs/refactor-summary.md`

## Windows releases and automatic updates

GitHub publishing is optional. `npm run package` produces a standalone Windows x64 installer without repository
credentials; if no `app-update.yml` is packaged, the application disables only online updating and keeps all other
features available. Repository owners can later enable GitHub Releases by following
[`docs/github-auto-update-handoff.md`](docs/github-auto-update-handoff.md).

CrucibleBox supports Windows 10/11 x64 and publishes an NSIS installer from a version tag. Stable and beta GitHub
Releases provide `latest.yml` or `beta.yml`, blockmaps and installer hashes to the explicit in-app updater. Releases
also include plugin signatures, CycloneDX SBOMs, SHA-256 checksums and a GitHub provenance attestation. Windows
installers are currently unsigned and can display Unknown publisher or SmartScreen warnings. See
`docs/release-runbook.md`.

## 当前验证基线（1.5.25）

- 主程序版本 1.5.25；数据库 schema v3（`plugins.sort_order`）。
- 六插件版本：diary 0.4.11、dice-roller 0.1.6、gif-editor 0.3.8、theme-manager 0.1.12、turntable 0.1.10、unienv 0.5.7。
- 测试：宿主 35 个测试文件、256 项；六插件 190 项；供应链 16 项全部通过（当前 0 跳过）。
- 内置主题六套：亮色（默认）/深色/清新绿/海洋蓝/科幻面板/零号城区。
- 插件列表支持长按拖动排序与键盘上移/下移。
- 工具箱本体与 UniEnv 的 Windows VM 安装/取消/切换/回滚验收已通过。
- 历史发布文档已归档至 `docs/history/`，记录的是当时状态，版本号与测试数量不代表当前。

## License

MIT

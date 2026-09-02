# 开发、构建与验证

> 当前可编辑运行线：Tauri 2 / Rust / React，开发与验证基线为 **2.0.0-beta.6**。
> 根目录 Electron 1.7.3 仅为冻结参照，不接受功能性改动。

## 环境

- Windows x64（M1 已验证平台）
- Node.js 24.15.0（见 `.nvmrc`）
- npm 11.12.1（由根 `packageManager` 固定）

项目级 `.npmrc` 固定 `https://registry.npmjs.org/`；根 lockfile 不接受其他 registry
主机。认证信息、代理和私有镜像不得写入仓库配置。

根工程和 `plugins/*` 子工程组成 npm workspaces，当前插件目录包含 11 个正式插件；根目录的 `package-lock.json`
是唯一依赖锁定事实源。所有安装命令都从仓库根执行。

## 首次安装

```powershell
npm ci
```

`npm ci` 会一次安装宿主和所有插件依赖，并为冻结的 Electron 版本重建
`better-sqlite3`；失败会直接中止。

## Tauri 日常开发

```powershell
cd tauri-frontend
npm install
npm run build

cd ..\src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

根目录 `npm run check` 只作为冻结 Electron 兼容与供应链参照门禁。自动修改代码只能显式运行格式化命令。

## 2.0 页面与状态约定

- 页面在 `tauri-frontend/src/app-pages.ts` 注册，侧边栏、命令面板和页面标题必须同步提供入口。
- 长任务写入统一任务中心；插件安装至少包含预检、等待确认、提交、完成/失败四个阶段。
- 插件卡片身份由 `plugin-identity.ts` 维护。第一方插件发布者统一为 `CrucibleBox`，不得再用单字母作为唯一辨识。
- 工作台和设置卡片使用主题边框、圆角和切角，不添加宿主默认阴影；主题可覆盖几何风格，但不能露出底层矩形轮廓。

## Document Engine 0.7.0

- 统一流水线为 Layout Analysis → Native/OCR 文字来源选择 → Unicode/XML-safe normalization → TOC/章节树/区域识别 → 公式块与布局元数据 → Document IR v3。
- 格式转换和 Chunk 切分只消费 IR，不得再次触发 OCR；PDF 物理拆分直接操作原始页并输出真实 PDF。
- 文本进入 IR 前必须清除 XML 1.0 非法控制字符并修复字体编码断词；DOCX 输出前后均做 XML 解析检查，`invalidControlChars` 与 `invalidXmlChars` 必须为 0。
- 章节识别必须区分 chapter/section、编号段落、习题编号和正文；TOC entry 只能作为候选信息，不能进入正文 heading stack。Formula/Table/Image 区域不因存在 Text Layer 而跳过。
- Chunk 任务输出逐行 JSONL 与 manifest JSON；任务快照只返回受限元数据，完整结果通过输出路径读取，避免 IPC payload 超限。
- Hybrid Chunk 继续使用现有约 450–500 token 目标及既有 `target=512/min=180/max=800` 配置；本版本只修正 section/title、非法字符及公式/表格/图片元数据，不调整长度算法。
- 默认模型随插件离线分发，`ppocrv4-mobile-zh-en` 作为兼容 profile 保留；新增模型或流水线字段时必须同步缓存版本、协议文档、插件目录和 trusted policy。

## 插件市场开发约定

- 2.0.0 使用应用内置展示目录，并从 `tauri-stable/plugins.json` 获取权威下载地址和摘要；不接入 GitHub Marketplace，也不实现账户注册、登录或上传。
- 市场数据与安装执行分离：目录只描述插件，安装仍复用既有预检、权限确认、staging、journal 和原子替换流程。
- 市场主页保持双栏布局，必须显示图标、名称、版本、发布者、简介及“获取/打开”状态；详情页展示权限和长描述。
- 测试版从 `tauri-beta/plugins.json` 获取目录，正式版从 `tauri-stable/plugins.json` 获取目录；下载必须支持重试、重定向校验、临时文件和 SHA-256 校验。
- beta5 起市场在官方 GitHub Release 单一来源上支持手动刷新、目录进程内缓存、离线使用最近一次可用目录、未知官方插件动态展示、下载临时文件的 Range 断点续传、批量下载和全部更新；宿主会透传导入/预检失败的后端详情。下载仍不使用镜像源。
- beta6 起 Windows 优先使用 WinHTTP 自动代理/WPAD 获取目录，BITS 优先处理新插件包下载；ureq 仍作为兼容回退。下载任务不再撑大市场卡片或左侧布局，批量操作使用普通优先级。
- 在线获取仅接受 CrucibleBox 仓库的 HTTPS Release 地址，限制目录/包体大小并校验 SHA-256；不能直接执行仓库源代码。后续若引入第三方远程目录，必须再增加目录级签名和密钥轮换机制。

## 稳定版与测试版

- `stable` 读取 `tauri-stable/latest.json`，只发布正式版本；`beta` 读取 `tauri-beta/latest.json`，允许 `-beta.N` / `-rc.N`。
- 通道是持久化设置，检查更新时必须显式传给 Rust updater；禁止仅切换界面标签却继续访问同一端点。
- 两个滚动元数据互不覆盖。所有安装包仍要求 minisign 签名、SHA-512 元数据和 HTTPS。

## 生产构建

```powershell
npm run build
```

此命令先清理并构建插件目录中的所有插件，再构建 Electron 主进程、preload 和 React renderer。
插件不会被隐式打入宿主 ASAR。

## 插件产物

```powershell
npm run package:plugins
npm run verify:plugins
```

确定性 ZIP 和 `manifest.json` 输出到 `artifacts/plugins/`。发布文件集合由
`scripts/plugin-catalog.json` 显式定义；验证器会拒绝额外文件、缺失入口、版本漂移、
路径穿越、与当前 `dist` 不一致的内容或清单摘要漂移。

当前 1.5.23 artifacts manifest 的 SHA-256：

| 插件          | 版本   | SHA-256                                                            |
| ------------- | ------ | ------------------------------------------------------------------ |
| Diary         | 0.4.11 | `bba0677ab738600d031950152e6089207e4760d659fad8aeae68c94625a1984f` |
| Dice Roller   | 0.1.6  | `c10e29b2fdebf1e8f246629f62b7dcc6843758f8f0a12f2cc19335666d005e14` |
| GIF Editor    | 0.3.8  | `55c682e02f5a93c6afeb3f99753ba48a7264be2dc5d19b3329552858685424f7` |
| Theme Manager | 0.1.12 | `6e27a3c1739cf896114a6ab5f6f8e526fd8803bfecd35dc9ff2f5ac26f4338c5` |
| Turntable     | 0.1.10 | `b49d47af187add2e5821ada86cb8b84149d3ec2311c3392ccfa3ba536b666898` |
| UniEnv        | 0.5.7  | `73e0fc5be810402b0223831c57ec801d554bb7917db65493353f916f1d46822f` |

正式发布还需设置仓库外 Ed25519 私钥/公钥和 key ID；`npm run release` 会在构建后强制签名、
验签并生成七份 CycloneDX SBOM。详见 `docs/release-runbook.md`。

## 插件数据

新插件使用 `ctx.storage.get/set/delete/list`，并按需要声明 `storage:read`、`storage:write`。命名空间
由宿主绑定到插件 ID，插件请求中不传 namespace。`ctx.database` 只用于旧 SDK 兼容；生产插件和新模板
不得再创建全局表或发送原始 SQL。schema v2 的 Diary/Turntable 兼容迁移见
`docs/adr-0007-plugin-storage.md`。当前数据库 schema v3 增加 `plugins.sort_order`（列表排序），
v2→v3 迁移按既有显示顺序稳定回填，链路详见 `docs/architecture.md` 的插件排序一节。

## 桌面打包与冒烟

```powershell
npm run package:dir
npm run smoke:packaged
```

`package:dir` 生成用于 CI 和本地验证的未签名 unpacked 应用：
`release/win-unpacked/CrucibleBox.exe`。`smoke:packaged` 使用临时用户数据目录、隐藏
窗口启动它；临时数据库预置 schema v1 的 Diary/Turntable 数据，验证逐字节备份、事务迁移、旧表保留、
renderer/backend 后自动退出并清理。

正式安装包使用：

```powershell
npm run release
```

默认本地成品使用 `npm run package`（electron-builder，生成 NSIS 安装器），无需 GitHub 仓库、
GitHub Token、插件发布密钥或 Windows 证书；`npm run smoke:installer` 在随机临时目录完成真实
静默安装、packaged 启动与静默卸载冒烟。

插件清单签名与 Windows 安装器签名是两层独立控制。第一方插件仍使用仓库外 Ed25519 密钥；Windows
安装器按当前产品决策保持未签名，并在设置页和发布说明中明确 Unknown publisher/SmartScreen 限制。

## M1 完整验收序列

```powershell
npm run check
npm run build
npm run package:plugins
npm run verify:plugins
npm run package:dir
npm run smoke:packaged
```

所有测试使用工程数据或临时目录；本里程碑不读取、迁移或覆盖现有用户数据库。

## M1.2 UniEnv 验收

UniEnv 0.4.0 的任务协议、输入/路径边界、无 Shell 进程执行、可取消下载及安全 staging
详见 `docs/install-recovery.md`。插件自身为 9 个测试文件、116 项测试；制品白名单包含 15
个 manifest/runtime 文件，构建只生成可发布 JS，不再产生未打包的 `.d.ts`/`.map` 或
悬空 `sourceMappingURL`。

M2.10 已为全部受支持的 Python、Node.js、Git、Go 与 Temurin JDK Windows x64 制品固定官方 URL、
文件名和 SHA-256；下载在原子提升前流式校验，镜像也必须提供逐字节相同制品。来源与失败语义见
`docs/install-recovery.md`（可信服务一节）与 `docs/security-model.md`。

所有安装测试使用 fake spawn/fetch 和临时目录，未执行真实安装器。真实 Windows VM 的安装、取消、
切换与回滚 E2E 已在一次性 Windows VM 中完成并通过（1.5.23 基线，用户确认范围）；当前 UniEnv
版本为 0.5.7、11 个测试文件 132 项。

M2.11 增加静态版本生命周期目录。下拉框把目录首选置顶并显示"维护分支的旧补丁 / 已停止维护 / 旧版"；
单项与组合安装在创建任务前再次确认。状态依据日期和官方来源见 UniEnv 插件的制品目录设计
（`plugins/unienv/src/`，历史细节见 `docs/history/plugin-platform-m2.11.md`）。

M2.12 为 Node.js 24.18.1、Git 2.54.0、Go 1.26.5 以及 Temurin 17.0.20、21.0.12、25.0.4
固定官方 Windows x64 制品和 SHA-256，并升级内置组合。当前维护制品使用成功状态且不显示旧版二次确认；
旧目录和 Python 3.12.5 兼容项继续保留。依据与边界见 `plugins/unienv/src/` 的制品目录
（历史细节见 `docs/history/plugin-platform-m2.12.md`）。

M2.13 增加 Python 3.14.7 当前官方 Windows x64 安装器并设为目录首选。Install Manager 26.3 的
MSIX/Store 注册、自动更新与全局别名属于显式系统集成，不在后台静默部署；过渡与 3.16 前迁移要求见
`plugins/unienv/src/` 的制品目录（历史细节见 `docs/history/plugin-platform-m2.13.md`）。

## 当前已知门禁边界

- 根工程与正式插件 `npm audit` 当前为 0 漏洞；CI 每次重新查询 advisory 服务，不能替代持续升级。
- renderer 已按工作台、日志、设置和插件详情拆分；当前总 JS 2,790,229 B、静态入口 1,091,990 B、
  默认首页启动闭包 2,018,469 B，分别受 3.4 MB、1.3 MB 和 2.3 MB 上限保护。
- CI 仅在 Windows x64 执行 check/build/audit、unpacked 打包、Electron ABI 和 GUI 冒烟。macOS、Linux
  与 Windows ARM64 不在当前支持范围。

## Windows GitHub release and automatic update

`package:dir` produces an unsigned unpacked smoke artifact. Formal releases use the public repository's version-tag
workflow in `.github/workflows/release.yml`. The Windows runner creates the NSIS installer, `latest.yml` or `beta.yml`
and blockmap; validates metadata SHA-512, Electron fuses, native ABI and packaged startup; then publishes SBOMs,
SHA-256 checksums and provenance. No Windows certificate is required. The plugin Ed25519 private key must remain in
GitHub Secrets and must never enter the repository. See `docs/release-runbook.md`.

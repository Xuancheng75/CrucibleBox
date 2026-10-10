# CrucibleBox 2.0.1 交接文档

> 交接日期：2026-09-06
> 当前发布基线：CrucibleBox 2.0.1
> 本文性质：当前状态、发布证据、已知限制和后续接手规则
> 本文不替代 `AGENTS.md`、架构文档、安全模型或发布手册；如本文与规范冲突，以 `AGENTS.md` 和对应活文档为准。

## 1. 交接结论

当前仓库已经完成 Tauri 线 2.0.1 正式发布，GitHub Release 不是草稿，也不是预发布版本。当前工作区所在分支是：

```text
codex/beta8-download-workspace
HEAD: b38133d
origin/codex/beta8-download-workspace: b38133d
```

正式发布使用的 tag 和 Release 提交是：

```text
tag: tauri-v2.0.1
release commit: 634433a
release workflow run: 33969012776
```

注意：当前分支的 `b38133d` 是在已发布提交 `634433a` 之上的合并提交，已发布 tag 不应移动、重打或覆盖。后续任何功能或修复都应增加新的版本号，并通过新的分支、PR、tag 和 Release 完成。

当前已验证的 GitHub Release：

- 名称：`CrucibleBox-v2.0.1`
- tag：`tauri-v2.0.1`
- 状态：已发布，`isDraft=false`，`isPrerelease=false`
- 发布时间：`2026-09-05T14:02:46Z`
- 地址：[CrucibleBox v2.0.1 Release](https://github.com/Xuancheng75/CrucibleBox/releases/tag/tauri-v2.0.1)

工作区目前没有已跟踪文件改动；有若干未跟踪的本地文件和目录，均未进入本次提交或 Release。不要在未确认内容前删除它们。

## 2. 版本矩阵

| 部件 | 当前版本/状态 | 说明 |
| --- | --- | --- |
| CrucibleBox Tauri 宿主 | 2.0.1 | 当前正式版基线 |
| Tauri 前端 | 2.0.1 | React 18、Ant Design 5、zustand |
| Rust workspace / plugin host | 2.0.1 | Tauri 2.11.x 线 |
| Document Engine | 0.10.0 | 已暂停常规功能迭代，进入维护冻结 |
| UniEnv | 0.11.0 | 已暂停常规功能迭代，进入维护冻结 |
| Electron 根包 | 1.7.3 | 历史遗留线，功能冻结，禁止功能性升级 |
| 数据库 | schema v4 | 含 legacy storage 迁移和 L3 数据目录迁移 |
| 插件协议 | SDK v2 / backend RPC v2 | renderer 使用跨源 sandbox iframe，backend 使用 quickjs-ng sidecar |
| 官方插件 | 11 个 | 详见下方发布资产清单 |

宿主版本的来源分散在 Tauri 配置、Rust crate、前端 package 和 lock 文件中。根目录 `package.json` 仍然属于 Electron 遗留线，保持 1.7.3 是有意设计，不应因为宿主升到 2.0.1 而一起修改。

## 3. 2.0.1 发布内容和资产

本次正式 Release 已包含以下资产：

- `CrucibleBox_2.0.1_x64-setup.exe`
- `CrucibleBox_2.0.1_x64-setup.exe.sig`
- `latest.json`
- `plugins.json`
- `cruciblebox.cdx.json`
- `cruciblebox-plugin-host.cdx.json`
- `document-engine-0.10.0.zip`
- `unienv-0.11.0.zip`
- `diary-0.5.0.zip`
- `gif-editor-0.5.0.zip`
- `clipboard-manager-0.3.0.zip`
- `json-toolkit-0.3.0.zip`
- `turntable-0.3.0.zip`
- `dice-roller-0.3.0.zip`
- `exchange-rates-0.3.0.zip`
- `system-info-0.3.0.zip`
- `theme-manager-0.3.0.zip`

发布流水线使用 Windows x64，完成了 plugin host sidecar、OCR worker、ONNX Runtime、Rust、前端、插件打包、NSIS、updater、SBOM、artifact attestation 和 Release 发布步骤。远程工作流 `33969012776` 的各项门禁均已通过。

## 4. 更新通道和插件市场地址

当前必须区分“宿主更新地址”和“插件市场目录地址”，两者不是同一个资源。

### 4.1 宿主更新

Tauri updater 继续使用滚动稳定通道：

```text
https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-stable/latest.json
```

测试版继续使用独立的 `tauri-beta` 通道。正式版 tag 规则为 `tauri-vX.Y.Z`，测试版规则为 `tauri-vX.Y.Z-beta.N` 或 `tauri-vX.Y.Z-rc.N`。

### 4.2 插件市场目录

插件市场目录使用与版本绑定的 Release 地址：

```text
https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-v2.0.1/plugins.json
```

2.0.1 稳定目录已验证：

- schema：1
- application：`cruciblebox`
- application version：`2.0.1`
- plugin count：11
- 11 个插件 URL 均指向 `/releases/download/tauri-v2.0.1/`

这样做是为了避免稳定通道目录在下一次发布前被提前覆盖。以后发布新稳定版本时，必须同时确认：

1. 新 Release 中有版本化的 `plugins.json`。
2. 目录中的每一个插件 URL 都指向同一个新版本 tag。
3. 宿主更新使用的 `tauri-stable/latest.json` 已同步到新版本。
4. 测试版目录和稳定版目录没有交叉引用。
5. 旧版本不能因为滚动目录变化而被误判为当前插件版本。

此前工具箱曾出现 GitHub 连接超时、插件下载慢、下载到约 10% 后失败以及将 `tauri-stable` 与具体插件 Release 混用的问题。2.0.1 已将插件目录定位改为版本化的 `tauri-v2.0.1`，但网络可达性、代理行为、GitHub Release 资产下载和断点续传仍属于后续可维护性风险，不能仅凭 URL 正确就认为所有网络环境都稳定。

## 5. 本次 2.0.1 变更范围

### 5.1 版本对齐

已同步 2.0.1 的关键文件包括：

- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/cruciblebox-plugin-host/Cargo.toml`
- `src-tauri/cruciblebox-plugin-host/Cargo.lock`
- `tauri-frontend/package.json`
- `tauri-frontend/package-lock.json`

版本对齐脚本已确认 Tauri 2.0.1 在 7 个相关文件中一致，同时根目录 Electron 遗留包仍为 1.7.3。

### 5.2 插件市场目录定位

插件市场 URL 生成逻辑位于：

```text
src-tauri/src/commands.rs
```

当前规则是按宿主发布版本生成版本化 Release 地址。稳定渠道的 beta 版本会使用稳定基线版本构造目录版本；不再依赖滚动的 `tauri-stable/plugins.json` 作为插件市场主目录。

### 5.3 UniEnv 请求标识

官方运行时版本发现请求的 User-Agent 已统一使用：

```text
CrucibleBox/<CARGO_PKG_VERSION>
```

实现位于：

```text
src-tauri/src/unienv_versions.rs
```

## 6. 当前架构交接

### 6.1 Tauri 宿主

主要装配点和职责如下：

- `src-tauri/src/main.rs`：Tauri 装配、updater、renderer 自定义协议、数据库初始化、数据路径迁移和命令注册。
- `src-tauri/src/commands.rs`：设置、应用、插件、会话、数据库状态等 IPC 命令，包含主窗口校验和 settings key 白名单。
- `src-tauri/src/db.rs`：rusqlite bundled、WAL、schema v1-v4 迁移、legacy storage 迁移和日志清理。
- `src-tauri/src/data_dir.rs`：`%APPDATA%\openbox` 到 `%APPDATA%\cruciblebox` 的 checkpoint、原子 rename 和恢复流程。
- `src-tauri/src/plugin_session.rs`：插件 renderer session registry。
- `src-tauri/src/plugin_protocol.rs`：插件资源协议处理器，Windows 使用 path 型 `http://cruciblebox-plugin.localhost/<token>/index.html`。
- `src-tauri/cruciblebox-plugin-host/`：插件 backend sidecar，使用 quickjs-ng、CJS loader、帧协议和 envelope v2。

插件不是直接加载到宿主页面，而是通过 session、sandboxed iframe 和版本化 MessagePort RPC 接入。backend 侧故障隔离不等于安全沙箱；插件信任模型、权限边界和安装策略详见 `docs/security-model.md`。

### 6.2 Tauri 前端

- `tauri-frontend/src/App.tsx`：应用骨架、更新检查、内存探针、插件宿主入口。
- `tauri-frontend/src/pages/Marketplace.tsx`：插件市场、双栏布局、官方只读目录、安装状态和更新入口。
- `tauri-frontend/src/marketplace-catalog.ts`：目录读取、版本和插件资产映射。
- `tauri-frontend/src/pages/TaskCenter.tsx`：宿主长任务统一状态、进度和失败入口。
- `tauri-frontend/src/store/task.store.ts`：任务状态持久化/订阅基础。
- `tauri-frontend/src/plugin-identity.ts`：第一方插件分类、颜色和发布者身份。
- `tauri-frontend/src/PluginHost.tsx`：插件 iframe 宿主、session 创建和 frame bridge 握手。
- `src/plugin-runtime/`：frame entry 和 `PluginFrameBridge`，由两条前端线共享。

### 6.3 冻结的 Electron 线

以下目录属于历史遗留实现，不是当前 Tauri 功能实现：

- `electron/`
- `database/`
- `plugin-system/`

这些目录已按规范冻结，带有 `ARCHIVED` 标记。需要迁移或对照逻辑时先查看 `docs/electron-legacy-registry.md`，禁止为了修复当前 Tauri 问题而进行功能性修改。

## 7. Document Engine 交接状态

Document Engine 当前版本为 0.10.0，已按用户决定暂停常规功能迭代，进入维护冻结。详细能力、数据结构、已知问题和条件式未来路线见：

- [Document Engine 当前状态](document-engine-status.md)
- [Document Engine 开发报告](document-engine-development-report.md)
- [系统架构](architecture.md)
- [维护计划](maintenance-plan.md)

### 7.1 当前处理链

当前设计已经明确区分解析输出和面向人的格式转换输出：

```text
原始 PDF
  -> 原生文本检测 / 页面渲染 / Layout 分析
  -> Native Text 或 OCR / Formula / Table / Image 区域处理
  -> 文本清洗与归一化
  -> 阅读顺序
  -> 结构恢复
  -> Document IR
       ├─ JSON / Markdown / TXT / Hybrid Chunk（AI、RAG、搜索）
       └─ DOCX / HTML / PDF / 面向人阅读的 Markdown（格式转换）
```

Native Text 和 OCR 是普通文字的两种来源，不能用“有 text layer”作为跳过公式、图片或表格处理的理由。Document IR 应保留块类型、页面、bbox、section、来源、公式/表格/图片元数据，转换器再使用这些布局信息进行渲染。

### 7.2 已验证的回归结果

两份桌面 PDF 只用于回归测试，不参与训练，也不应在代码中写入文件特例。

#### Fogharbor 扫描 PDF

文件：`C:\Users\hjc\Desktop\fogharbor_botanical_field_notes_scanned.pdf`

- 页数：10
- native text blocks：0
- OCR text blocks：214
- heading blocks：28
- formula blocks：0
- chunks：5
- quality flags：空
- 全 OCR 相关单元测试：通过，用时约 74.23 秒

这说明当前测试数据上中文扫描 OCR、标题识别、公式误判抑制和无标题文档的 chunk fallback 已有可用结果，但 OCR 质量仍不是出版级，不应据此宣称所有中文扫描件都能达到同等准确率。

#### Gilbert Strang《Linear Algebra and Its Applications》

文件：`C:\Users\hjc\Desktop\linear algebra by strang 4 th edition.pdf`

- 页数：542
- native text good pages：541
- native text blocks：61167
- OCR text blocks：0
- heading blocks：47
- TOC entries：195
- section path ratio：约 0.9954
- tree consistency score：1
- formula blocks：5954，candidate total 6010
- formula AST success：5543
- LaTeX success：5548
- matrix blocks：56
- suspicious formula count：467
- invalid control characters：0
- invalid XML characters：0
- DOCX XML parse：通过
- chunks：549
- average tokens：512.08
- median tokens：513
- rag-eligible chunks：339
- overall quality：通过，但因公式检测不确定性标记为 degraded

当前数学教材结果已经满足“可继续维护和回归”的基础，但公式碎片、误判和布局级还原仍是已知限制。`sectionPathRatio=1` 不能单独证明章节树正确，必须结合真实父子关系抽样、标题误判检查、TOC 污染检查和公式结构检查。

### 7.3 模型和缓存现状

当前文本 OCR 使用 PP-OCR 线路，默认 profile 为：

```text
ppocrv6-small-det-v5-mobile-rec
```

模型目录默认位于：

```text
%APPDATA%\cruciblebox\document-engine\models
```

兼容 profile 包括 `ppocrv4-mobile-zh-en`。模型来源按项目文档使用固定的 ModelScope 镜像策略，当前没有将 Hugging Face 作为默认来源。

OCR 缓存 key 必须包含以下维度，否则模型、配置或引擎变更后可能错误复用旧结果：

- PDF hash
- OCR engine
- det model version
- rec model version
- dictionary/language
- OCR configuration version

### 7.4 已知限制和维护边界

- OCR 识别准确率仍受扫描质量、语言混排、字体、小字号和图片背景影响。
- 数学公式在复杂矩阵、上下标、分式、跨行表达式上的二维恢复仍非出版级。
- PDF 原生文本的控制字符清洗、XML-safe 输出和 DOCX 合法性已有门禁，但格式转换尚不能承诺像素级还原原 PDF。
- 2000 页 PDF 的目标需要继续做压力测试，当前数据不足以证明所有机器配置都能稳定处理。
- Windows 下 PDFium 重复初始化、OCR worker、ONNX Runtime DLL、权限、长路径和输出目录仍是重点诊断对象。
- 模型下载失败不能破坏普通 OCR；任何新增模型都必须支持 SHA-256 校验、失败清理、删除和重装。
- Document Engine 后续只接受安全、崩溃、数据损坏、构建阻塞和明确的严重正确性修复；新的 OCR 精度路线需先重新评估资源和回归基线。

## 8. UniEnv 交接状态

UniEnv 当前版本为 0.11.0，已经暂停常规功能迭代。详细说明见 [UniEnv 当前状态](unienv-status.md)。

### 8.1 当前职责

UniEnv 是宿主固定摘要的可信服务，不是普通插件 backend。对应 Rust 模块为：

- `src-tauri/src/unienv_catalog.rs`：固定制品完整性目录和 SHA-256。
- `src-tauri/src/unienv_versions.rs`：官方版本发现和 8 秒硬超时。
- `src-tauri/src/unienv_install.rs`：下载、解压、junction/current 切换和安装原语。
- `src-tauri/src/unienv_task.rs`：单飞任务、进度和取消。
- `plugins/unienv/plugin.json`
- `plugins/unienv/src/main.ts`
- `plugins/unienv/src/renderer.tsx`

支持 Python、Node.js、Git、Go、Java、Rust、PHP、Ruby、Zig、Deno、Bun 及组合包。安装使用 staging、journal、原子 promote、版本目录和 current junction/symlink，构造失败时拒绝激活。

### 8.2 安全和维护边界

- 官方制品使用 HTTPS、SHA-256 和固定 trusted policy。
- 宿主 PermissionGuard 是权限边界，不能省略。
- backend sidecar 是故障隔离，不是 OS 级强制沙箱。
- 不再扩展新的运行时、版本源、安装策略或大型性能重构。
- 只处理安全、崩溃、数据损坏、构建阻塞和明确的安装恢复缺陷。

## 9. 验证证据

本次 2.0.1 已完成的本地验证：

```text
node scripts/verify-tauri-version-alignment.mjs       PASS
cargo fmt --check                                     PASS
cargo clippy --workspace --all-targets --locked -- -D warnings  PASS
cargo test --workspace --locked                       248 passed, 0 failed, 3 ignored
node node_modules/typescript/bin/tsc --noEmit -p tauri-frontend/tsconfig.json  PASS
node node_modules/vite/bin/vite.js build              PASS
```

远程 Windows x64 Tauri Release 工作流已完成并成功发布。正式接手新改动时，仍必须按 `AGENTS.md` 执行完整门禁：

```bash
npm run check

cd src-tauri
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd ../tauri-frontend
npm run build
```

如果修改插件，再进入对应 `plugins/<id>/` 执行：

```bash
npm run clean && npm run build
```

涉及 UniEnv 时，还要更新并复核 trusted digest：

```bash
npm run update:trusted-policy
```

## 10. 当前未跟踪文件和本地工作痕迹

交接时工作区存在以下未跟踪路径：

```text
.pnpm-store/
HANDOFF-1.9.14.md
latest.json
poc-ocr/
tmp-model-test/
```

处理原则：

- 它们不属于 2.0.1 Release 资产。
- 当前没有执行删除或清理，以保留可能的用户测试结果。
- 根目录 `latest.json` 是旧的未跟踪本地产物，不能当作当前 Release 的 updater manifest。
- 根目录 `HANDOFF-1.9.14.md` 是旧版本交接文件，不能当作当前版本事实来源。
- `poc-ocr/` 和 `tmp-model-test/` 可能包含实验或模型测试痕迹；在清理前应先确认是否需要保留。
- 后续提交前必须再次执行 `git status --short`，确保不会误把这些本地文件加入提交。

## 11. 风险清单和待办优先级

### P0：发布后立即保持可观测

1. 监测稳定 updater 是否持续返回 `latest.json`，以及正式版是否被错误标记为 beta。
2. 监测版本化 `plugins.json` 是否可访问，目录内 11 个插件 URL 是否均返回对应 Release 资产。
3. 复核用户网络环境下 GitHub Release 资产的下载失败率、代理兼容性和错误信息。
4. 出现下载失败时先区分目录读取失败、资产连接失败、超时、代理、权限、临时文件和校验失败，不要只增加重试次数。

### P1：需要明确授权后再做

1. 插件下载的断点续传、临时文件恢复、并发/队列、重试退避、带宽与磁盘空间提示。
2. 插件市场刷新、批量下载、全部更新、取消任务、任务中心合并和页面切换后的任务持续性。
3. 插件更新时 UI 尺寸稳定、进度显示移出卡片、侧边栏高亮和工作台返回主页语义统一。
4. 插件卡片简介过长导致的高度不一致、按钮重叠、阴影和切角残留。
5. 特定插件启用后的内存异常，尤其是从数 MB 增长到 GB 级的生命周期、sidecar、iframe、日志和缓存问题。
6. Document Engine 图片 PDF 的 OCR worker、ONNX Runtime DLL、模型路径、PDFium 初始化、权限和 payload 大小问题。

### P2：冻结模块的条件式修复

1. Document Engine 只在发现数据损坏、崩溃、严重错误结果或构建阻塞时修复。
2. UniEnv 只在安全、崩溃、数据损坏、安装恢复或构建阻塞时修复。
3. 公式识别、OCR 精度、格式转换版式等大范围功能增强，必须先重新获得明确的版本计划和验收数据，不应直接在冻结版本上扩展。

## 12. 后续接手步骤

新维护者开始工作时按以下顺序执行：

1. 阅读根目录 `AGENTS.md`。
2. 阅读本文件以及 `docs/architecture.md`、`docs/development.md`、`docs/release-runbook.md`、`docs/security-model.md`、`docs/maintenance-plan.md`。
3. 执行只读状态核对：

   ```bash
   git status --short --branch
   git log --oneline --decorate -8
   git show --stat tauri-v2.0.1
   git ls-remote --tags origin tauri-v2.0.1
   gh release view tauri-v2.0.1
   ```

4. 先复现问题并保存日志、版本、URL、HTTP 错误、文件大小、任务状态和系统环境。
5. 判断问题是否属于冻结模块、发布链路、插件市场、下载器、宿主 UI 或插件自身，不要跨模块盲改。
6. 若确需修改，创建短分支并保留现有用户改动；遵循 Conventional Commits 和一个工作包一个 PR 的规则。
7. 修改后执行对应单元测试、构建、回归测试和发布前门禁。
8. 新版本必须同步版本矩阵、README、开发文档、维护文档、发布手册、插件目录和 Release 资产。
9. 发布前确认 tag 指向预期提交，禁止移动已经发布的 `tauri-v2.0.1`。
10. 发布后同时验证 GitHub Release、`tauri-stable/latest.json`、版本化 `plugins.json`、安装包签名和插件 ZIP 校验。

## 13. 事实来源索引

- [项目架构](architecture.md)
- [开发指南](development.md)
- [发布手册](release-runbook.md)
- [维护计划](maintenance-plan.md)
- [安全模型](security-model.md)
- [插件 SDK](plugin-sdk.md)
- [安装恢复](install-recovery.md)
- [Electron 遗留登记](electron-legacy-registry.md)
- [Document Engine 状态](document-engine-status.md)
- [UniEnv 状态](unienv-status.md)
- [项目根规范](../AGENTS.md)

本交接文档只描述当前已核对的状态。旧的 beta4、beta6、beta7、beta8 计划和旧版交接文件属于历史背景；它们不能被解释为 2.0.1 尚未完成的当前任务，也不能覆盖本文件中已经明确的冻结决定。

# CrucibleBox 维护复杂度优化方案（整合版）

> 状态：维护规范（2.0.0-beta.8 工作包）。历史规划数字仅作迁移背景，不作为当前验收结论。
> 基线：CrucibleBox 2.0.0-beta.8（Tauri 为唯一可编辑运行线，Electron 1.7.3 冻结）。
> 唯一可编辑源码：`E:\CrucibleBox_Sourses`（git 仓库；beta6 发布提交以实际发布 tag 为准）。
> `E:\CrucibleBox_Plugins` 为只读镜像 / 发布备份，不参与构建。

## 0. 目标与约束

- 目标：降低单人维护成本；减少重复实现；保持插件安全边界；保持安装事务与崩溃恢复；保持 UniEnv 可信服务模型；保持 GitHub 发布/签名/SBOM/attestation；保持插件 API v2 兼容；降低文档与构建链漂移风险。
- 明确不做（护栏）：
  1. 不把插件 backend 合并回主进程；
  2. 不取消跨源 sandboxed iframe；
  3. 不取消 utility process；
  4. 不把 UniEnv 降级为普通插件权限；
  5. 不删除安装 journal；
  6. 不同时升级 Electron、重构 RPC、删除 sql.js；
  7. 不接入 GitHub Marketplace、账户系统或静默自动安装；官方目录、手动刷新和用户确认后的插件更新属于 Tauri 2 正常能力；
  8. 不提前支持 macOS、Linux、ARM64；
  9. 不维护第二套可编辑插件源码；
  10. 不再增加阶段性 milestone 文档。

## 0.1 beta6 工作包与验收范围

beta6 在 beta5 的稳定基础上，收敛 Windows 代理下的市场可用性，并完成 Document Engine 结构正确性、数学文档和格式转换可靠性验收；不调整 Hybrid Chunk 的目标长度、min/max token 或基本 merge 逻辑。

| 优先级 | 范围                 | 验收标准                                                                                                                                                       |
| ------ | -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P0     | 市场目录与下载       | Windows WinHTTP 自动代理/WPAD 获取目录；BITS 优先处理新包，ureq/Range 回退；官方 GitHub 单一来源、HTTPS、大小、SHA-256 校验不变；批量下载/全部更新复用任务队列 |
| P0     | 市场布局与交互       | 进度移到固定下载任务栏；卡片高度和左侧布局稳定；常态无复选框，右键或顶部多选进入选择态；刷新固定在顶部右侧                                                     |
| P0     | IPC 稳定性           | 任务快照按节点数和深度限流，完整结果走输出文件；图片类 PDF 不因 IPC payload 预算失败                                                                           |
| P1     | Document Engine 结构 | 原生文本质量标记、XML-safe 清洗、TOC 隔离、重复页眉页脚过滤、标题候选评分与章节树一致性指标；正确生成 parentId/sectionId/sectionPath                           |
| P1     | 数学与区域           | 公式块保留 LaTeX/plainText/bbox/page/confidence/source；表格、图片、实验性 figure/vector 区域独立记录，不被 Text Layer 短路                                    |
| P1     | 输出与回归           | 解析型 JSON/MD/TXT/Chunk 与 DOCX/HTML/PDF 阅读型转换分离；Thomas 1348 页原生文本回归、扫描 PDF 全 OCR 回归、DOCX XML 门禁和 Hybrid Chunk 指标全绿              |

beta6 不包含下载镜像、GitHub Marketplace、账户系统、Windows Authenticode 证书，也不把插件 backend 改造成不可信代码沙箱。

## 0.1 beta7 修复计划

beta7 优先收敛 beta6 暴露的插件下载故障；插件市场下载链路不引入镜像源，仍直连 GitHub Release；Document Engine 模型制品允许使用固定、可校验的 ModelScope 镜像，不改变插件安装事务、签名校验和 Document Engine 的现有架构。

| 优先级 | 范围 | 计划内容 | 验收标准 |
| ------ | ---- | -------- | -------- |
| P0 | 插件包下载直连 | 插件目录和插件包下载链路明确使用官方 GitHub Release 直连；移除下载路径对 `HTTP(S)_PROXY`、WinHTTP/BITS 预配置代理和本地代理端口的隐式继承。BITS 若继续保留，必须显式设置 no-proxy；兼容回退也必须使用同一条直连策略。 | 代理端口不可用时不再尝试 `127.0.0.1:7897` 等本地代理；错误信息明确区分 DNS/连接失败、HTTP 状态、断点续传和 SHA-256 校验失败；直连下载 11 个官方插件包全部通过大小与 SHA-256 校验。 |
| P0 | Release 地址绑定 | `stable` / `beta` 清单中的每个插件 URL 必须由实际发布 tag 生成并校验，禁止引用不存在的 `tauri-v2.0.0` 或未来版本；beta 清单绑定当前 beta tag，stable 清单在 2.0.0 正式版发布前继续绑定最后一个真实稳定 Release。 | CI 在发布前验证清单 URL 的仓库、tag、artifact、版本、大小和摘要；所有 URL 返回可下载资产；清单中的应用版本、插件包版本与 Release 资产一致。 |
| P0 | 通道选择一致性 | 插件市场、更新器和下载命令共用持久化的 stable/beta 通道，不允许界面显示 beta 却请求 stable 清单，也不允许 beta 应用无提示读取错误通道。 | beta6/beta7 安装包默认或按设置访问 `tauri-beta/plugins.json`；稳定通道只访问 `tauri-stable/plugins.json`；切换通道后刷新目录并重新计算新增/更新数量。 |
| P1 | 下载诊断与恢复 | 在任务中心记录实际通道、Release tag、传输模式（direct）、HTTP 状态、重试次数、断点文件大小和最终校验结果；失败后保留可恢复断点，但不把网络错误伪装成固定 10% 进度。 | 网络失败提示包含可行动原因；新任务、断点恢复、重复下载、批量下载和全部更新均可重试；进度从真实字节数开始，失败任务不显示为已完成。 |

### beta7 Document Engine 边界重构计划

Document Engine 在 beta7 只做有边界的管线修复，不推翻现有 Document IR，也不重新调整已经稳定的 Hybrid Chunk 目标长度、最小/最大长度和基本 merge 逻辑。两份回归样本为 `fogharbor_botanical_field_notes_scanned.pdf` 与 `linear algebra by strang 4 th edition.pdf`；模型制品允许使用已备案的 ModelScope 镜像，但必须固定版本、逐文件通过 SHA-256 校验，禁止使用未验证的 ONNX 转换物。

| 阶段 | 范围 | 交付内容 | 验收门槛 |
| ---- | ---- | -------- | -------- |
| VNext.1 | 统一文本清洗 | Native/OCR 统一 Unicode Normalize、XML 1.0 控制字符清理、断词修复；输出前二次 XML-safe 检查 | `invalidControlChars=0`、`invalidXmlChars=0`，DOCX XML 可解析 |
| VNext.2 | TOC 隔离 | 识别 `toc` / `toc_entry`，目录只提供章节候选，不进入正文 heading stack | TOC 不污染正文 `sectionPath`，`tocEntryCount` 可追踪 |
| VNext.3 | 结构树 | 结合编号、视觉高度、bbox、页面位置、上下文、习题区域区分 chapter/section/list/exercise | 20–50 个章节样本抽检；`parentId`、`sectionId`、`sectionPath` 父子关系正确 |
| VNext.4 | 来源与版面解耦 | LayoutDetector 接口与文字来源分离；Native/OCR 只负责普通文字，页眉页脚/页码单独保留 | 有文本层仍运行版面分类；扫描页不因低置信度把视觉标题全部丢失 |
| VNext.5 | 数学/区域块 | FormulaDetector 与 FormulaRecognizer 分离；严格拒绝标题、页码、时间、URL、编号；Formula Block 保存 raw/normalized LaTeX、plainText、bbox、page、confidence、engine、modelVersion | `FIELD ARCHIVE / FOGHARBOR`、`03/10`、`08:05` 不得为公式；公式不再产生重复相邻 token |
| VNext.6 | 输出适配 | 解析型 JSON/MD/TXT 与阅读型 DOCX/HTML/PDF 继续分离；Markdown 使用规范公式块 | Formula/TOC/结构字段在解析输出中可供 AI/RAG 使用 |
| VNext.7 | DOCX 合法性 | XML-safe → OOXML renderer → XML parse；逐步加入 Heading/List/Table/Image/Caption/Equation/Page Break/Header/Footer/Page Number | `document.xml parse=PASS`、Word/LibreOffice 打开、DOCX→PDF 渲染通过后再做视觉相似度优化 |
| VNext.8 | 质量门控与回归 | 文档级 `ragQuality` 与 Chunk 级 `ragEligible` 双门控；无标题扫描文档按页/语义页回退切块；输出诊断指标 | 扫描 10 页不得只有 1 个 chunk；质量不通过时不得无条件让全部 chunk 进入 RAG |

模型策略：Auto/Mixed 只选择通用 `PP-OCRv5_mobile_rec`；英文优化模型必须显式选择。`PP-DocLayout-M` 作为版面/公式候选检测目标，`PP-FormulaNet_plus-S/M` 作为后续识别模型；在备案镜像模型文件、Windows runtime/依赖和两份样本回归全部具备前，不把轻量文字适配器标记为已完成的 FormulaNet。模型下载失败不得破坏普通 OCR，必须保留缺失状态、重试和 SHA-256 失败原因。

beta7 Document Engine 发布门禁：Rust workspace、插件独立构建、前端构建全绿；两份 fixture 的前后指标落盘；至少检查 `headingCount`、`suspectedFalseHeadingCount`、`tocEntryCount`、`formulaBlockCount`、`nativeTextBlockCount`、`ocrTextBlockCount`、`chunkCount`、平均/中位 token 和 RAG 门控；若通用 OCR 或目标版面/公式模型只有 URL 而无可校验制品，则 beta7 只能作为代码预发布候选，不得宣称扫描中文恢复和 FormulaNet 已验收。

beta7 不为插件下载增加镜像，不接入账户或 GitHub Marketplace；Document Engine 模型只使用已备案的 ModelScope 镜像。插件直连策略意味着在本机网络禁止直接访问 GitHub 时，下载仍会失败，但错误必须准确说明为直连网络不可达，而不是代理超时或无效 Release 地址。

beta7 发布前专项检查：清理下载临时目录后分别验证 beta 与 stable 清单；对所有插件执行单个下载、断点恢复、重复下载、批量下载和全部更新；检查清单中不存在 `tauri-v2.0.0` 等未发布 tag；确认 `latest.json`、`plugins.json`、安装包、签名和 SBOM 来自同一发布 tag。

## 0.2 beta8 候选工作包（持续补充，暂不发布）

beta8 纠正 beta7 将“直连”作为唯一传输路线的假设：GitHub Release 地址仍是唯一可信插件来源，但传输层必须能使用系统代理。当前仅在本地工作分支实施和验证，不提交 tag、不更新滚动通道、不创建 GitHub Release；后续需求继续并入本节后再统一冻结范围。

| 优先级 | 范围 | 当前方案 | 发布前验收 |
| ------ | ---- | -------- | ---------- |
| P0 | 统一下载网络策略 | 工具箱更新、市场目录、插件包共用 Auto/System/Manual/Direct 设置；Auto 优先显式地址，否则跟随系统代理；官方 URL、HTTPS、大小和 SHA-256 边界不变 | 四种模式分别验证；代理可用、代理中断、恢复网络、HTTP 错误和摘要错误均显示真实原因 |
| P0 | 插件断点与重试 | 新下载优先 BITS，瞬时错误保持任务并等待恢复；兼容链使用持久 `.part` + HTTP Range，失败不删除可用断点；无进度超时提高到 300 秒 | 在 10%、50% 主动断网后恢复，字节进度从已有断点继续；最终大小和 SHA-256 通过 |
| P0 | OCR 运行时打包 | OCR Worker 启动时从显式路径、worker 同目录、`binaries`、`resources` 和应用目录解析完整 ONNX Runtime DLL 对，并向子进程注入路径 | 安装版与便携版扫描 PDF 不再报告 `onnxruntime.dll not found`；运行时资产脚本验证 DLL 成对存在 |
| P1 | 页面生命周期 | 设置、市场和任务页首次打开后保持挂载；插件页切换侧栏后保持后台状态；插件页映射为工作台高亮，点击工作台仍返回主页 | 下载中切页不中断；插件内状态和 iframe 会话保持；导航高亮和返回主页行为一致 |
| P1 | 任务与日志合并 | 删除独立“插件日志”侧栏入口，任务中心提供“任务/运行日志”分页；全局任务浮层不改变市场卡片布局；日志支持筛选、展开和复制完整详情 | 下载任务跨页面可见；日志完整展示来源、时间、级别和原始消息；完成/失败状态准确 |
| P1 | 市场卡片布局 | 卡片采用固定网格行、两行简介和底部动作区；状态标签预留空间，下载进度移出卡片 | 简介长短、更新状态和批量任务均不改变卡片高度或左侧布局 |
| P1 | 转换输出目录 | Document Engine 转换与 PDF 解析统一使用只读目录框和“选择输出目录”；目录下自动生成目标扩展名，同格式转换使用 `-converted` 防覆盖 | Markdown/TXT/HTML/DOCX/PDF 均写入所选目录；PDF→PDF 不覆盖输入文件 |

### beta8 第二部分：数学教材结构化输出

不更换普通英文 OCR 主模型，不改 Hybrid Chunk 的目标/min/max token 与基本小块合并算法。处理顺序固定为：结构分类与页面噪声标记 → 普通正文断词修复 → Math Region/二维 token → Math AST → Markdown/OMML → Clean Document IR → RAG Chunk。

| 优先级 | 范围 | 实施内容 | 验收标准 |
| ------ | ---- | -------- | -------- |
| P0 | 英文断词 | 仅对已确认的正文段落执行显式连字符与高置信度词典式断词修复；记录 original/merged/confidence/rule | `Certainly`、`Difference` 等恢复；正常换行、标题、列表和公式不误拼 |
| P0 | Math AST | 原生 PDF 公式合并时保留 token bbox、中心、基线、高度、置信度；统一 AST 支持上下标、方程、方程组和规则矩阵 | `A^T`、`A^{-1}`、`λ_1`、矩阵不再按字符拆散；原始 token 可调试追踪 |
| P0 | 数学导出 | Math AST 统一生成 LaTeX；DOCX 通过 OMML 输出上下标、分式、根号和二维矩阵 | Markdown 公式边界规范；DOCX XML 可解析且矩阵生成 `m:m/m:mr` 结构 |
| P1 | 页面噪声 | 结合边缘位置与跨页重复识别 header/footer；独立识别阿拉伯和罗马页码 | 噪声块保留在 Document IR，但不进入 Markdown/DOCX 正文和 RAG |
| P1 | 元数据与 RAG | 公式/矩阵/图片 flag 从结构块计算；增加公式类型、数量、最低置信度、图片类型与断词统计；公式、矩阵、图片作为 atomic block | 不因单个字符或等号误报公式；长矩阵和方程组不被 Chunker 从中间切断；现有约500 token 分布不主动调整 |

回归固定包含 Strang 教材至少20页抽检以及 Fogharbor 10页扫描样本。若本机缺少 Strang 原文件或默认 PP-OCRv5 模型，只能报告对应项未执行，不得以合成测试冒充真实文档验收。

后续工作仍包括：应用安装包下载的跨进程持久断点、模型下载的 Range 续传、任务队列落库与重启恢复、下载诊断字段结构化，以及真实代理环境下的端到端故障注入。未完成这些验收前，不宣称 beta8 的“统一下载器”全部完成。

## 0.3 beta5 工作包与验收范围

本工作包不推翻现有 Tauri、插件独立构建或 Document Engine 架构，重点收敛 beta3 暴露的市场与导入故障，并把 Document Engine 的结构契约落实到可验证行为：

| 优先级 | 范围               | 验收标准                                                                                                                                                                                                        |
| ------ | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P0     | 插件导入与宿主兼容 | Manifest v2 接收 `minHostVersion`；不兼容、权限、路径和安装预检错误透传到界面与任务中心；Electron 冻结线无功能改动                                                                                              |
| P0     | 插件市场可用性     | stable/beta 目录选择正确；支持手动刷新、5 分钟进程内缓存、网络失败时最近目录回退；新增插件可动态展示，已安装插件显示打开/更新                                                                                   |
| P0     | 下载可靠性         | HTTPS/Release 来源、大小和 SHA-256 校验保持；支持显式代理、完整包复用、临时文件和 Range 续传；下载过程显示真实字节进度                                                                                          |
| P1     | Document Engine    | OCR 语言选择参与模型方案；缓存 key 含源 PDF hash、engine、det/rec/dictionary 身份和配置版本；heading/page number/公式编号过滤；parentId、title、sectionPath 不自指；解析导出与阅读型转换、真实 PDF 拆分保持分离 |
| P1     | 发布链和文档       | 11 个插件独立构建并打包，Tauri 版本七点对齐，前端/Rust/插件门禁全绿，更新 README、开发指南、发布 runbook 和变更记录                                                                                             |

beta5 不包含下载镜像、GitHub Marketplace、账户系统、Windows Authenticode 证书，也不把插件 backend 改造成不可信代码沙箱。网络无法访问 GitHub 时，离线缓存只保证继续查看最近目录；首次下载仍依赖用户网络、代理或企业出口配置。

## 1. 复杂度热点（审计确认）

| #   | 热点                   | 结论                                                                                                                         |
| --- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 1   | RPC 三套重复基础设施   | 三套独立信封：backend RPC(v2)、renderer RPC(v1)、host IPC（无版本），外加第 4 个迷你握手；校验器/错误码/pending 追踪三处重复 |
| 2   | PluginManager 上帝对象 | 888 行；9 张状态 Map；performActivation(~215 行) + handleChildRequest(20 分支 switch) + 日志 SQL + 崩溃恢复编排仍内联        |
| 3   | 双数据库引擎           | 已收敛：1.7.0 起生产仅 better-sqlite3（WAL）单引擎；`EngineDb` 接口保留供测试经 `setDatabaseForTesting` 注入 sql.js 内存引擎 |
| 4   | 安装事务               | 已收敛为事务族（InstallationService/DirectoryTransaction/Journal/Recovery），残留维护锁与恢复编排在 PluginManager            |
| 5   | UniEnv 与宿主强耦合    | 运行期直接 import 插件源码；digest 手工钉死、无生成脚本                                                                      |
| 6   | 发布链缺单一 manifest  | 版本散落 ≥7 处；CI 手工重放 release 链（两处维护点）                                                                         |
| 7   | openbox-api.d.ts 复制  | 5 插件 + 模板共 6 份 102 行完全一致；unienv 多 1 行                                                                          |
| 8   | 源码镜像               | E:\CrucibleBox_Plugins 为陈旧 CRLF 快照（theme-manager plugin.json 文案落后）                                                |
| 9   | 文档漂移               | AGENTS.md 1087 行：基线/remote/路径/三平台记录均已漂移或自相矛盾                                                             |
| 10  | clean checkout 风险    | dist 全局 gitignore、81/347 文本文件含 CR、unienv digest 字节级 pin                                                          |
| 11  | 构建产物膨胀           | ~1.43GB / 35,300 文件（node_modules 721MB + release 673MB + ...）                                                            |
| 12  | theme-manager 无测试   | 无 tests/ 目录、无 vitest 依赖                                                                                               |
| 13  | 大文件/大测试          | plugin-system 6 个 300-900 行核心文件；安装/事务/恢复测试 4 文件 2,348 行                                                    |
| 14  | 主进程同步 DB          | 全部 database.* 同步执行，"async 壳、同步内核"；sql.js 全库 export 是主要阻塞源                                              |

## 2. 目标架构

```
E:\CrucibleBox_Sourses            ← 唯一 canonical monorepo（npm workspaces）
├── packages/
│   ├── openbox-rpc/              ← RPC 共享内核（Envelope/RequestTracker/Timeout/PayloadBudget/ErrorCodec/SessionRegistry）
│   └── cruciblebox-plugin-api/       ← 插件 API 类型唯一事实源（替代 6 份 openbox-api.d.ts）
├── shared/                       ← 双 RPC 能力注册表 + ipc.types + themes + trusted policy
├── plugin-system/
│   ├── PluginManager.ts          ← 瘦身为纯运行时编排（per-plugin runtime record）
│   ├── runtime/                  ← PluginRuntimeRecord + capability 分发表 + PluginLogService
│   ├── trusted-services/unienv/  ← UniEnv 高信任模型物理隔离目录
│   └── install/                  ← 安装事务族保持，收敛为显式状态机
├── database/                     ← 生产仅 better-sqlite3；sql.js 降级为测试/恢复工具
├── scripts/
│   ├── release-manifest          ← 单一发布清单（构建时生成，不入库）
│   └── update-trusted-policy.mjs ← digest 生成脚本（消灭手工重钉）
└── docs/
    ├── architecture.md / security-model.md / plugin-sdk.md / install-recovery.md / release-runbook.md / development.md
    └── history/                  ← 历史归档（plugin-platform-m2.*、theme-release-1.5.0、gif-editor-m1.1、unienv-m1.2、历史 AGENTS）
```

## 3. 文档体系收缩（1.5.24）

保留为当前规范（6 份）：

| 规范文件                   | 来源 / 说明                                                             |
| -------------------------- | ----------------------------------------------------------------------- |
| `docs/architecture.md`     | 已有，更新基线                                                          |
| `docs/security-model.md`   | 从 trusted-release.md + AGENTS 安全模型抽取（新建）                     |
| `docs/plugin-sdk.md`       | 从 plugin-sdk-migration.md + 模板 + cruciblebox-plugin-api 提炼（新建） |
| `docs/install-recovery.md` | 从安装事务族 + ADR 提炼，含显式状态机定义（新建）                       |
| `docs/release-runbook.md`  | 从 trusted-release.md + delivery-package.md + release.yml 提炼（新建）  |
| `docs/development.md`      | 已有，更新                                                              |

归档至 `docs/history/`：`plugin-platform-m2.1~m2.13`（13 份）、`theme-release-1.5.0.md`、`gif-editor-m1.1.md`、`unienv-m1.2.md`、历史 AGENTS.md 全文。
AGENTS.md 压缩为"当前状态"单页（保留文件地图 + 安全模型 + 验证命令）。

## 4. 版本路线图

版本线语义：**1.5.X = 治理与低风险收敛；1.6.X = 运行时简化；1.7.X = 兼容性清理 + SDK v2 冻结。**

**拆分原则**（决定各小版本怎么切）：

1. 一个 minor 版本 = 一个完整工作包，独立走固定检查单（check → build → release:local → release:validate → tag）。
2. 护栏 #6 约束：升级 Electron、重构 RPC、删除 sql.js 三件大事**不得同版执行**——因此 1.6.0 不一次性合并"sql.js 降级 + RPC 合并 + UniEnv 隔离"三项，必须拆开。
3. minor（1.6.X）允许内部破坏性变更，但不得改插件 API v2 与 DB schema 语义；patch（第三位）只允许 bugfix/清理/文档，零破坏性。
4. 每版"可独立回退"：前一个 tag 始终可运行，随时停发。

### 1.5.24 — 治理与发布链收敛（零运行时风险）

- 固定 canonical monorepo；`E:\CrucibleBox_Plugins` 冻结为只读备份
- 新增 `npm run clean:all`（只清 out/dist/release/artifacts/node_modules，不动用户数据）
- 文档收缩（见 §3）
- 统一发布入口：`npm run release:local` + `npm run release:validate`；`release.yml` 改调统一链路
- 构建时生成单一 `release-manifest.json`（应用版本、6 插件版本、ZIP 清单、SHA-256、签名 key ID、SBOM、安装器摘要、latest.yml、attestation subject），校验脚本改读 manifest
- 增加 clean checkout CI 门禁（干净树检查、文本无 CR、unienv digest 差异报告）

出口验证：`npm run check` + `npm run build` + `release:validate` + 一次 CI 全绿。无运行时代码行为变化。

### 1.5.25 — 实现层合并（类型 + 状态）

- 共享插件 API 类型：新增 `packages/cruciblebox-plugin-api/`，模板 + 6 插件改引用，删除复制 d.ts
- digest 生成脚本化：`scripts/update-trusted-policy.mjs`（unienv 重建后自动重钉）
- PluginManager 状态记录：`plugin-system/runtime/PluginRuntimeRecord.ts`，9 张 Map 合并，capability 分发表
- 安装事务收敛为显式状态机：`prepared → awaiting-confirmation → staged → stopping-old → applied → committed / recovery-required`；`PluginInstallPreparation/InstallationService/DirectoryTransaction/TransactionRecovery` 职责对齐

出口验证：全量测试（254/190/16）+ 生命周期专项 + dev 冒烟（6 插件激活/切换/卸载无 EBUSY）。

### 1.6.X — 运行时简化线（逐 minor 拆分）

> 1.5.25 已含：cruciblebox-plugin-api、digest 脚本、PluginManager runtime record、安装事务状态机。1.6.X 不重复。

| 版本      | 主题                               | 内容（文件级）                                                                                                                                                                                                                                                                                                                        | 出口验证                                                                                                 |
| --------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| **1.6.0** | 数据库运行时收敛（单引擎）         | `database/index.ts`：删除 `OPENBOX_DB_ENGINE` 生产切换，生产只走 BetterEngine；SqlJsEngine 保留实现但不再可被生产选择；`package.json` 将 sql.js 移入 devDependencies；新增 `scripts/db-recovery-tool.mjs`（用 sql.js 离线读/修 openbox.db）；保留迁移、启动备份、失败抛错路径；`electron-builder.yml` 确认 sql-wasm 不再进安装包      | 全门禁 + `smoke-packaged` 旧库迁移 + `release-compatibility`（previous→candidate 回滚）+ 单引擎 dev 冒烟 |
| **1.6.1** | RPC 公共基础层                     | 新增 `packages/openbox-rpc/`（RpcEnvelope / RequestTracker / TimeoutManager / PayloadBudget / ErrorCodec / SessionRegistry）；`shared/plugin-backend-rpc.ts`、`shared/plugin-renderer-rpc.ts` 改为"内核 + 各自 capability 注册表"；`src/plugin-runtime/frame-entry.ts` 第三套 pending Map 改用内核追踪器；线上信封 v2/v1 字节格式不变 | 新增"新旧构造字节级等价"测试 + RPC 专项测试（backend/renderer/frame-bridge）+ 全门禁 + 6 插件激活冒烟    |
| **1.6.2** | UniEnv 物理隔离 + trusted 目录整理 | 迁移至 `plugin-system/trusted-services/unienv/`（宿主固定加载，不依赖插件源码相对路径）；`TrustedServiceRuntime` 收敛到稳定内部模块边界；`update-trusted-policy.mjs` 最终定型并挂 CI 门禁；固定文件集/摘要/fail-closed 语义零变化                                                                                                     | trustedServiceRuntime 专项测试 + unienv 插件测试 + VM 冒烟（安装/切换/回滚）                             |
| **1.6.3** | 稳定性收尾（按需发布）             | 1.6.0–1.6.2 暴露的回归修复；性能预算复测（`verify:performance`）；测试套件按新内部结构重构对齐；无修复需求则跳过，不空发                                                                                                                                                                                                              | 全门禁 + 完整 CI                                                                                         |

节奏：1.6.0 ≈ 1–2 周末，1.6.1 / 1.6.2 各 ≈ 1 周末，1.6.3 视回归情况。

### 1.7.X — 兼容性清理 + SDK v2 冻结线（逐 minor 拆分）

| 版本       | 主题                             | 内容（文件级）                                                                                                                                                                                                                                                                                                                                                                  | 出口验证                                             |
| ---------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| **1.7.0**  | 删除层（production removal）     | ① 删除旧 backend/renderer **fallback**（`OPENBOX_PLUGIN_PROCESS=0` 进程内回退、旧 raw SQL 新装路径）——只删回退，不删 utility process 主路径与沙箱；② 删除生产 sql.js（SqlJsEngine、WASM 加载路径；**`shared/types/sql.js.d.ts` 保留作测试类型声明**——sql.js 无自带 types，测试仍 import 它，删除会破坏 typecheck:test）；③ 删除 `E:\CrucibleBox_Plugins` 镜像（确认已冻结备份） | grep 无引用证明 + 全门禁 + installer smoke + VM 冒烟 |
| **1.7.1**  | SDK v2 冻结 + theme-manager 测试 | `docs/plugin-sdk.md` 正式声明 SDK v2 冻结（此后 API 变更走 v3 提案，经 `packages/cruciblebox-plugin-api` 版本化）；新增 CI 门禁（6 插件对冻结 d.ts 构建兼容矩阵）；`plugins/theme-manager/tests/`（manifest 契约 + 渲染入口最小测试）；theme-manager 补 vitest 依赖                                                                                                             | 全门禁 + 插件 190 项（现 189+1）                     |
| **1.7.2**  | 代码库可维护性                   | 拆分 `src/styles/global.css`(1001 行) 按主题/基础/特效分文件；拆分超大测试文件（`pluginInstallTransaction.test.ts` 873 行等）；确认 PluginManager <600 行、renderer-rpc 缩容达标；docs/ 终稿（AGENTS 单页、6 份活文档、history 归档核对）                                                                                                                                       | 全门禁 + format/lint 全绿                            |
| **1.7.3+** | 维护线（按需，不空发）           | bugfix、`npm audit` 依赖更新、安全补丁（含 Electron 小版本升级，走独立版本，不与其他重构混发）；无内容则跳过                                                                                                                                                                                                                                                                    | 全门禁 + 相关 smoke                                  |

节奏：1.7.0 ≈ 1–2 周末，1.7.1 ≈ 半天–1 周末，1.7.2 ≈ 1 周末，1.7.3+ 按需。

## 5. 版本管理方案

- **Patch（1.5.24 → 1.5.25）**：单工作包，无破坏性变更（治理、文档、内部重构）
- **Minor（1.5.X → 1.6.0）**：允许内部破坏性变更（DB 运行时切换、RPC 内核抽取），不允许改插件 API v2 / DB schema 语义
- **Major 语义线（1.7.0）**：兼容性冻结/大清理；删除生产 sql.js、fallback、SDK v2 冻结
- 每个版本固定检查单：`check` → `build` → `release:local` → `release:validate` → commit → tag vX.Y.Z → CI 权威重建发布
- 单 `main` 分支，始终可发布；不打补丁分支；线上事故在 main 修复后走下一 Patch
- 插件版本独立演进，由 release-manifest 绑定每版组合
- DB schema v3 冻结；变更 = 强制 minor + 必须通过 smoke-packaged 与 release-compatibility 双验证

## 6. 依赖锁与调整说明（相对原始建议的修正）

1. **文档目标清单**：security-model/plugin-sdk/install-recovery/release-runbook 四文件当前不存在，需按 §3 映射新建后再归档旧文档。
2. **共享 API 类型依赖锁**：抽包会触发 6 插件 dist 重建 → unienv digest 变化 → 必须先有 digest 脚本（update-trusted-policy.mjs）。故 cruciblebox-plugin-api 与 digest 脚本化同批（1.5.25），不放入 1.5.24。
3. **PluginManager 状态拆分**：回归风险最高（生命周期核心），移入 1.5.25 与 RPC 基础层同批，由现有 702 行 lifecycle 测试 + 254 项宿主测试兜底。
4. **release-manifest 存储策略**：构建时生成、不入库（避免手工漂移），发布时作为 release 附件上传；本地与 CI 各自生成后 canonical 对比校验。

## 7. 分支与提交方案

### 7.1 模型

**Trunk-based + 短生命周期工作分支 + PR 合入 + tag 发布。**

```
main（唯一长期分支，始终可发布）
  │
  ├── feat/1.5.24-release-entry       每个工作包 = 一个短分支
  ├── chore/1.5.24-archive-docs
  ├── refactor/1.5.25-runtime-record
  ├── fix/1.5.24-clean-checkout
  └── hotfix/1.5.24-crash-recovery    紧急修复同规则
       │
       └── 全部经 PR squash merge 回 main
            │
            └── 版本发布：git tag v1.5.24 → push tag → release.yml 自动发布
```

- 不建 develop、release-*、长生命周期版本分支——单人维护下维护成本高于收益。
- 版本用 annotated tag 表达；`release-compatibility.yml` 已覆盖跨版本验证。

### 7.2 分支命名规范

| 分支      | 命名                       | 示例                             |
| --------- | -------------------------- | -------------------------------- |
| 功能      | `feat/<版本>-<短描述>`     | `feat/1.5.24-release-entry`      |
| 重构      | `refactor/<版本>-<短描述>` | `refactor/1.5.25-runtime-record` |
| 修复      | `fix/<版本>-<短描述>`      | `fix/1.5.24-clean-checkout`      |
| 文档/工程 | `chore/<版本>-<短描述>`    | `chore/1.5.24-archive-docs`      |
| 紧急      | `hotfix/<版本>-<短描述>`   | `hotfix/1.5.24-crash-recovery`   |

### 7.3 每次开发的标准工作流

```bash
# 1. 从最新 main 检出工作分支（禁止本地攒活）
git fetch origin
git checkout -b feat/1.5.24-release-entry origin/main

# 2. 开发 → 全量门禁通过才允许提交
npm run check        # format + lint + typecheck + test(254/190/16)
npm run build        # 生产构建
npm run release:validate   # 涉及发布链的改动

# 3. Conventional Commits 提交（与现有提交风格一致）
git add <files>
git commit -m "feat(1.5.24): add unified release entry"

# 4. 推送到 GitHub 并开 PR（CI 自动跑 ci.yml）
git push -u origin feat/1.5.24-release-entry
gh pr create --title "feat(1.5.24): add unified release entry" --body "..."

# 5. CI 全绿 → squash merge → 删除远程分支
gh pr merge --squash --delete-branch
```

要点：一个工作包 = 一个分支 = 一个 PR；一个版本 = 多个 PR 累积；不允许在本地攒多个未推的工作包。

### 7.4 版本发布工作流

```bash
npm run check && npm run build
npm run release:local       # 本地打包+签名+SBOM+安装器
npm run release:validate    # 全部 smoke + manifest + checksum
git tag -a v1.5.24 -m "CrucibleBox 1.5.24"
git push origin main
git push origin v1.5.24     # 触发 release.yml → 权威重建 + attest + 发布
```

### 7.5 紧急修复规则

- 从**最近一个可发布 tag** 检出 `hotfix/...`，修复后 PR 合入 main，作为**下一 patch 版本**发布。
- 不改已发布 tag；紧急场景也走 PR+CI，只是不攒批、单点直发。

### 7.6 GitHub 端设置（已确认执行）

1. 单一 remote：已删除 `CruciBox`，仅保留 `origin`（指向 `https://github.com/Xuancheng75/CrucibleBox.git`）。
2. main 分支保护：要求 PR（默认关闭直接 push）、要求 status checks 通过、禁用 force push；单人可不强制 review，但 status checks 门禁保留。

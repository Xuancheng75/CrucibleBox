# 架构优化复验（2026-10-09）

## 结论

**主要结构性缺口已修复，但仍不通过“原计划全部完成”的总体验收。**

与 10 月 8 日相比，A1 文档进程隔离、A2 契约冻结和旧执行入口退役、A3 数据访问封装可以关闭对应源码缺口。A5 的独立运行时安装机制也已通过本次本机复验。剩余工作集中在默认测试/CI 接线、一次未定位的归档测试失败，以及原生 UI、安装交付和整应用性能验收。

范围：当前脏工作区（HEAD `5e22173`），对照原方案及昨日 A1–A6。保留七官方插件迁移范围；不要求重新接入或迁移其余旧插件。本次只新增验收记录，未修改实现、清理工作区、提交或发布。测试使用临时数据，没有操作生产用户库。

## A1–A6 逐项结论

| 项目               | 复验结论                         | 证据与剩余边界                                                                                                                                                                                       |
| ------------------ | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A1 文档处理出进程  | 原源码缺口关闭                   | `crates/document`、`document-native`、`workers/document` 已分离；宿主正常依赖树不含 PDFium/document-native；真实子进程解析、崩溃、超时、取消及后续请求测试通过。尚不等于 WebView2 宿主完整业务验收。 |
| A2 契约与旧入口    | 原源码缺口关闭                   | Manifest/API 5、wire 3、data 1 frozen；SDK 5.0.0-beta.1、CLI 1.0.0-beta.1。生产宿主装配 retired_backend，旧会话和激活拒绝，Next 不含 SQL RPC；旧 fixture/源文件保留不等于仍可执行。                  |
| A3 repository 边界 | 原源码缺口关闭                   | Connection 访问私有化，fixture_connection 仅测试 feature；宿主设置、插件、任务、存储使用 repository 用例接口。锁等待收益仍属 A6 实测范围。                                                           |
| A4 业务/恢复矩阵   | 部分完成                         | 新增文档、UniEnv、归档服务级业务及恢复证据；本次验证文档发布和归档定向流程。仍缺完整 WebView2 交互、真实历史数据消费、安装升级/配对回滚 UI、真实用户 PATH 刷新。归档全量测试另有一次失败，见下文。   |
| A5 按需 runtime    | 实现及本机安装通过，交付验收未完 | 当前固定目录匹配 catalog；取消测试专用 worker 覆盖后，独立安装、PDFium 可用性、解析和服务重启后发现均通过。正式 MSVC/NSIS 安装升级路径未复验。                                                       |
| A6 性能与证据      | 部分完成                         | 有同解析器、同合成十页 PDF 的 45 次直接/worker 对照；没有整应用启动、插件打开、交互延迟、进程树内存、DB 锁等待和 2000 页压力的完整对照。新增 CI 上传尚无远端执行结果。                               |

## 本次发现的阻塞项

### R1：默认宿主测试及两条流水线缺少文档 worker 前置配置

未设置测试专用 worker 环境时，`cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked` 返回 101：225 passed、2 failed、11 ignored。失败为 `parse_pdf_task_returns_unified_document` 与 `convert_task_writes_markdown_output`。

`Service::new()` 使用 `with_worker(..., None)`，未安装 runtime 时客户端指向 unavailable 路径。两个默认执行的测试却要求文档任务成功。设置 `DOCUMENT_WORKER_ACCEPTANCE_EXE` 和 `DOCUMENT_WORKER_ACCEPTANCE_PDFIUM` 后，这两项通过。

`.github/workflows/ci.yml:181` 和 `tauri-release.yml:166` 的默认 Cargo test 步骤没有配置上述前置条件；更后的独立 publication 步骤才局部设置 exe，不能影响前面的步骤。仅构建 runtime 包也不等于安装到 Service 使用的目录。因此这是可复现的本地门禁问题和可预见的干净 CI 阻塞，不能写成“只是远端还没运行”。

最小修复：让默认宿主测试显式取得已构建 runtime，或将依赖外部 runtime 的测试划为明确的集成测试并在两条流水线显式执行；同步本地验证说明。退出条件是干净临时运行时环境下按标准入口验证成功，不依赖个人机器残留。

### R2：配置 runtime 后，全量宿主测试出现一次归档结果缺失

第二轮宿主测试返回 101：226 passed、1 failed、11 ignored。`archive_service::tests::creates_zip_from_files_and_reads_it_back` 在 `src-tauri/src/archive_service.rs:1407` 读取第二次压缩结果的 `destination` 时 unwrap(None)；上一行已断言 status 为 succeeded。

随后独立运行全部 `archive_service::tests::`，8 项通过。当前证据表明失败不稳定，**尚不能确定是结果投影时序、测试隔离还是其他原因，也不能断言归档业务稳定损坏**。但定向重跑通过不能删除全量失败记录或据此宣布全门禁通过。应核对终态与结果发布的可见性，再复验全量并行测试。

## 本次执行结果

原始日志目录：`E:/OCR/architecture-reaudit-20261009/`。

| 检查                                                     | 结果                                                     |
| -------------------------------------------------------- | -------------------------------------------------------- |
| `npm run check`                                          | exit 0；含宿主前端 145、Next SDK 145、CLI 7 及既有检查   |
| document worker，`--features acceptance-faults --locked` | exit 0；2 单元 + 3 原生集成通过；1 性能测试默认忽略      |
| repository 独立测试                                      | exit 0；18 passed、1 ignored                             |
| 宿主 workspace，未配置 worker                            | exit 101；225 passed、2 failed、11 ignored，见 R1        |
| 宿主 workspace，配置固定 worker/PDFium                   | exit 101；226 passed、1 failed、11 ignored，见 R2        |
| 独立 runtime 安装/重启，移除 worker 测试覆盖             | exit 0；1 passed；日志 `runtime-install-no-override.log` |
| 文档持久产物/取消/失败/重启集成                          | exit 0；1 passed；日志 `document-publication.log`        |
| 归档服务定向复验                                         | exit 0；8 passed；日志 `archive-retest.log`              |

运行时包采用 `E:/OCR/next-document-runtime-package-20261009/0.1.0`；其 worker 为当前 catalog 指定的 22,372,256 bytes / `ae6f8af37227735a1a4f56ad279a02099161811745f226f4717c089c2c18f66b`。名称含 local/pinned-gnu/check 的其他目录不是当前 catalog 对应制品，不能混用。

已有性能记录 `E:/OCR/next-document-worker-performance-20261009.json`：直接解析中位数 33.4022 ms，worker 80.0517 ms，额外 46.6495 ms。它量化了这一夹具的隔离成本，不证明整应用收益，也不能直接套用整应用 10% 建议阈值判失败。本次未重复跑该微基准。

本次未重跑所有 Rust fmt/clippy、sidecar、前端生产构建、七插件独立构建或原生 UI/NSIS；此前交付结果不冒充本次执行。未运行远端 Actions。OCR 精度门禁仍独立，不因架构隔离而自动通过。

## 下一验收顺序

1. 修复 R1 默认测试与两条流水线的 runtime 接线，定位 R2，再跑宿主全量门禁。
2. 用实际 MSVC 安装/便携产物完成文档、UniEnv、归档三条原生纵向流程，以及历史数据、升级和配对回滚验证。
3. 补原方案要求的整应用受控性能与长文档资源证据；微基准和 CI 上传分别验收。

当前适合认定为“主要架构改造已落地，仍有门禁缺陷及整体验收未闭环”，不适合标为 S0–S6/L0–L6 全部完成。

## beta4 修复复验（2026-10-09）

R1 已修复：两个需要独立文档 runtime 的测试标记为显式集成测试，CI 与发布流水线先安装固定 runtime，再执行它们和持久产物集成。未配置 runtime 的标准 MSVC 宿主 workspace 实测 225 passed、13 ignored；忽略项不计作通过。

R2 已修复：任务读取使用既有发布锁，终态与结果投影一起对读取者可见。增加终态读取等待结果发布的并发回归测试；宿主全量并行复验通过，失败日志保留在 E:/OCR。

文档 worker 的 MSVC 独立 runtime 已打包并验证安装与重启。实际 PDFium 的 2000 页文本 PDF 测试通过，最后一页内容和任务临时产物释放得到验证。45 次受控微基准及原始结果保存在 E:/OCR/beta4-document-worker-performance.json；这不是整应用性能对照，也不代表 OCR 精度达标。

七个 Next 官方插件独立源码构建与 ZIP 校验通过。实际 WebView2 已验证主题管理挂载及主题预览；Windows 原生通信显式附带来源信息，宿主校验仍保留。用户生产目录中的旧插件不能由更新宿主自动变成 Next：必须通过同一版本的七插件目录下载安装升级。旧架构插件保留包和用户数据，禁止启动旧运行时。

最新修复还包括侧栏版本位置、主题焦点、长按拖动目录排序、原生标题栏主题同步，以及 iframe 底层背景随主题更新。发布前须使用包含这些修改的最终原生构建复验。历史验收日志不得当作此次新构建的证明。

本记录只补充已执行检查；三条全业务原生纵向流程、真实用户 PATH 生效、历史生产数据配对回滚和整应用资源性能仍须以实际证据收尾，不能由微基准或挂载截图替代。远端签名 Actions 和 GitHub release 未通过之前，不得写已发布。

### 干净远端环境补充复验

首轮 Actions 因根锁文件遗漏内置 Next SDK 工作区记录而失败；已补齐，随后本地真实 npm ci、npm check、生产构建和七插件包校验通过。第二轮源码检查暴露文档插件的 vendor/plugin-ui 只有本地 dist 而没有完整源文件；已补齐 UI 源码与构建步骤，独立导出排除所有 dist。七插件从此纯源码导出完整验证通过，目录 E:/OCR 日志 beta4-source-only-independent.log；可信服务摘要保持不变。

第二轮 Rust Actions 已通过文档 worker 构建、隔离与两个显式性能测试，失败发生于上传路径：Cargo 测试的工作目录使相对路径写入子项目。已把性能证据输出改为 GITHUB_WORKSPACE 下的绝对路径，上传文件存在性仍为硬门禁。旧失败日志保留，修复后的完整远端结果须另行确认。

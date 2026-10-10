# 文档与知识库：当前状态、能力边界与后续路线

> 架构状态：Next beta.1 使用独立文档 worker，PDFium 与 Document IR 不在宿主进程内。以下 OCR/公式记录是独立能力的实测状态，不等同于 Next 架构交付或 beta.3 全计划完成。
> OCR 准确率和深度公式门禁仍按其历史样本分别验收；本次架构隔离不改变这些结果。

## 1. OCR 准确率与 Next 架构的边界

OCR/公式识别仍需要以固定人工标注样本测量；本文件中 beta.3 的识别数据仅描述被测能力，不构成 Next 架构或正式发布完成声明。

文档进程边界、按需运行时安装、host publication 与恢复的权威实现说明见 docs/architecture.md 和 docs/plugin-sdk.md。

## 2. 产品定位与不承诺事项

Document Engine 是 CrucibleBox 的本地文档处理插件，面向两类输出：

1. 解析型输出：服务 AI、RAG、搜索和结构化处理，优先内容、结构和可追踪元数据。
2. 阅读型转换：服务人类阅读和编辑，尽量利用原始布局、图片、公式、表格和页面尺寸重建 DOCX、HTML、PDF 或面向人的 Markdown。

它不是通用 OCR 训练平台，不执行用户文档训练，也不把桌面上的回归 PDF 用于训练或针对单一文件写特例。对低质量扫描件、复杂字体、严重倾斜、复杂表格、手写内容和数学公式，输出准确率不作绝对保证。

## 3. 当前端到端管线

```text
原始 PDF / 图片 / Office / 文本
              |
独立 document-worker：PDFium 与输入探测
              |
  原生文字层优先（普通文字来源）
  扫描页渲染 + Layout/OCR（普通文字来源）
              |
  区域分类：text / heading / formula / table / image /
            caption / header / footer / page_number
              |
  Unicode/XML-safe 清洗、断词修复、阅读顺序、结构树
              |
          Normalized Document IR v3
          /                         \
 AI/RAG：JSON/MD/TXT/JSONL/Chunk     阅读型：DOCX/HTML/PDF
```

原生 Text Layer 只决定普通文字是否优先使用 native text；它不能短路标题、公式、表格、图片和布局分析。转换器和 Chunker 只消费 IR，不应再次加载 OCR 模型。PDF 物理切分直接复制原始页并输出真实 PDF，不是把 JSON 改名为 PDF。

## 4. 实现组成

| 位置                                                              | 职责                                                      |
| ----------------------------------------------------------------- | --------------------------------------------------------- |
| `plugins/document-engine/src/main.ts`、`engine-api.ts`            | 插件入口、能力声明和宿主调用契约                          |
| `plugins/document-engine/src/renderer.tsx`、`renderer-entry.tsx`  | PDF 解析、转换、切分、批处理、模型和任务界面              |
| `src-tauri/src/document_engine_service.rs`                        | trusted service 入口、权限检查和任务编排                  |
| workers/document、src-tauri/crates/document-native | PDFium worker runtime、解析、IR、输出验证与媒体特性 |
| src-tauri/crates/document、document-native | Layout/IR、结构、文本、质量与公式解析 |
| `document_layout.rs`、`document_structure.rs`、`document_text.rs` | 布局区域、阅读顺序、文本清洗和章节结构                    |
| `document_math.rs`、`document_quality.rs`                         | 公式块、质量指标和 RAG 质量门控                           |
| `document_chunker.rs`                                             | Structure/Semantic/Hybrid/Pages/Chapters 分块             |
| src-tauri/src/document_converter.rs | 宿主适配器通过 worker 协议转换并发布耐久输出 |
| `src-tauri/src/ocr_worker.rs`、`ocr-worker/`                      | Rust JSONL worker、ONNX Runtime、PaddleOCR 推理和模型校验 |
| `docs/document-engine-worker-protocol.md`                         | worker 协议、请求、结果和错误码                           |

插件 renderer 只通过 trusted service 请求宿主处理。宿主将 PDF/文档操作交给独立 document-worker；该进程加载 PDFium 并运行 Document IR 解析，再由宿主校验 worker 输出并经统一任务 runtime 发布。PDFium native abort、超时和取消均隔离在 worker 进程；OCR inference worker、模型准确率和深度公式门禁仍单独验收。

## 5. 输入与 OCR 规则

| 输入                       | 当前策略                                                                     |
| -------------------------- | ---------------------------------------------------------------------------- |
| 有可靠文字层的 PDF         | PDFium 提取原生文字，保留页面、bbox 和布局信息；普通正文优先 native          |
| 扫描 PDF                   | PDFium 渲染页面，det/rec worker 识别普通文字；无 native block 时按页继续处理 |
| 图片                       | 进入同一 OCR worker 和 Document IR，不另设一套输出格式                       |
| DOCX/PPTX/XLSX/TXT/MD/HTML | 解析为统一 IR，再按目标输出；不因输入格式绕过质量检查                        |

当前发布 profile 为 `ppocrv6-small-det-v5-mobile-rec`，模型目录默认位于 `%APPDATA%\\cruciblebox\\document-engine\\models`；保留 `ppocrv4-mobile-zh-en` 兼容 profile。OCR worker 支持 CPU，并可按打包环境使用 DirectML。模型、字典、worker 和 PDFium 都必须存在并通过 SHA-256 校验；模型缺失不能静默伪装成成功。

缓存身份必须至少包含：源 PDF hash、OCR engine、det 版本、rec 版本、字典/语言身份和 OCR 配置版本。缓存命中不能只依据文件名。模型下载、删除和重装属于模型管理，不会把用户文档用于训练。

## 6. Document IR 与结构

IR 以 `Document → Page → Block → Chunk` 组织。Block 至少覆盖 `text`、`heading`、`list`、`table`、`image`、`formula`、`caption`、`header`、`footer`、`page_number` 等类型，并保留 `page`、`bbox`、置信度、来源和可追踪的原始信息。Chunk 保留 `document_id`、页范围、`section_id`、`section_path`、heading、source file、语言、OCR 质量和 formula/table/image 标记。

章节识别区分 chapter、section、subsection、编号段落/列表项和 exercise；正文块必须得到有效的 `parent_id`、`section_id`、`section_path`。TOC 记录为 `region=toc`、`type=toc_entry`，只能辅助候选章节，不能直接压入正文 heading stack。页眉、页脚和页码保留在 IR，但默认不进入正文 Chunk。

公式块保留 `latex`、`plainText`、`bbox`、`page`、`confidence`、`source` 及引擎信息；当前实现仍存在复杂矩阵、上下标、二维排列和普通短文本误判边界。表格和图片至少保留区域与元数据，不因存在 Text Layer 就假设它们不存在。

## 7. Chunk、质量和导出

Hybrid Chunk 的现有长度策略是稳定基线：目标约 450–500 tokens，当前配置约为 `target=512/min=180/max=800`。冻结期间不为追求数量重新设计长度算法；只允许在严重数据损坏或回归时修复结构元数据、原子公式块和非法字符。

质量分为 DocumentQuality 和 ChunkQuality。`invalidControlChars`、`invalidXmlChars` 应为 0；低 OCR 置信度、混合脚本噪声、疑似乱码、缺少结构、公式检测不确定等应进入 quality flags。文档质量差时，单个 Chunk 仍需独立判断，不能无条件设置 `ragEligible=true`。无标题扫描文档应按页或语义页组回退，不能把十页拼成一个 Chunk。

解析型 JSON/MD/TXT/JSONL 追求机器可处理；DOCX/HTML/PDF 转换追求版面和可打开性。DOCX 的最低门槛是 XML parts 可解析、非法 XML 字符为 0，并尽可能使用 Heading、List、Table、Image、Caption、OMML、Page Break、Header/Footer 和 Page Number。视觉相似度不是当前冻结版的准确率承诺。

## 8. 已完成的回归基线

以下文件只作为测试 fixture，不参与训练：

- `C:\Users\\hjc\\Desktop\\fogharbor_botanical_field_notes_scanned.pdf`
- `C:\Users\\hjc\\Desktop\\linear algebra by strang 4 th edition.pdf`

| 项目                      |     Fogharbor 扫描 PDF |          Linear Algebra 原生文字 PDF |
| ------------------------- | ---------------------: | -----------------------------------: |
| 版本                      | Document Engine 0.10.0 |               Document Engine 0.10.0 |
| 页数                      |                     10 |                                  542 |
| native / OCR blocks       |                0 / 214 |                            61167 / 0 |
| headings                  |                     28 |                                   47 |
| formula blocks            |                      0 |                    5954（候选 6010） |
| chunks                    |                      5 |                                  549 |
| 平均 tokens               |                      — |                               512.08 |
| invalid control/XML chars |                  0 / 0 |                                0 / 0 |
| 质量结论                  |                   good | passed，但公式检测仍有 degraded 标记 |

Linear Algebra 回归已覆盖 `Ax=b`、`Ax=λx`、`A^{-1}`、矩阵等样本，矩阵块计数 56；仍有约 467 个可疑公式候选，说明数学结构恢复不是“已完全解决”。扫描件回归证明中文/英文 OCR、标题和按页分块链路可运行，但不代表所有中文扫描件均达到出版级准确率。

### 2026-09-26 beta3 本地实测

- `linear algebra by strang 4 th edition.pdf`（桌面，542 页）：`parses_large_fixture_without_forcing_ocr` 实际运行通过。541 页原生文字质量良好，未强制送 OCR；提取 61,167 个原生文字块、56 个矩阵块，形成 549 个 Chunk；Markdown 1,232,653 字节、DOCX 622,606 字节，DOCX XML 校验通过。仍有 467 个可疑公式候选，RAG 文档质量标记为 `degraded`。
- `fogharbor_botanical_field_notes_scanned.pdf`（桌面，10 页）：`parses_scanned_pdf_through_full_ocr_pipeline` 用 `E:\OCR\Models` 和重新编译的打包 OCR worker 实际运行通过。识别 214 个 OCR 文字块、28 个标题，形成 5 个 Chunk，质量标记为 `good`。
- 初次扫描测试失败是因为 `src-tauri/binaries` 中的 OCR worker 仍为 2026-09-03 旧构建，错误为 `model manifest SHA-256 does not match installed files`。源码中的按模型 profile 区分清单逻辑已存在；重新构建 release worker 并同步两个 Tauri 目标文件后，同一测试通过。旧二进制保存在 `.beta3-baseline-20260925/ocr-worker-before-beta3.exe`。
- 另以人工复核的 ClinOCR-Bench v1.0 英语扫描样本构造三个无文字层单页 PDF 并运行同一宿主管线；原轻量 OCR 的清晰、劣质和表格样本 CER 分别为 0.838690、0.825093、0.445844。集成 RapidOCR 正文识别与低内存 ONNX 公式识别，并修正 IR 重复排序后，三者 CER 分别为 0.084672、0.660692、0.010076，WER 分别为 0.095238、0.722944、0.011765。劣质页仍明显漏识别。详细来源、归一化规则和局限见 `docs/history/2.1.0-beta.3-local-acceptance.md`。安装包中的用户界面操作仍需单独验收。

## 9. 已知限制与风险

1. OCR 准确率受模型、分辨率、倾斜、字体、语言混排和页面噪声共同限制；当前没有继续训练或替换模型的维护能力。
2. 公式检测/识别仍可能把复杂二维数学拆碎，或把少量短文本当作公式；矩阵、上下标和跨行公式是高风险区域。2026-09-26 的 OHR-Bench 人工校验扫描 PDF：原始轻量文字 OCR 对 9 条真值公式生成 0 块；新增低内存 ONNX 方案均生成候选，但逐式去空白 LaTeX 严格匹配仅 2/9，高精度 L 备选在第一页也仅 2/5。两份单页任务的 OCR 进程树峰值约 1.14GB/1.15GB，十页重复压力样本约 1.21GB；高精度 L 备选第一页峰值 3.46GB，需主动切换且明确提示超限。详见 `history/2.1.0-beta.3-local-acceptance.md`；数学内容仍需人工复核。
3. 章节树依赖布局、bbox、上下文和 OCR 质量；目录、列表、习题编号及页眉页脚仍需抽检。
4. DOCX/HTML/PDF 是结构化重建，不是 PDF 的逐像素复制；复杂字体、浮动对象、表格和图注可能有差异。
5. PDFium 已从宿主进程移至 document-worker；native abort、超时、取消及后续请求恢复已有进程测试。运行时缺文件或摘要不匹配会失败关闭；目标工具链生成的 worker 摘要须与独立安装包和静态 catalog 同次更新。此隔离不替代 OCR 模型、长文档资源和准确率验收。
6. 长文档最多约 2000 页的目标需要持续压力测试；内存峰值与公式/图片数量有关，不能以单页测试推断全量稳定性。
7. 模型和运行时制品必须固定版本、许可证和 SHA-256；来源不可验证时不得把结果当作发布门禁通过。

## 10. 暂停中的未来调整路线（非当前承诺）

若未来重新获得合适人员和工具，应按以下顺序恢复，禁止一次性重写：

| 阶段 | 目标                                                                                        | 主要验收                                                              |
| ---- | ------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| R1   | 建立多语言 OCR 基准，评估通用 PP-OCR、中文/混排 profile、许可证和本地部署                   | 固定数据集、字符级/行级指标、模型 hash 和可重复报告                   |
| R2   | LayoutDetector 与普通文字 OCR 解耦                                                          | text/heading/formula/table/image/header/footer 区域可追踪，失败可回退 |
| R3   | FormulaDetector 与 FormulaRecognizer 解耦，优先验证 PP-DocLayout-M 与 PP-FormulaNet profile | 页码、时间、标题、URL 不进公式；矩阵/上下标二维结构通过样本验收       |
| R4   | 恢复结构树、阅读顺序和 TOC 隔离                                                             | 随机抽检至少 20–50 个 section 的父子关系，而非只看 sectionPathRatio   |
| R5   | 完善 OCR 噪声过滤和质量门控                                                                 | 低质量 Chunk 不进入 RAG；文档与 Chunk 两级状态一致                    |
| R6   | 完善公式 Markdown/OMML 与布局型 DOCX/HTML/PDF                                               | XML parse、Word/LibreOffice 打开、DOCX→PDF 渲染和视觉抽检全通过       |
| R7   | 压力、内存和长任务恢复                                                                      | 10/100/500/2000 页、断点、取消、重启、输出目录和峰值内存有数据        |
| R8   | 模型注册表与发布门禁                                                                        | 版本、后端、语言、大小、下载地址、SHA-256、安装/删除/重装全可追踪     |

建议的替代方案包括：先采用可验证的本地 Paddle runtime；只有 decoder、tokenizer、autoregressive decoding、dynamic shape 与官方结果完成回归后才考虑 ONNX；轻量 ONNX 公式识别只能作为明确标注的 fallback。没有这些条件时，应继续冻结，而不是为了“统一技术栈”强行转换模型。

## 11. 重新开启维护的必要条件

恢复功能开发前必须明确：负责人、评测数据集、模型来源与许可证、可重复运行环境、准确率指标、失败回退策略、Windows 打包能力、内存预算和发布签名链。每个阶段都要编译、单元测试、两份真实 fixture 回归并保存前后指标；任何新模型不得使用桌面 fixture 训练或过拟合。

恢复后的第一批门禁至少包括：`invalidControlChars=0`、`invalidXmlChars=0`、DOCX XML parse、Word/LibreOffice 打开、公式/矩阵样本、中文扫描样本、heading/TOC/section 抽检、Chunk 分布、RAG 资格门控和 2000 页压力数据。

## 12. 维护者交接清单

1. 先阅读 `AGENTS.md`、本文及 `docs/document-engine-development-report.md`。
2. 以 `Document Engine 0.10.0` 和 `CrucibleBox 2.0.1` 为基线，不把旧 handoff 或历史 beta 计划当成已完成事实。
3. 检查工作区、模型目录、worker/PDFium 制品和 SHA-256，再运行两份 fixture。
4. 先修复可复现的安全、崩溃、数据损坏或构建阻断问题；功能路线必须先解除冻结。
5. 代码修改只走 Tauri/Rust/React 当前线，Electron 目录保持冻结；验证命令以 `AGENTS.md` 为准。

## 13. 关联文档

- [Document Engine 开发报告](document-engine-development-report.md)
- [Document Engine v2 架构说明](document-engine-v2.md)
- [Document Engine 数据模型](document-engine-data-model.md)
- [Document Engine Worker 协议](document-engine-worker-protocol.md)
- [开发、构建与验证](development.md)
- [维护计划与历史路线](maintenance-plan.md)
- [发布流程](release-runbook.md)

本文是当前状态和冻结决策的优先参考；旧 handoff、beta 阶段路线和历史报告只用于追溯，不代表当前仍在开发。

## OCR 档位当前状态

快速档只识别文字，无法识别公式。标准档使用 UniMERNet；固定九式样本通过完整 Rust 文档处理链复验，原始 LaTeX 严格匹配 4/9（第一页 4/5、第二页 0/4），达到本轮门槛。运行须配置包含 UniMERNet 的公式附加运行时，未安装时不就绪；该结果不代表其他文档具有相同精度。深度档目标为至少 7/9；截至 2026-10-01 候选尚未达标，按用户修订延后开发，优先完成 beta3 宿主工具箱。深度档暂不开放，不把语义相似度作为严格通过率。未来开放前须补齐模型质量、运行时安装及最终整页验收。

# 架构优化独立验收（2026-10-08）

## 结论

**不通过“原计划全部完成”的总体验收；可以接受为七个官方 Next 插件及核心基础设施的阶段性实现。**

当前已经有实质进展：统一任务核心、无 Tauri 的 repository/协议 crate、显式服务注入、Next 权限与存储、opaque iframe、七插件源码迁移、独立构建、数据保留及本地原生验证。不能将这些成果抹去，也不能将其等同于原方案全部退出。

交付记录 `E:/OCR/next-architecture-delivery-20261008.md` 中“本地 Next 架构开发与当前七官方插件范围的集成验收已完成”和 `next-architecture-final-evidence-20261008.json` 的整体 passed，超出了它们能证明的原计划完成范围。以下结论基于实际源码和原始结果，而非仅引用该总结。

本次仅验收并新增报告；没有修复业务实现、升版、提交、发布或修改既有完成记录。测试会重建 dist/target。所有原生结果均为读取既有证据，本次没有重新启动生产程序或操作生产数据。

## 范围与判定方法

- 对照 `docs/architecture-optimization-proposal.md` 第 4–7 节和 S0–S6、L0–L6 退出条件。
- 采用最新 AGENTS.md 的调整：官方 Next 范围是七插件，Sol 承担全部 S/L；不再要求把其余七个旧插件也迁成 Next。旧包、旧源码、旧数据继续保留。
- “保留旧数据”与“在新宿主永久运行旧协议”是两个决定。后者与原方案的旧运行时退出目标有差异，需明确范围决定，不能悄悄记成已经删完。
- 不将深度 OCR 未达 7/9、没有 Authenticode、没有 OS 沙箱算作本轮新增架构缺陷；它们已有明确边界。正式发布未执行也不单独否定所有本地开发成果。
- 物理文件/目录存在不等于行为完成；单测不等于原生 UI/安装器；页面挂载不等于业务流程；不可变 alpha 快照不等于整个契约已正式冻结。
- 工作区仍有大量已修改/未跟踪实现，HEAD 为 `5e22173`。本次按工作区验收，不按该提交验收。

## 关键未完成项

### A1：文档/PDF 处理没有完成出进程，S4/S6 不满足原目标

原方案 §4.7 要求先提取不依赖 Tauri 的 Document IR/算法，再将 PDFium、重型解析/转换放入 document worker。

当前证据：

- `src-tauri/src/main.rs` 仍直接声明 `document_*`、`pdf_parser` 模块。
- 主 `src-tauri/Cargo.toml` 仍直接依赖 `pdfium-bundled`。
- `src-tauri/src/pdf_parser.rs:623` 的 `PdfiumRuntime`、`:628` 的静态运行时及 `bind_pdfium()` 直接在主进程加载原生库。
- `document_engine_service.rs` 的 parse 任务闭包仍直接调用 `parse_document_with_cache()`；传入 OCR manager 不代表整个解析过程已经移到 worker。
- 当前 `next_backend.rs` supervisor 管理的是 QuickJS backend；已有 OCR manager 也不等于 document worker。
- `ocr_worker.rs:529` 的崩溃测试启动 PowerShell，读取一行后 exit 42；证明 manager 能处理外部进程退出并再次启动，不能证明 PDFium 原生崩溃不影响宿主。

影响：文档业务及原生解析仍与宿主共享故障域。`catch_unwind` 可以处理 Rust unwind，不替代原生库崩溃隔离。

退出要求：提取 Document IR/算法边界、真实 worker 入口及有界请求/结果引用；验证 PDFium/文档 worker 异常退出后宿主保持可用、任务准确失败、资源回收、后续请求恢复，以及取消/超时/进程树退出。

### A2：契约与旧运行时退出尚未收敛

- `contracts/next/contract.json:2` 明确为 `experimental-not-frozen`；当前 AGENTS.md 也写“契约尚未冻结”。
- alpha.7 的 seal/HANDOFF 明确说明是不可变消费者快照，不宣称 S0–S6 release acceptance。快照摘要一致，是有效成果，不能推导整个设计决策已冻结。
- `src/plugin-runtime/frame-entry.ts:280` 仍有 `loadLegacyRenderer()`，`:302` 仍用 `new Function`。
- `src-tauri/src/envelope_host.rs` 仍保留旧 `db.query/db.execute` 分发；`commands.rs` 保留旧 renderer API 映射；当前 architecture 文档明确保留 v2–v4 兼容路径。

Next 调用面本身没有暴露宿主 SQL，不能据旧分发器仍在就宣称 Next 可越权。问题是原计划“迁完正式范围后退出旧兼容”没有完成，也没有在本次可见范围中获得等价替代验收。

退出要求：明确最终稳定契约或继续限定为 alpha 阶段；决定新分发宿主是否继续激活旧插件。若继续，正式修订范围、列维护与隔离成本；若退出，保留包/数据和恢复工具，删除新运行线旧加载/SQL 通道并验收旧插件预检、停用提示与恢复。

### A3：repository 提取完成，但完整数据访问边界未完成

`crates/repository` 已独立且无 Tauri，`db.rs` 只做适配，这是已完成成果。长服务通过便宜的共享 Db 句柄释放外层锁，也已有修复与测试。

但 `crates/repository/src/lib.rs:54` 仍公开 `conn()`；`commands.rs` 和 `host_task.rs` 等仍直接拿连接写业务 SQL，宿主继续大量使用 `Arc<Mutex<Db>>`。没有形成原方案 §4.6 所描述的按领域 repository 与不向适配层暴露 Connection 的完整边界。

这不证明仍存在死锁，也不要求为每个函数制造 trait；它说明“把文件移到 crate”不等于数据层重构已全部结束。

退出要求：按插件、设置、任务、存储收敛用例入口，明确跨表事务归属；限制 conn 的可见性，将数据库调用与 Tauri 适配分离；锁等待用实测而非目录结构验收。

### A4：业务流程与恢复验收的覆盖小于“全部完成”

现有证据应分别接受：

- 七插件独立 runner：每插件 install/typecheck/test/build/pack-a/pack-b 成功，重复包与 canonical runtime 一致。
- 原生 workflow 最终结果：七页挂载；主题切换、日记保存、转盘抽取、GIF 编辑导出有交互结果。
- 原生安全结果：两个 opaque frame、同键命名空间隔离、直接 invoke 被拒、三种可信服务篡改被拒、过期 session 被拒。
- 配对恢复结果：schema 7→10，原数据比较，以及旧程序和旧库副本启动通过。

不能扩大的覆盖：

- `next-seven-native-workflows-final-20261008/results.json` 的文档、UniEnv、归档条目主要是挂载/状态文本，未记录三者完整的 UI 发起任务→实际输出→取消/失败→重启流程。
- 七插件安装保留测试先构造旧 fixture 包，写入统一 `retention.acceptance` 和人工文件，再升级真实 Next 包；这证明安装事务保留，不等于七插件真实历史业务数据的 UI 消费与编辑都验收通过。
- 配对旧库演练为避免旧服务启动，对 candidate 副本停用了插件；不能用它证明迁移后的插件业务均能运行。
- `task-runtime/README.md` 自述仍缺原生安装进程输出、部分 PDF 操作、已验证 checkpoint resume 和断电目录元数据持久性证据。不过当前 `pdf_parser.rs` 已新增并通过 `remaining_pdf_operations_publish_verified_results_and_many_parts_use_one_reference`，覆盖合并、重排、旋转、提图、40 份拆分与重开日志；因此不能仅凭 README 判定这些 PDF 功能未实现。问题是文档与实际恢复矩阵没有收敛，现有测试也不替代全部原生业务及断电验收。

退出要求：补文档、环境、归档三条真实纵向流程，包含真实旧数据消费与失败恢复；列清每项任务是否支持 resume，不支持的明确标 interrupted/可重新执行。断电持久性如果暂不承诺，应明确从计划退出标准移除，而非凭普通重启测试勾选。

### A5：基础资源 profile 已有，但不等于完整按需 runtime 交付

`tauri.base.conf.json` 去掉 OCR worker/ONNX/公式资源，基础安装器已可启动。但仍包含主进程 PDFium 处理；缺失能力检测与完整的按需包安装、版本绑定、活动任务租约、替换/回滚体系不是同一件事。

当前安装器证据使用独立 `com.cruciblebox.nextacceptance`，两次安装均为 beta.3 架构候选；接受其安装/覆盖升级/卸载与数据保留结果，不能当作生产身份及正式版本升级已验收。正式签名发布尚未做是已披露边界，本报告不要求在这次审查中发布。

退出要求：完成文档 worker 后补其独立运行时清单、安装与失败恢复；在最终构建上核对资源版本与摘要，再执行原计划承诺的安装/便携与升级回滚矩阵。

### A6：性能门槛和文档没有完成闭环

- 新 metrics 有三次原生 readiness（约 1.69/2.51/4.64 秒）、七插件首次/再次打开和 working set 快照；记录明确未清系统缓存，也不与 S0 做受控性能比较。
- 没有找到原方案要求的完整同机前后对照：空闲私有内存、并发交互延迟、DB 锁等待、worker 启动、构建各阶段与包体对比、是否超过建议 10% 回归门槛。
- 不据三次混有自动化等待的数值宣布性能退化，也不能据它们宣布性能验收通过。
- 总交付报告写已完成，但 AGENTS.md、契约、task-runtime README 和原方案首包状态仍有未完成描述。活文档还残留安装“待 1.9.2”与旧数据库章节等过期说明。

退出要求：补受控性能报告；将当前实现、批准延期项、实验契约和历史记录分清。L6 应输出与源码、验收矩阵一致的最终文档。

## S/L 工作包逐项判定

“核心通过”只对应已列出的实现与证据，不自动赋予整个原包完成状态。

| 工作包 | 判定                               | 已完成或可接受的部分                                                                           | 剩余退出项                                                      |
| ------ | ---------------------------------- | ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| S0     | 部分完成                           | 脏树基线、备份、文件清单、配对旧程序/库恢复；已有新测量                                        | 同条件性能基线及前后预算比较                                    |
| S1     | 部分完成                           | 契约生成、Rust/TS fixture、SDK/CLI、两个示例、alpha.7 不可变快照                               | 整体契约冻结与完整版本/退出策略                                 |
| S2     | 核心通过，范围仍需收尾             | Next 无 SQL、owner 存储、opaque frame、宿主权限、一次性复制工具与保留机制                      | 全部真实业务旧数据消费验收；旧运行线退出决策                    |
| S3     | 核心通过，恢复矩阵未全闭环         | 一个 task runtime、持久记录、取消确认、终态/投影版本、有界执行、多个适配器，PDF 发布与重开测试 | 原生业务恢复矩阵、checkpoint 与断电承诺边界；修正文档遗漏       |
| S4     | 部分完成                           | 无 Tauri 核心 crate、Services/Platform 注入、repository 提取、Next backend 监督                | Document IR/算法边界、document worker、完整 repository 访问边界 |
| S5     | 七插件源码迁移通过，完整验收未完成 | 七插件 manifest/API 5、自包含 renderer、独立构建、保留测试、部分原生交互                       | 三类可信长任务原生全流程；历史业务数据消费者矩阵；旧运行时退出  |
| S6     | 部分完成                           | base profile、CI 接线、独立身份安装器、封存清单                                                | document worker/按需包完整交付、最终承诺的安装/便携验证         |
| L0     | 盘点主体完成                       | S0 清单、正式范围和基线资产                                                                    | 性能缺口归 S0，不重复记作盘点未做                               |
| L1     | 部分完成                           | 目录行组件、版本比较 selector 已提取并有测试                                                   | Marketplace/Home 的目标边界及交互退出证据未形成完整对照         |
| L2     | 七插件与示例构建迁移通过           | vendored CLI/SDK、独立锁、两次打包一致                                                         | 稳定契约状态归 S1                                               |
| L3     | 七插件源码调用迁移通过             | 无需再要求旧 14 插件全部迁 Next                                                                | 不替代 S5 业务/历史数据验收                                     |
| L4     | 主要接线完成                       | 统一 task client、序号和取消消费                                                               | 原生取消/失败/重启 UI 矩阵仍待补                                |
| L5     | 部分完成                           | 官方目录/身份生成、新清单分发                                                                  | 旧加载/声明/兼容路径未按原计划退出                              |
| L6     | 未完成最终闭环                     | 多份活文档与交付记录已更新                                                                     | 完成声明冲突、历史首包说明及缺口统一                            |

## 本次证据复核

原始复核输出在 `E:/OCR/architecture-audit-20261008/`。

- `prior-evidence-hashes.json`：交付 evidence inventory 的 40 项文件摘要全部相符。
- `seal-check.json`：alpha.7 seal 的 26 项文件摘要全部相符；当前 contract SHA-256 为 `747c2ab1ddf66813b55332eace9ecd45f44fa2819775be45b68b8b8778c88476`，与封存一致。
- `current-plugin-source-vs-seal.json`：七插件 215 项源码与封存 source inventory 一致。
- 读取实际独立 runner 输出 `C:/Users/hjc/AppData/Local/Temp/cruciblebox-next-independent-aIsqhm/results.json`：7×6 步退出码为 0；重复 ZIP/canonical runtime 一致。本次没有重新下载独立依赖。
- 读取 `next-native-security-pass-20261008`、`next-seven-native-workflows-final-20261008`、`next-seven-six-theme-20261008`、`next-native-paired-drill-20261008`、`next-native-metrics-20261008`、`next-base-installer-native-20261008` 的原始 JSON。证据中的限制按原样保留。

## 本次命令复验

初次 npm、Vite、Tauri 编译分别遇到 realpath/canonicalize 的 Windows 权限错误；已保留日志，并经自动审批在沙箱外重跑。不能把这类失败直接算作产品缺陷，也不能省略失败记录。

| 命令                                   | 本次结果                                                          |
| -------------------------------------- | ----------------------------------------------------------------- |
| 根 `npm run check`（含 precheck 构建） | 重跑退出 0；包括格式、lint、typecheck、宿主/插件/供应链/Next 测试 |
| `tauri-frontend/npm run build`         | 重跑退出 0；仍有大于 500 kB chunk 的提示                          |
| 主 Rust fmt/clippy/test                | 重跑均退出 0；316 通过、0 失败、12 默认忽略                       |
| 七个独立核心 crate test                | 全部退出 0；合计 67 通过、0 失败、1 默认忽略                      |
| 独立 sidecar test                      | 退出 0；20 通过                                                   |

七个核心 crate 分别为 task-core 9、task-store 7、file-publication 8、task-runtime 10、repository 12（另 1 忽略）、next-data 9、next-protocol 12。本次未重复七个独立 crate 和 sidecar 的 fmt/clippy；其已有记录通过且摘要已核对，不能写成此次重新执行。

主 Rust 默认忽略项为：3 项 OCR/模型/资源、3 项外部 PDF fixture、3 项 Windows 网络/TLS/PAC、1 项 FFmpeg、2 项独立插件包安装。repository 默认忽略项为真实旧库 fixture。它们不能计入默认测试通过数；既有显式执行记录与本次默认执行分别记录。

本次另行显式执行默认忽略的两项插件安装测试：七官方独立包的升级/卸载/重装保留测试 1/1 通过；alpha.7 两个示例的升级、执行与数据保留测试 1/1 通过。日志分别为 `seven-install-test.log` 和 `examples-install-test.log`。主测试余下 10 项外部 fixture 与 repository 的真实旧库测试本次未重跑；已有真实旧库显式结果只计为此前证据。

## 建议补完顺序

1. **先更正完成状态**：将“全部完成”改为“核心基础设施及七插件 alpha 集成完成，原架构计划仍有下列退出项”。原交付报告保留历史，不覆盖其原始证据。
2. **Sol 收尾结构性缺口**：Document IR/算法与文档 worker、repository 访问边界、旧兼容策略和稳定契约决策。不要为赶验收只改 status 字段。
3. **补三个可信业务闭环**：文档解析并输出、环境安装/切换、归档压缩/解压；在最终原生程序中覆盖取消、失败、重启、产物与旧数据。
4. **补受控性能、运行时资源及最终制品矩阵**：明确测量条件，绑定程序/插件/资源摘要；生产发布身份与签名更新在正式发布工作包执行。
5. **统一活文档与 S/L 矩阵**：每项有实际结果或明确批准的范围修订，再评估是否允许标记全部完成。

无需重新迁移七个插件、重新开发任务核心或抛弃现有证据；应集中补齐上述差距。

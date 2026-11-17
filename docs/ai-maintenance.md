# Next 3.0 维护与交接指南

2.2.0 已正式发布并完成本轮交付。实际交付与验证见 [交付记录](2.2.0-delivery.md)，逐项证据及失败历史见 [补完记录](2.2.0-completion-audit.md)。

架构名称为 Next 3.0，宿主为 2.2.0，包名和目录继续使用 next2。固定 CI 程序的公共能力、真实业务和故障恢复，以及云端签名实际安装切换通过；用户回执覆盖原界面清单及 r4 六项补修。Release、稳定渠道、唯一 Portable、配套插件和实际数据核验均完成，旧测试程序已清理。不得复跑用户已取消的一小时长测，也不得将历史 beta.6 的证据冒作当前程序结果。原告警及续行依据见 [安全记录](2.2.0-security-incident-20261107.md)。

## 开始工作

先读根 AGENTS.md、Git 状态、当前任务书阶段及最近的 docs 验收记录。不得覆盖未提交改动。源码只用本地 Git 保存，未经本轮授权不推送或发布。禁止写 E:\OCR，业务源码与业务验收归独立插件仓库，正式 Portable 不作测试目录。所有 TEMP/TMP/TMPDIR、npm cache 和测试数据留在工作区 .workbench。

## 代码定位

| 职责       | 入口及调用关系                                                                                                                                                                                                            |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 启动与退出 | src-tauri/src/main.rs 的 main 装配 Tauri、数据库、会话与服务；CloseRequested 经 next_backend::Manager::shutdown 停止实例。instance_lock::acquire 在当前源码候选中先取得数据目录排他句柄，阻止第二个新宿主写同一 profile。 |
| 安装许可   | commands.rs 的 preview/commit 进入 install.rs 事务，next2_install.rs 保存按清单摘要绑定的批准，next2-runtime/package.rs 核对文件、签名和架构。                                                                            |
| 路由       | next2_host::Manager::registry_with 从获批清单装配 gateway::Registry；renderer 与 Worker 回调都经身份、权限、上下文及依赖裁决。选中的执行包仍做完整性验证。                                                                |
| 实例       | next2_host/lifecycle.rs 管理 Service/Task 独立实例；next2-runtime/worker.rs 与 process.rs 管理握手、协议和进程树。QuickJS runner 位于 cruciblebox-plugin-host。                                                           |
| 公共能力   | next2_host/platform_service.rs 是可信命名空间装配，文件/网络/进程/流/事件分别由相应模块处理。不要增加业务方法枚举。                                                                                                       |
| 持久任务   | task-core、task-store、task-runtime、file-publication 分别负责状态、SQLite、执行投影与文件发布；next2_host.rs 的 task/restore_task_outcome 将已保存执行结果合回公共查询。                                                 |
| 界面隔离   | plugin_session.rs 签发会话，plugin_protocol.rs 提供受管资源，next_frame_runtime.js 与 NextFrameBridge.ts 建立 MessagePort；NextPluginFrame.tsx 连接主题控制器与可信主窗口。                                               |
| 公开契约   | contracts/next2/contract.json 为生成源；generate-next2-contract.mjs 生成 Rust、TypeScript、frame Schema 及 API 索引。SDK 不能依赖 Tauri 或宿主业务文件。                                                                  |

## 实施规则

1. 先确认具体触发条件、当前保障和最小修复。不要为猜测风险引入状态、缓存、重试或框架。
2. 修改契约时同步生成源、Rust/JS 校验器、SDK 和使用文档，遵守 Manifest/API/wire/Capability 各自的版本策略。变更旧字段语义不能靠只改宿主版本掩盖。
3. 对外输入在信任边界验证；内部复用既有契约。不能删除安装批准、最终路径核对、owner/session 绑定、取消确认或用户数据保留。
4. 路由发现不应散列所有无关包的载荷；实际执行仍核验选中包。优化必须先有测量，不能用修改时间缓存替代安全摘要验证。
5. 临时数据与源码分开。受管文件输出使用已有发布机制，不能“先返回成功再等待落盘”。异步测试应观察真正终态，而非只看任务创建成功。
6. 遇到防护告警先保持拦截、保留日志和失败状态，不恢复、改名、排除或换执行方式制造通过。只读源码校验不等于二进制安全结论。
7. 自动边界检查包含 Cargo 来源与 JS/TS 依赖方向。独立插件使用 node scripts/check-source-boundaries.mjs --plugin-root <插件项目目录>，每个项目分别传参；会检查别名解析、宿主私有引用、公开 SDK 的实际字节/版本漂移，以及原生目录复制的宿主任务框架。更新公开 SDK 后同步受维护项目副本再验，不能只改副本版本号。

## 验证与交接

遵循 [开发门禁](development.md)。Tauri 会复制暂存 sidecar，运行宿主前必须同步匹配的 runner；过期 sidecar 会污染共享目标目录。固定宿主后再做独立插件验收，宿主改变时重新固定，不将两个候选拼接成一次通过。

交接说明至少包含源码提交、候选/SDK/契约版本、改动原因、实际测试、失败/忽略项、制品位置、剩余事项。发布说明按最终实现重写，不能复用上一版本的成功结论。当前 beta.6 的菜单创建/隐藏代码已移除，profile 锁通过真实双实例、独立目录和终止释放复验；页面创建的数据库竞争已修复。原生显示、主题恢复与正常重开采用 2026-11-08 用户回执，不把该回执推广到未提及的全部交互。依赖替代只移除文档探测程序，运行库源码不变；升级依赖时复查其 ORIGIN.md。

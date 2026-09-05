# UniEnv 当前状态、能力边界与维护冻结说明

> 文档状态：维护冻结说明（2026-09-05）
> 当前宿主版本：CrucibleBox 2.0.1
> 当前插件版本：UniEnv 0.11.0

## 1. 决策

UniEnv 自 0.11.0 起暂停功能性更新。当前实现保留并继续作为 CrucibleBox 的第一方环境管理能力，但暂不继续增加运行时、组合包、安装策略、在线版本源、界面功能或性能优化。原因是当前没有足够的人力和工具持续验证 Windows 多版本工具链、上游制品、安装恢复和安全边界。

暂停不等于删除或停止已有安装：用户可以继续查看、安装、切换和卸载当前已支持的工具（前提是官方制品地址可达且摘要匹配）。只对安全漏洞、数据损坏、无法启动、安装事务破坏、严重崩溃和构建阻断进行必要维护评估；任何新功能必须先正式解除冻结。

## 2. 产品定位

UniEnv 是 CrucibleBox 内置的开发环境管理插件，统一处理工具链的发现、版本选择、下载、校验、解压、安装、切换、卸载、运行和组合包编排。它不是系统级包管理器，不修改机器级环境，不执行用户任意 shell 字符串，也不把普通插件的权限扩大为宿主级安装权限。

当前覆盖的工具和能力以 `plugins/unienv/src/` 的目录及宿主可信服务为准，包括 Python、Node.js、Git、Go、Java，以及 Rust、PHP、Ruby、Zig、Deno、Bun 等已登记版本和组合包。具体版本随发布时固定的制品目录变化，不应由本文推断出某个上游版本永远存在。

## 3. 安全架构

```text
UniEnv renderer
    -> Manifest v2 / MessagePort RPC
    -> trusted.invoke("unienv", ...)
    -> 宿主 PermissionGuard 与固定摘要策略
    -> Rust UniEnv trusted service
       catalog / versions / install / task
```

UniEnv 的高权限操作不在插件包中实现。`trusted:unienv` 必须匹配宿主允许的服务名、版本、文件集合和 SHA-256 digest；摘要不匹配时 fail-closed，拒绝激活。普通插件不能借用 UniEnv 的下载、文件、解压、junction 或进程能力。

关键决策和边界见 `docs/security-model.md`、`docs/architecture.md`、ADR-0005、ADR-0017 和 ADR-0021。Electron 中的旧实现仅作历史参照，不能作为当前 Tauri 行为的可编辑来源。

## 4. 代码组成与职责

| 文件/模块 | 职责 |
| --- | --- |
| `plugins/unienv/plugin.json` | 插件身份、版本、权限和 renderer/backend 入口 |
| `plugins/unienv/src/main.ts` | 插件侧能力代理和生命周期入口 |
| `plugins/unienv/src/renderer.tsx` | 工具、版本、组合包、任务和操作界面 |
| `src-tauri/src/unienv_catalog.rs` | 内置制品目录、固定版本、大小和 SHA-256 完整性信息 |
| `src-tauri/src/unienv_versions.rs` | Node/Go/Java 等官方端点的在线版本发现，含 8 秒硬超时 |
| `src-tauri/src/unienv_install.rs` | 下载、摘要校验、解压、版本目录、junction 和安装原语 |
| `src-tauri/src/unienv_task.rs` | 单飞任务、进度、取消、状态快照和失败结果 |
| `src-tauri/src/envelope_host.rs`、`commands.rs` | trusted service 路由、权限和宿主 IPC |
| `shared/trusted-service-policies.json` | 可信服务版本/文件/摘要策略 |
| `docs/install-recovery.md` | staging、崩溃恢复和安装事务边界 |

## 5. 操作流程

### 5.1 查看目录和版本

目录优先使用宿主内置的固定制品信息，确保下载对象有版本、大小和摘要。动态版本发现只用于受支持的官方版本源，并受超时、HTTPS、主机白名单和解析规则约束。在线源不可用时，应保留已知目录和清晰的网络错误，不把“未发现”当成“可安装”。

### 5.2 下载与完整性

安装任务使用 HTTPS 官方制品地址，流式写入临时文件，计算 SHA-256，大小和摘要均通过后才能进入解压/安装阶段。网络失败、HTTP 错误、超时、包体损坏和摘要不匹配必须区分记录。下载失败不能留下可被误认为已安装的半成品。

### 5.3 staging 与原子提升

制品先落在版本根目录下的 `.unienv-staging-*` 临时目录，校验和解压完成后再原子提升为正式版本目录。安装过程中取消或失败应清理对应 staging；崩溃恢复只枚举严格目录表派生的版本根，只删除合规的 staging 子目录，不删除已安装 runtime 兄弟目录。

### 5.4 版本切换、运行与卸载

每个工具按版本目录保存，通过受控 junction 或等价路径选择当前版本。执行进程使用固定的可执行文件和参数配方，`shell: false`，不拼接任意命令行。卸载只作用于目录表确认的 UniEnv 版本目录；切换失败不能破坏当前可用版本。组合包是多个已登记工具的编排，不应绕过单项安装校验。

## 6. 当前支持的宿主能力

- 工具和组合包目录读取。
- 已登记工具的版本列表与当前安装状态检测。
- 官方制品下载、进度展示、SHA-256 校验和解压。
- 安装、取消、切换、卸载和组合包任务。
- 任务单飞、进度轮询、失败状态和取消传播。
- 用户级版本目录与可控 PATH shim（若该版本功能已启用）。
- Node/Go/Java 的在线版本发现；其他工具以固定目录为准。
- 安装 staging、原子提升和中断恢复边界。

## 7. 数据、路径和错误边界

UniEnv 的安装路径必须由宿主计算并限制在应用数据/用户指定的 UniEnv 根目录内；禁止路径穿越、任意 junction 目标、任意解压目标和符号链接逃逸。任务快照只返回必要的状态、进度、阶段和错误摘要，完整日志由任务中心统一承载。

典型状态顺序为：`queued → downloading → verifying → extracting → activating → completed`；取消、网络错误、摘要错误、模型/制品缺失和进程失败进入明确的失败或取消终态。客户端不能只依据一个百分比判断成功，必须等待最终状态和安装检测。

## 8. 已知限制与风险

1. 上游下载地址、版本目录和制品可用性会变化；固定摘要策略会主动拒绝未知或被替换的文件。
2. 在线版本源受网络、TLS、上游 API 格式和 8 秒硬超时影响；它不是完整镜像或离线仓库。
3. 当前任务注册表主要是进程内状态；安装恢复有边界，但不应宣传为所有下载均可跨重启续传。
4. Windows 权限、杀毒软件占用、长路径、junction 权限、文件锁和代理环境都可能导致安装失败。
5. 多版本工具同时存在会增加磁盘、PATH、进程和用户认知成本；当前 UI 不保证覆盖所有上游发行版差异。
6. 组合包失败可能发生在中间步骤，必须以任务日志和实际安装检测为准，不可仅以 UI 按钮状态判断。
7. UniEnv trusted service 是可信代码边界，不是面向不可信插件的操作系统沙箱；普通插件不能获得同等权限。

## 9. 暂停中的未来路线（非当前承诺）

解除冻结后建议按以下顺序执行，每阶段单独回归，不同时改动所有安装原语：

| 阶段 | 调整方向 | 验收重点 |
| --- | --- | --- |
| U1 | 制品目录与版本源统一为带版本、大小、许可证、摘要和来源的 Model/Artifact Registry | 固定目录、在线发现和缓存的优先级明确，未知制品 fail-closed |
| U2 | 下载器增强：可恢复 Range、持久任务、连接失败分类、代理/直连策略和跨重启恢复 | 中断后摘要仍正确，断点文件不可伪装为完整包 |
| U3 | 安装事务增强：journal、崩溃恢复、回滚和并发锁可观测 | 强杀、磁盘不足、权限错误、杀毒占用均不破坏旧版本 |
| U4 | 版本矩阵与兼容性检测 | Windows x64 各工具、多版本、组合包和 PATH 行为有固定测试矩阵 |
| U5 | 内存、磁盘和并发优化 | 下载/解压/切换峰值可测，长任务不阻塞市场、设置和其他插件 |
| U6 | UI 与任务中心统一 | 进度、日志、重试、取消、失败原因和实际路径一致；页面切换不中断任务 |
| U7 | 安全与供应链复核 | URL 白名单、SHA-256、签名/证明、许可证、SBOM、策略 digest 自动门禁 |

禁止为了方便而回退到任意 shell、动态下载未校验制品、把高权限能力放回插件 backend，或用“下载完成”替代摘要和安装检测。

## 10. 解除冻结的前置条件

必须先确定维护负责人、支持工具清单、上游来源和许可证、Windows 测试机、故障注入脚本、可重复制品摘要、安装恢复测试矩阵、内存/磁盘预算和发布签名流程。每个工作包都要先建立失败场景，再修改代码；验证至少覆盖单项安装、取消、重复安装、切换、卸载、组合包、网络中断、磁盘不足、权限拒绝、进程占用和应用重启。

## 11. 维护者交接清单

1. 阅读 `AGENTS.md`、本文、`docs/architecture.md`、`docs/security-model.md` 和 `docs/install-recovery.md`。
2. 检查 trusted-service policy、目录摘要、已安装版本目录和任务状态，不凭历史 handoff 推断当前实现。
3. 确认当前发布版本为 CrucibleBox 2.0.1 / UniEnv 0.11.0；Electron 目录保持冻结。
4. 先复现安全、崩溃、数据损坏或构建阻断问题，再判断是否需要解除冻结。
5. 涉及 UniEnv 可信代码时，修改后必须重新生成并核对 trusted digest，再执行 AGENTS.md 规定的 Rust、前端和插件验证。

## 12. 关联文档

- [架构说明](architecture.md)
- [安全模型](security-model.md)
- [安装与恢复](install-recovery.md)
- [ADR-0005：后端运行时与可信服务](adr-0005-backend-runtime-and-trusted-services.md)
- [ADR-0017：中断安装恢复](adr-0017-unienv-interrupted-install-recovery.md)
- [ADR-0021：在线版本源](adr-0021-unienv-online-version-feeds.md)
- [开发与验证](development.md)
- [发布流程](release-runbook.md)

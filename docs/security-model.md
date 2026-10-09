# CrucibleBox 安全模型（Tauri 2.11.x 与 Next beta.1）

> 当前宿主开发基线为 2.1.0-beta.4。Next 契约已冻结为 Manifest/API 5、wire 3、data 1。
> Electron 安全实现仅作为历史材料保留；本文描述当前 Tauri/Next 运行路径。

## 信任与权限

插件按用户明确安装的可信代码管理。renderer 隔离用于限制宿主资源访问与会话混淆，不构成对恶意 backend 代码的 OS 沙箱。权限声明限制插件可请求的 capability；宿主在信任边界逐次校验会话身份、owner、方法及权限。

普通 Next 插件只通过 SDK 暴露的宿主 API 操作任务、主题、通知、对话框和命名空间 storage，不获得 SQL 连接或任意本地路径接口。宿主固定摘要的官方可信服务使用单独策略；构造、身份、版本、权限、文件集合或摘要校验失败时拒绝激活或调用。

## Renderer 会话边界

- 插件 renderer 在 sandboxed iframe 中运行，每次打开由 Rust 宿主签发会话身份并绑定所属窗口。
- 宿主与 renderer 使用专用 MessagePort 和冻结 wire 契约；未知方法、过期会话、超出帧预算的请求或无权限调用失败关闭。
- Next renderer origin 为 opaque origin；WebView2 自定义协议仅提供本会话白名单内的静态资源，不暴露 Node、Rust 或宿主 DOM。
- 会话队列、超时、销毁状态和会话身份由宿主维护，renderer 不能自行指定 storage owner。

## Backend 与可信服务

Next backend 在独立 cruciblebox-plugin-host sidecar 内的 QuickJS runtime 运行，通过 Next wire 协议与宿主通信。sidecar 没有 Node builtin；进程隔离用于故障隔离，不应被描述为针对恶意代码的强制安全沙箱。所有宿主 capability 仍由宿主校验。

三个官方可信服务的身份、版本、权限和静态 SHA-256 摘要固定在 shared/trusted-service-policies.json。构建校验、安装策略和运行时会话使用同一策略；改动可信服务时重新运行 npm run update:trusted-policy 与 npm run verify:trusted-services。

## 安装、迁移与回滚

Next 安装及升级由 Rust 宿主执行 staging、固定快照确认、journal、同卷替换和启动恢复。manifest、归档路径与普通文件约束在宿主边界校验；无法无歧义恢复的事务阻止插件激活并保留现场。

repository crate 独占生产 SQLite 连接及迁移。插件数据以准入身份分区；卸载与重装按稳定插件 ID 保存并恢复原配置、storage 原值和迁移标记。Next 数据迁移不删除原数据。回滚旧运行程序必须配对恢复其原数据库；scripts/next-paired-rollback.mjs 只恢复已验证的旧程序/数据库配对。

## 任务、文件输出与文档处理

统一 task runtime 校验执行器、owner、取消意图与终态；任务 journal 持久化恢复信息，迟到事件不得覆盖终态。结果发布经文件事务写入并返回受限引用，插件不能任意读取宿主路径。

PDFium 与 Document IR 仅由独立 document-worker 进程加载。宿主对 worker 运行目录、摘要、超时和输出引用进行校验；PDFium native abort、超时和取消的进程隔离已有本地原生测试。此边界不代表 OCR 准确率、模型质量或长文档资源门禁通过。

## 供应链与发布限制

第一方插件产物使用 Ed25519 签名及确定性文件清单；发布流程生成 SBOM 并附带 GitHub artifact attestation。Tauri updater 要求 minisign 签名。安装器当前未使用 Authenticode 证书；sidecar 和 worker 的进程隔离也不等同于 Windows 强制沙箱。

当前支持范围为 Windows 10/11 x64。未执行的 WebView2 手工流程、正式安装器、配对回滚 UI、OCR 精度或发布验收必须作为独立门禁报告，不能从单元测试或源码审查推断通过。

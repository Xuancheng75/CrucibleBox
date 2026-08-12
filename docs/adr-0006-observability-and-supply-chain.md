# ADR-0006：可观测性、性能预算与发布供应链

- 状态：Accepted
- 日期：2026-08-10
- 里程碑：M2.4

## 背景

M2.1–M2.3 已建立事务恢复、跨源 renderer、backend RPC v2 和 UniEnv 可信服务，但发布链仍只
打印 ZIP 摘要，缺少可携带的物料清单、签名入口与 SBOM；运行时也没有稳定的启动性能记录或
主进程异常会话标记。CI 只覆盖 Windows，无法尽早发现普通 TypeScript/Node 构建的跨平台回归。

## 决策

1. 每次插件打包生成 `artifacts/plugins/manifest.json`，记录应用版本、插件版本、API 版本、ZIP
   大小/摘要及每个运行文件的大小/摘要。验证器从实际 ZIP 和构建目录独立重算，拒绝漂移。
2. 正式发布使用外部 Ed25519 私钥签署清单。私钥与公钥路径只通过环境变量提供，不写入仓库；
   `release` 缺密钥或验签失败时立即终止。开发态 `package:plugins` 保持可用但不声称已签名。
3. 根工程和六插件分别生成 npm CycloneDX SBOM。CI 上传 SBOM、插件 ZIP 和清单作为供应链产物。
4. 根工程及六个插件锁文件全部纳入 `npm audit --audit-level high`；M2.4 基线为 0 漏洞。
5. `StartupMetrics` 输出版本化 JSON：里程碑耗时、进程数、插件 utility 数和总 working set。
   打包冒烟将 20 秒交互时间、1 GiB working set 和两个真实插件 utility 作为宽松回归上限。
6. `DiagnosticLog` 在 `userData/logs` 写有界 JSONL，保留一个 2 MiB 轮转备份；session marker 用于
   下次启动识别非正常退出。renderer 异常退出至多每 60 秒自动恢复一次，避免崩溃循环。
7. 构建后执行版本化体积预算：main、宿主 renderer、frame runtime 和六个插件 renderer 任一超限
   都使构建失败。预算是回归阈值，不是性能目标值。
8. CI 的 check/build/audit 扩展到 Windows、Ubuntu、macOS；Electron ABI、unpacked 打包及真实 GUI
   冒烟仍在 Windows 执行，其他平台的签名安装包不在本决策的完成声明内。

## 取舍

- Ed25519 清单保护第一方插件制品的一致性和来源，但不能代替 Windows/macOS 安装器代码签名。
- npm SBOM 描述依赖图，不证明依赖本身安全；审计、锁文件、摘要、外部签名共同构成发布门禁。
- JSONL 是本地、低依赖的诊断基础，不上传遥测，也不记录环境变量、文件内容或插件消息负载。
- 当前 renderer bundle 仍较大；本阶段先建立可执行预算，代码拆分只有在基准证明收益后再进行。

## 兼容性

本阶段不修改数据库 schema、插件配置或用户业务数据。新增日志与 session marker 位于应用自身
`userData/logs`；干净退出会删除 marker。六插件因依赖锁和发布制品变化仅提升补丁版本。

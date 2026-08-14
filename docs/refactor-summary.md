# 2026 现代化重构总结

## 结果

CrucibleBox 保持 Electron + React 技术栈，从单文档动态执行、无恢复安装、宽泛 backend 能力和全局插件表，
演进为 Electron 43 / React 19 基线、跨源 renderer、utility backend RPC v2、可恢复安装事务、宿主可信
服务、插件私有存储、页面级加载、Theme v2、可观测性和可验证发布链。主工程版本为 1.5.23。内置主题
共六套（亮色/深色/清新绿/海洋蓝/科幻面板/零号城区）；插件列表以 schema v3 `plugins.sort_order`
持久化顺序，支持长按拖动与键盘上移/下移。

重点插件成果：

- GIF Editor 修复非方图旋转、帧 disposal、全帧画布不变量、裁剪、历史/缩略图泄漏和滤镜重复提交；
  增加导入、帧数、像素、工作集、分层与导出预算，将重型残影检测/修复迁入可取消 Blob Worker，并以
  可逆 XOR 区间保存同画布增量历史，44 项插件测试持续通过。
- UniEnv 移除字符串 Shell，统一 executable+argv；增加严格协议/路径、可取消任务、有界 HTTPS 下载、
  安全 staging；最终将进程/文件/下载/解压实现移入宿主固定摘要可信服务，并为全部受支持上游制品固定
  官方 URL、文件名和 SHA-256；UI 进一步区分维护分支、EOL 与旧版，默认选择目录内风险最低的兼容版本，
  132 项测试持续通过；Node、Git、Go 与 Temurin 目录已补入当前维护制品，旧版本仍兼容保留；启动时会
  安全清理中断安装 staging，恢复检查失败时环境写操作 fail-closed。
- Dice 改为显式 renderer-only 插件，宿主不再为其创建空 backend 进程；随机数改用 Web Crypto 与无偏
  rejection sampling，边界、拒绝区间和确定性均匀分布均有纯函数测试。
- Diary 在私有 namespace 上增加受限原子批处理，正文保存/删除与草稿清理同事务提交；编辑草稿自动恢复，
  保存使用显式 Result 与编辑 revision，磁盘失败或保存期间继续编辑都不会离开页面，日期键不再受时区影响。
- Turntable 用 Web Crypto 和半开权重区间选择 winner，统一后端结果与顶部指针几何；mutation queue 与
  `storage.batch` 防止并发 CRUD 丢更新，重排必须精确包含全部 ID，旧 `items` 数据结构保持兼容。

## 诊断闭环

| 初始严重问题                            | 处理结果                                                               |
| --------------------------------------- | ---------------------------------------------------------------------- |
| 同 document `new Function` 可取宿主全局 | 删除该路径，改为唯一跨源 sandboxed iframe + MessagePort RPC            |
| `fork` backend 继承环境且权限可直接绕过 | utility process、最小环境、RPC v2；明确可信 backend 模型               |
| UniEnv 直接 shell/fs/fetch              | 宿主固定摘要可信服务；插件包仅保留代理                                 |
| 生命周期并发、僵尸 runtime、超时不终止  | per-plugin single-flight、等待退出、崩溃清理和显式重启                 |
| 升级先删旧目录且无回滚                  | 不可变 stage、journal、原子 rename、补偿事务、启动恢复                 |
| 安装确认 TOCTOU                         | preview/确认/提交消费同一 stage token                                  |
| 原始 SQL 共享全库                       | 私有 KV 命名空间、读写权限、事务化兼容迁移                             |
| 主题事件作用域断路与硬编码颜色          | CSS 变量 + antd token + renderer/backend 事件的统一主题契约            |
| Electron 34 已 EOL                      | 精确锁定稳定 Electron 43.3.0 与 ABI 148                                |
| 无插件源码/测试/可复现发布证据          | 六个独立源码工程、lockfile、clean build、确定性 ZIP、摘要、签名和 SBOM |
| 无 E2E/性能/诊断门禁                    | Windows packaged smoke、启动/内存/体积预算、JSONL 诊断与 Windows CI    |

## 技术选型

| 领域     | 比较                                              | 决策与理由                                                  | 迁移风险与控制                                          |
| -------- | ------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------- |
| 构建     | electron-vite / 自建 Vite / webpack               | electron-vite 5，进程入口清晰且保留 Vite 生态               | Electron 42 下载变化；移除固定 electronDist 并打包验证  |
| UI       | Ant Design 6 / MUI / Chakra                       | 升级 Ant Design 6，保留成熟组件并采用 semantic DOM API      | 自有 `ob-*` 类替代内部 DOM selector；完整 renderer 冒烟 |
| 状态     | zustand / Redux Toolkit / Jotai                   | 保留 zustand，当前状态规模小、store 边界清楚                | 不引入迁移噪声；跨进程状态仍以 RPC/event 为准           |
| Electron | 旧 34 / 稳定 43 / beta 44                         | 稳定 43.3.0；受支持且使用 Node 24 / Chromium 150            | native ABI 132→148；Electron 与 packaged 双重冒烟       |
| renderer | Proxy/new Function / BrowserView / sandbox iframe | 唯一跨源 iframe；最小文档边界且与 React 宿主解耦            | Blob 下载与 modal 兼容；下载显式允许，确认改 RPC        |
| backend  | child fork / worker / utility process             | utility process；Electron 生命周期/MessagePort/指标集成更好 | 不是安全沙箱；可信模型与 UniEnv 专用服务分层            |
| 数据库   | sql.js / better-sqlite3 / 外部服务                | better-sqlite3 WAL 默认、sql.js 显式 fallback               | native rebuild/打包；ABI smoke 与 sql.js 旧库备份       |
| 插件数据 | 全局 SQL / 每插件 DB / namespaced KV              | 宿主 KV；API 小、可校验、自动隔离并支持统一迁移             | 批处理限 64 项；复杂关系模型仍需版本化设计              |

React Server Components 未采用：Electron renderer 是本地静态客户端，没有服务端渲染边界；引入 RSC 会增加
运行时和打包复杂度而不产生对应收益。

## 里程碑

1. M1：可复现工程基线；GIF Editor 与 UniEnv 专项正确性/安全。
2. M2.1：插件生命周期、安装事务和崩溃恢复。
3. M2.2A：Electron 43 / electron-vite 5 / sandboxed preload。
4. M2.2B：跨源插件 renderer 与严格 MessagePort RPC。
5. M2.3：utility backend SDK v2 与 UniEnv 可信服务。
6. M2.4：可观测性、性能预算、依赖治理、签名、SBOM 和 CI。
7. M2.5：插件私有存储与 Diary/Turntable 兼容迁移。
8. M2.6：数据库 fail-closed 与真实 packaged schema v1→v2 备份/迁移门禁。
9. M2.7：宿主页面级代码拆分与默认首页启动闭包预算。
10. M2.8：Windows/Linux/macOS unpacked 打包与统一临时数据冒烟门禁。
11. M2.9：GIF Editor 残影分析 Worker、取消生命周期与单次分析修复。
12. M2.10：UniEnv 全版本上游制品摘要、流式校验、镜像回退与 JDK 元数据修复。
13. M2.11：UniEnv 版本生命周期目录、首选版本排序和旧版本安装确认。
14. M2.12：UniEnv 当前维护制品、内置组合升级和当前/旧版差异化安装提示。
15. M2.13：Python 3.14.7 当前稳定制品与 Install Manager 系统集成边界。
16. M6：单一主题注册表、隔离 renderer `theme.list`、可逆预览、旧主题迁移与 `@openbox/ui`。
17. M7A：Dice renderer-only 执行模式、Web Crypto 无偏随机数与边界/分布测试。
18. M7B：GIF 可取消 Worker、资源预算、typed-array 执行边界与增量 XOR 撤销。
19. M7C：由 M6 交付的 Theme v2 capability、宿主单一 PRESETS 注册表与 ThemeManager 预览回滚闭环。
20. M7D：私有存储原子批处理、Diary 草稿恢复、显式保存 Result 与时区安全日期。
21. M7E：Turntable 安全加权 RNG、winner 几何性质、原子串行重排与重启持久性。
22. M7F：UniEnv 固定摘要 `environment.manage` 能力、崩溃 staging 恢复与写操作 fail-closed。
23. M8：Ant Design 6、semantic classNames、弃用 API 清理和键盘/主题矩阵门禁。
24. M9：Windows x64 GitHub Releases、NSIS 更新元数据、SBOM/校验和/provenance、显式 stable/beta 更新与
    beta-to-stable 回滚，以及停止新装 Manifest v1。安装器当前不使用付费 Authenticode 证书。
25. M9.1：GitHub 自动更新可选化。普通 `npm run package` 生成无仓库依赖的本地 NSIS 成品，无需 GitHub
    仓库、Token、插件发布密钥或 Windows 证书；未配置 `app-update.yml` 的安装包禁用在线更新并保留完整
    离线功能，Release 工作流仍保留给仓库所有者按 `docs/github-auto-update-handoff.md` 启用。
26. 插件排序与主题基线：数据库 schema v3 增加 `plugins.sort_order`，v2→v3 按既有显示顺序稳定回填；
    列表支持长按拖动排序与上移/下移（键盘可操作），顺序在事务内持久化且激活顺序跟随列表。内置主题
    六套，含科幻面板与零号城区。

每个里程碑均以 typecheck、lint、测试、生产构建和相关打包冒烟结束，并建立 Git checkpoint。

## 最终验证基线

- 宿主：35 个 Vitest 文件、254 项。
- 六插件合计 190 项：Diary 1 文件/4 项、Dice 1 文件/5 项、GIF Editor 5 文件/44 项、Turntable 1 文件/5 项、
  UniEnv 11 文件/132 项。
- 脚本与供应链：16 项 Node 测试全部通过（当前 0 跳过）。
- 根工程与六插件 `npm audit`：0 漏洞。
- Windows packaged smoke：1.5.23 初始化 1,237 ms、working set 427,988 KiB，预置 schema v1 的临时
  userData 完成字节备份、v2/v3 迁移与 `sort_order` 回填断言；仅 UniEnv 创建 utility process，Dice
  renderer-only 负向断言通过。
- 生产构建、体积预算、六插件确定性 ZIP/摘要、可信服务摘要、七份 SBOM、Electron ABI 与 packaged
  renderer/backend 冒烟通过。

## 已知边界

- 未为任意恶意 Node backend 提供 OS 级强制沙箱；第三方 backend 必须可信。
- Windows 安装器当前明确采用无 Authenticode 证书模式，首次安装或升级可能显示 Unknown publisher/SmartScreen；
  macOS、Linux 与 Windows ARM64 不在支持范围。
- 正式插件签名需要仓库外 Ed25519 密钥；开发清单不冒充正式签名。
- UniEnv 的真实 Windows VM 各工具安装/取消/切换/回滚 E2E 已通过；目录同时保留当前制品和遗留兼容版本，
  正式发布前仍需制定旧版本下线策略。
- 宿主 renderer 总 JS 约 2.79 MB；静态入口和默认首页启动闭包分别约 1.09 MB、2.02 MB，均有预算门禁。
- KV 不适合大型关系数据；原始数据库 API 在 SDK v1 兼容期仍存在。

构建、运行、测试和发布命令见 `docs/development.md`；模块与数据流见 `docs/architecture.md`；插件迁移
见 `docs/plugin-sdk-migration.md`。

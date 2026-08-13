# CrucibleBox 维护复杂度优化方案（整合版）

> 状态：方案评审稿（2026-08-12），尚未开始实施。
> 基线：CrucibleBox 1.5.23（宿主 254 项 / 插件 190 项 / 供应链 16 项测试全绿）。
> 唯一可编辑源码：`E:\CrucibleBox_Sourses`（git 仓库，HEAD `c22609b`）。
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
  7. 不引入插件市场和自动插件升级；
  8. 不提前支持 macOS、Linux、ARM64；
  9. 不维护第二套可编辑插件源码；
  10. 不再增加阶段性 milestone 文档。

## 1. 复杂度热点（审计确认）

| #   | 热点                   | 结论                                                                                                                         |
| --- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 1   | RPC 三套重复基础设施   | 三套独立信封：backend RPC(v2)、renderer RPC(v1)、host IPC（无版本），外加第 4 个迷你握手；校验器/错误码/pending 追踪三处重复 |
| 2   | PluginManager 上帝对象 | 888 行；9 张状态 Map；performActivation(~215 行) + handleChildRequest(20 分支 switch) + 日志 SQL + 崩溃恢复编排仍内联        |
| 3   | 双数据库引擎           | 同接口同步实现，仓储/插件 RPC 零感知；生产仅 database/index.ts 一处用 sql.js；无运行时 fallback                              |
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
│   └── openbox-plugin-api/       ← 插件 API 类型唯一事实源（替代 6 份 openbox-api.d.ts）
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

| 规范文件                   | 来源 / 说明                                                            |
| -------------------------- | ---------------------------------------------------------------------- |
| `docs/architecture.md`     | 已有，更新基线                                                         |
| `docs/security-model.md`   | 从 trusted-release.md + AGENTS 安全模型抽取（新建）                    |
| `docs/plugin-sdk.md`       | 从 plugin-sdk-migration.md + 模板 + openbox-plugin-api 提炼（新建）    |
| `docs/install-recovery.md` | 从安装事务族 + ADR 提炼，含显式状态机定义（新建）                      |
| `docs/release-runbook.md`  | 从 trusted-release.md + delivery-package.md + release.yml 提炼（新建） |
| `docs/development.md`      | 已有，更新                                                             |

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

- 共享插件 API 类型：新增 `packages/openbox-plugin-api/`，模板 + 6 插件改引用，删除复制 d.ts
- digest 生成脚本化：`scripts/update-trusted-policy.mjs`（unienv 重建后自动重钉）
- PluginManager 状态记录：`plugin-system/runtime/PluginRuntimeRecord.ts`，9 张 Map 合并，capability 分发表
- 安装事务收敛为显式状态机：`prepared → awaiting-confirmation → staged → stopping-old → applied → committed / recovery-required`；`PluginInstallPreparation/InstallationService/DirectoryTransaction/TransactionRecovery` 职责对齐

出口验证：全量测试（254/190/16）+ 生命周期专项 + dev 冒烟（6 插件激活/切换/卸载无 EBUSY）。

### 1.6.X — 运行时简化线（逐 minor 拆分）

> 1.5.25 已含：openbox-plugin-api、digest 脚本、PluginManager runtime record、安装事务状态机。1.6.X 不重复。

| 版本      | 主题                               | 内容（文件级）                                                                                                                                                                                                                                                                                                                        | 出口验证                                                                                                 |
| --------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| **1.6.0** | 数据库运行时收敛（单引擎）         | `database/index.ts`：删除 `OPENBOX_DB_ENGINE` 生产切换，生产只走 BetterEngine；SqlJsEngine 保留实现但不再可被生产选择；`package.json` 将 sql.js 移入 devDependencies；新增 `scripts/db-recovery-tool.mjs`（用 sql.js 离线读/修 openbox.db）；保留迁移、启动备份、失败抛错路径；`electron-builder.yml` 确认 sql-wasm 不再进安装包      | 全门禁 + `smoke-packaged` 旧库迁移 + `release-compatibility`（previous→candidate 回滚）+ 单引擎 dev 冒烟 |
| **1.6.1** | RPC 公共基础层                     | 新增 `packages/openbox-rpc/`（RpcEnvelope / RequestTracker / TimeoutManager / PayloadBudget / ErrorCodec / SessionRegistry）；`shared/plugin-backend-rpc.ts`、`shared/plugin-renderer-rpc.ts` 改为"内核 + 各自 capability 注册表"；`src/plugin-runtime/frame-entry.ts` 第三套 pending Map 改用内核追踪器；线上信封 v2/v1 字节格式不变 | 新增"新旧构造字节级等价"测试 + RPC 专项测试（backend/renderer/frame-bridge）+ 全门禁 + 6 插件激活冒烟    |
| **1.6.2** | UniEnv 物理隔离 + trusted 目录整理 | 迁移至 `plugin-system/trusted-services/unienv/`（宿主固定加载，不依赖插件源码相对路径）；`TrustedServiceRuntime` 收敛到稳定内部模块边界；`update-trusted-policy.mjs` 最终定型并挂 CI 门禁；固定文件集/摘要/fail-closed 语义零变化                                                                                                     | trustedServiceRuntime 专项测试 + unienv 插件测试 + VM 冒烟（安装/切换/回滚）                             |
| **1.6.3** | 稳定性收尾（按需发布）             | 1.6.0–1.6.2 暴露的回归修复；性能预算复测（`verify:performance`）；测试套件按新内部结构重构对齐；无修复需求则跳过，不空发                                                                                                                                                                                                              | 全门禁 + 完整 CI                                                                                         |

节奏：1.6.0 ≈ 1–2 周末，1.6.1 / 1.6.2 各 ≈ 1 周末，1.6.3 视回归情况。

### 1.7.X — 兼容性清理 + SDK v2 冻结线（逐 minor 拆分）

| 版本       | 主题                             | 内容（文件级）                                                                                                                                                                                                                                                                                                    | 出口验证                                             |
| ---------- | -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| **1.7.0**  | 删除层（production removal）     | ① 删除旧 backend/renderer **fallback**（`OPENBOX_PLUGIN_PROCESS=0` 进程内回退、旧 raw SQL 新装路径）——只删回退，不删 utility process 主路径与沙箱；② 删除生产 sql.js（SqlJsEngine、WASM 加载路径、`shared/types/sql.js.d.ts`，恢复工具改为独立脚本+文档）；③ 删除 `E:\CrucibleBox_Plugins` 镜像（确认已冻结备份） | grep 无引用证明 + 全门禁 + installer smoke + VM 冒烟 |
| **1.7.1**  | SDK v2 冻结 + theme-manager 测试 | `docs/plugin-sdk.md` 正式声明 SDK v2 冻结（此后 API 变更走 v3 提案，经 `packages/openbox-plugin-api` 版本化）；新增 CI 门禁（6 插件对冻结 d.ts 构建兼容矩阵）；`plugins/theme-manager/tests/`（manifest 契约 + 渲染入口最小测试）；theme-manager 补 vitest 依赖                                                   | 全门禁 + 插件 190 项（现 189+1）                     |
| **1.7.2**  | 代码库可维护性                   | 拆分 `src/styles/global.css`(1001 行) 按主题/基础/特效分文件；拆分超大测试文件（`pluginInstallTransaction.test.ts` 873 行等）；确认 PluginManager <600 行、renderer-rpc 缩容达标；docs/ 终稿（AGENTS 单页、6 份活文档、history 归档核对）                                                                         | 全门禁 + format/lint 全绿                            |
| **1.7.3+** | 维护线（按需，不空发）           | bugfix、`npm audit` 依赖更新、安全补丁（含 Electron 小版本升级，走独立版本，不与其他重构混发）；无内容则跳过                                                                                                                                                                                                      | 全门禁 + 相关 smoke                                  |

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
2. **共享 API 类型依赖锁**：抽包会触发 6 插件 dist 重建 → unienv digest 变化 → 必须先有 digest 脚本（update-trusted-policy.mjs）。故 openbox-plugin-api 与 digest 脚本化同批（1.5.25），不放入 1.5.24。
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

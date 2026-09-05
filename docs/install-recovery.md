# CrucibleBox 安装事务与崩溃恢复

> 当前规范（汇总 adr-0002 / adr-0010 / adr-0017；ADR 原文保留作决策溯源）。
> 目标：明确安装/升级/卸载的**显式状态机**、journal 恢复规则与崩溃恢复策略，作为单人维护时的单一事实源。

## 1. 事务状态机（已实现，1.5.26 收敛）

```
prepared ──► awaiting-confirmation ──► staged ──► stopping-old ──► applied ──► committed
   │                │                    │                              │
   └── discard      └── cancel           └── rollback ←──────────────────┘
                                         └── (崩溃) ──► recovery-required
```

状态机定义在 `plugin-system/runtime/InstallTransactionStateMachine.ts`（`INSTALL_TRANSACTION_TRANSITIONS` 转移表 + `assertInstallTransactionStateTransition` 校验）；`PluginInstallationService` 的 install/upgrade/uninstall 编排逐节点调用该校验（非法转移即抛错，fail-closed）。

| 状态                    | 含义                                                       | 进入条件         | 出口                                               |
| ----------------------- | ---------------------------------------------------------- | ---------------- | -------------------------------------------------- |
| `prepared`              | 已从源（ZIP/目录）读取并验证 manifest（不落盘安装）        | `previewInstall` | `commitPreparedInstall` / `discardPreparedInstall` |
| `awaiting-confirmation` | 用户已见权限/版本差异，确认后提交同一不可变 stage token    | 用户确认         | `commitPreparedInstall`                            |
| `staged`                | 完整复制到同卷 staging，Revalidate（manifest 未变）        | stage 完成       | swap                                               |
| `stopping-old`          | 停旧 runtime（single-flight，等真实 exit）                 | stage 通过       | swap                                               |
| `applied`               | target→backup；stage→target 原子替换 + 写元数据 + 健康确认 | swap 完成        | commit 清理                                        |
| `committed`             | 提交完成，清理 backup/journal                              | 健康确认通过     | 完成                                               |
| `recovery-required`     | 崩溃遗留歧义状态                                           | 任意步骤崩溃     | 启动恢复器按 §3 处理                               |

**状态真源映射**（编排状态 ↔ 持久化/内存标记，供单人维护对照）：

| 状态                | journal phase | 目录 artifact     | DB 行 | 内存标记                     |
| ------------------- | ------------- | ----------------- | ----- | ---------------------------- |
| `prepared`          | prepared      | stage / target    | —     | —                            |
| `awaiting-confirm`  | —             | stage             | —     | `preparedInstalls` token     |
| `staged`            | prepared      | stage             | —     | —                            |
| `stopping-old`      | prepared      | stage + target    | 旧行  | `stopRuntime` 进行中         |
| `applied`           | applied       | target (+ backup) | 新行  | —                            |
| `committed`         | committed     | —                 | 新行  | —                            |
| `recovery-required` | 任意          | 任意              | 任意  | `recoveryBlockedPluginNames` |

> 注意：持久化 journal 刻意只保留 3 个崩溃窗口相位（prepared/applied/committed）——那是启动恢复推导的正确粒度（§3）。`awaiting-confirmation` 不跨崩溃（内存 token，15 分钟 TTL）。

**核心不变式**：

- FS 交换与 DB 无法同一 ACID 事务 → **backup 在健康启动前始终保留**；
- 失败先停新 runtime → 恢复目录与旧元数据 → 恢复旧 runtime（补偿事务）；
- 清理失败只留可恢复垃圾，不得误报失败；
- 升级/卸载/停用/配置写入由**维护租约**协调，fail-closed。

## 2. 事务边界（哪些可回滚，哪些不可）

- **可回滚**：代码目录、manifest、宿主元数据、宿主管理配置。
- **不可回滚**：插件在 `activate` 内直接创建的自建表/文件——SDK v2 禁隐式启动迁移，数据迁移必须走宿主版本化幂等事务。
- 升级不改 `config_data`；回滚恢复原版本/入口/权限/config schema；`enabled` 以用户最后操作为准。

## 3. Journal 恢复规则（启动恢复器，幂等）

交换前宿主写保留的 journal（候选包不得自带该文件）。Manager 只扫严格命名直接子目录、不跟随 symlink。

| 现场状态                                                             | 动作                                                                  |
| -------------------------------------------------------------------- | --------------------------------------------------------------------- |
| 未提交升级（journal 指示 staged/applied 中途）                       | 恢复 backup + 旧元数据 + 移除候选                                     |
| 已提交 + backup 同时存在                                             | 核对元数据指向新版后仅清理 backup                                     |
| 多个升级 journal，但 target 有唯一较新 journal 且与 DB/manifest 一致 | 清理更早的 stage/backup，完成 target journal 收敛                     |
| 无 journal 的 backup + target 并存                                   | **歧义 → 双保留并阻止激活，绝不猜测删除**                             |
| 有目录但无 DB 记录                                                   | 拒绝覆盖，留待人工恢复                                                |
| 卸载中断（quarantine）                                               | DB 删除失败 → 原位恢复；删除成功 → 提交清理，清理失败由启动恢复器继续 |

> 多个 journal 只有在上述“唯一较新 target + DB/manifest 一致 + 更早事务均为升级残留”
> 条件同时满足时才会自动收敛；顺序无法判定、操作类型混杂或元数据不一致时仍保持
> `recoveryBlockedPluginNames` 阻断，等待人工处理。

## 4. 崩溃恢复（runtime 层，可靠性边界）

- 后台恢复并发默认 2、最大 8；单插件 30s 超时不阻塞 UI。
- backend RPC 超时 = 失败 runtime：拒绝请求 → 强制终止 → 归类意外退出 → 交由 supervisor 恢复。
- 指数退避 1s → 5s → 最大 30s。
- 隔离（quarantine）：5 分钟内 3 次崩溃 → 内存中 fail-closed 并持久化 disabled；用户显式重新启用清除崩溃历史。
- 维护/停用/卸载/关机优先于自动重启；关机取消恢复计时器并等待所有 runtime 退出（utility process 非独立 daemon）。

## 5. UniEnv 中断安装恢复（可信服务域）

- 背景：普通失败/取消会清 staging；崩溃会遗留 `.unienv-staging-*`；任务注册表是内存态，**不宣称可安全续传**。
- 恢复决策：激活时只枚举严格目录表派生的版本根；只删名为 `.unienv-staging-*` 的直接子目录（须普通且非 symlink）；**绝不检查/删除已安装 runtime 兄弟**；以原子提升完成的结果为权威。
- fail-closed：恢复或配置校验失败 → 保留原因，install/combo/uninstall/version switch 全部关闭，直到服务以有效状态重启；只读列表与检测仍安全可用。
- 不尝试续传任意安装器进程；新任务从干净 staging 开始并重验固定制品（官方 URL + SHA-256）。

## 6. 数据兼容（schema v3）

- schema v3：`plugins`（含 `sort_order`）/`settings`/`plugin_logs`/`plugin_storage`（含迁移标记）。
- 迁移用 `PRAGMA user_version` + `MIGRATIONS` 数组 + `BEGIN IMMEDIATE`；失败回滚并保留旧 `user_version`。
- 启动迁移后清理 30 天前 `plugin_logs`。
- 旧表迁移（diary_entries/turntable_items）只复制不删除，marker 防旧数据复活；未安装旧插件在首次激活执行同一幂等事务。
- 数据库 schema 变更 = 强制 minor 版本 + `smoke-packaged`（旧库迁移）+ `release-compatibility`（previous→candidate 回滚）双验证。

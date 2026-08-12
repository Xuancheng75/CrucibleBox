# ADR-0007：插件私有存储与兼容迁移

- 状态：Accepted
- 日期：2026-08-10

## 背景

旧版 SDK 把宿主 SQLite 的原始 `query/execute` 暴露给插件。Diary 与 Turntable 因而创建全局表，
插件之间没有数据命名空间，表结构升级也无法由宿主统一编排。权限检查只能决定是否允许整库 SQL，
不能限制插件实际读取或修改的表。

## 决策

SDK 新增 `ctx.storage` JSON KV API：`get/set/delete/list`。宿主始终以当前插件 ID 作为命名空间，
worker RPC 不接受 `pluginId`，因此插件不能选择或伪造其他命名空间。读写分别要求
`storage:read` 与 `storage:write`。

存储键最长 256 字符并拒绝控制字符；值必须是有限、无环的 JSON，单值上限 1 MiB。数据库 schema v2
新增 `plugin_storage` 和迁移标记表。Diary 的旧行映射为 `entry:<date>`，Turntable 的有序行映射为
`items` 数组。

schema 升级在 `BEGIN IMMEDIATE` 事务中执行。旧表只复制、不删除；`INSERT OR IGNORE` 保留已存在的
新格式值。迁移标记防止用户在新格式中删除数据后，旧表在下次启动重新注入。对数据库升级时尚未安装
的旧插件，首次激活还会在独立事务中执行同一幂等迁移。

## 兼容策略

原始 `ctx.database` 与 `database:*` 权限暂时保留给 API v1 和既有第三方插件，但标记为兼容接口。
六个生产插件中，Diary 与 Turntable 已迁入私有存储；其余插件不使用原始 SQL。未来删除兼容接口前，
需要发布独立的 SDK 主版本和迁移期。

## 后果

- 生产插件不能再访问宿主或其他插件的数据。
- 插件不再负责 DDL，宿主可以对 schema 和备份策略集中治理。
- KV API 不提供跨键事务；需要复杂关系模型的插件必须先提出版本化数据模型，由宿主审查迁移方案。
- 回滚旧插件时原表仍在，但新格式期间产生的数据不会反向写入旧表；降级前应导出或使用受控转换工具。

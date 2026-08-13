# M2.5 验收：插件数据命名空间

## 交付范围

- 宿主数据库 schema v2：`plugin_storage`、迁移标记、事务化 schema 升级。
- backend RPC v2：严格校验的 `storage.get/set/delete/list`。
- SDK：`PluginStorageAPI`、`storage:read`、`storage:write`，模板与六插件声明同步。
- Diary 0.4.9 与 Turntable 0.1.8 改用私有存储，不再声明原始数据库权限。
- 旧 `diary_entries`、`turntable_items` 向新格式幂等复制，旧表保留。

## 数据保证

迁移以插件 ID 命名空间，不接收插件提供的 namespace。schema 版本与数据复制在同一事务提交；任一步失败
都会回滚并保留旧 `user_version`。新格式已有键不会被旧表覆盖，迁移标记阻止已删除内容被再次导入。

## 验证

`tests/pluginStorage.test.ts` 使用真实 sql.js 内存数据库覆盖旧表复制、schema 版本、旧表保留、命名空间
隔离、幂等标记和失败回滚。`tests/pluginStorageConsumers.test.ts` 覆盖 Diary 保存/查询/导出/删除与
Turntable CRUD/排序/加权抽奖，并让原始数据库 API 在任何调用时直接失败。

完整发布门禁与最终制品摘要记录在 `docs/development.md`。

## 验收结果

- 主工程 1.5.5；Diary 0.4.9；Turntable 0.1.8。
- 宿主 24 个 Vitest 文件、160 项；GIF Editor 35 项；UniEnv 116 项；供应链 3 项。
- 根工程与六插件 npm audit 均为 0 漏洞。
- typecheck、零 warning lint、测试、生产 build、体积预算、确定性插件 ZIP、摘要验证、七份 SBOM、
  Electron unpacked package 全部通过。
- 打包态冒烟使用临时 userData，耗时 1,087 ms，总 working set 451,412 KiB。

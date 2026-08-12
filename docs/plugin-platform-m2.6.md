# M2.6 验收：数据库失败关闭与打包态迁移恢复

## 目标

M2.5 已通过内存数据库验证数据迁移，但发布门禁仍只用全新 userData 启动打包应用。M2.6 将旧数据
备份、better-sqlite3 实际迁移和旧表保留加入 Windows packaged smoke，并禁止数据库初始化失败后进入
PluginManager 或创建主窗口。

## 行为

- better-sqlite3/sql.js 引擎创建失败会向调用方抛错，不再返回 `null`。
- schema migration 失败时先回滚事务，再关闭并清空全局数据库引用，宿主不能继续读写半初始化数据库。
- 主进程记录 fatal diagnostic，显示错误框，设置非零退出码并在创建 PluginManager/窗口前退出。
- packaged smoke 使用 sql.js 生成真实 schema v1 文件，包含 Diary 与 Turntable 旧表和 Unicode 数据。
- 启动前原数据库必须形成逐字节 `.bak-sqljs`；退出后验证 `user_version=2`、两个 namespace 的 JSON、
  两个迁移 marker 和旧表仍然存在。

## 验证

- `tests/pluginStorage.test.ts` 注入不兼容 schema，验证初始化 reject、全局引用清空、版本保持为 1、旧行
  保留且事务中创建的 marker 表不存在。
- Windows unpacked 应用完成真实 better-sqlite3 迁移、renderer iframe、两个 utility backend、UniEnv
  可信服务和性能门禁。
- 打包态结果：1,375 ms，452,628 KiB working set。

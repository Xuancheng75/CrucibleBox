# 数据库引擎评估：sql.js → better-sqlite3

> P7 交付物之一。结论先行：**建议迁移，但单独成阶段（P9），不并入本里程碑**。
> 当前仍使用 sql.js。本文记录对比结论、迁移成本与前置条件，供后续阶段决策。
> **状态：P9 已完成**——双引擎（better/sqljs A/B）已落地并验证，默认引擎为 better-sqlite3（WAL），`OPENBOX_DB_ENGINE=sqljs` 可回退。

## 1. 现状

- `database/index.ts`：sql.js（WebAssembly SQLite）内存数据库 + 防抖 500ms 全量 `export()` 写文件（`*.tmp` + `renameSync` 原子替换）。
- 引擎完全驻留在 Electron 主进程；插件（P7 起）通过子进程 RPC 访问主进程的 `queryAll/execute`，同步 SQL 对主进程事件循环仍有阻塞窗口。
- 已交付的可靠性设施：`PRAGMA user_version` 版本化迁移、`will-quit` 兜底落盘、防抖合并写。

## 2. 对比维度

| 维度 | sql.js（现状） | better-sqlite3 |
|---|---|---|
| 运行形态 | WASM，纯内存，每次 `export()` 整库序列化 | 原生 Node 扩展，直接读写磁盘文件 |
| 并发 | 引入进程锁+手动原子写，仍是全量覆盖 | WAL 模式，读写并发、崩溃安全 |
| 性能 | 全库序列化为目标文件，库越大越慢 | 只有脏页写盘 |
| 事务 | 手动 | `db.transaction()` 原子批处理，批量写提速巨大（Turntable reorder 循环） |
| 崩溃安全 | export 序列化窗口内崩溃丢数据 | SQLite 自身 ACID |
| 内存 | wasm 二进制 + 全库镜像在堆中 | 默认 page cache，可控 |
| 打包 | `sql-wasm.wasm` 需随产物打包（AGENTS 已列工作项） | native `.node` 需按 Electron ABI 预编译随包 |
| Electron 集成 | 纯 JS/WASM，零 prebuild | 需 `@electron/rebuild`（或 prebuild-install 预编译）对齐 ABI；`externalizeDepsPlugin` 处理 |
| 插件兼容 | 0（当前插件只用 query/execute） | 0（保持仓储层 API 不变即可） |

结论：better-sqlite3 在**正确性（WAL/事务/崩溃安全）与扩展性（大库、并发）**上全面优于 sql.js；主要成本集中在 native 依赖的 Electron 环境构建与打包。

## 3. 迁移成本清单（若执行）

1. **依赖**：`better-sqlite3` 加入主进程 dependencies；`electron-builder` 配置 `asarUnpack` 放行 `node_modules/better-sqlite3/**`；
2. **加载**：初始化改为 `new Database(dbPath)`（同步打开），文件不存在自动建库——流程比当前更短；
3. **迁移兼容**：现有 `openbox.db` 是 sql.js 导出的完整 SQLite 文件，better-sqlite3 直接可打开，无需数据搬迁；
4. **仓储层不动**：`database/repositories/*` 已在调用 `queryAll/queryOne/execute`，仅改 `database/index.ts` 的这四个函数实现体；
5. **原子写/防抖**：删除 `persistDatabase` 防抖与 `saveDatabase` 原子写代码（不再需要），`will-quit` 只需 `db.close()`；
6. **async 插件 API 保持不变**：P7 已在子进程协议上异步化，主进程内部同步即可；
7. **CI/冒烟**：typecheck/lint/test 之外，用 `npm exec electron-rebuild` 验证 ABI；打包产物实机验证。

## 3. 前置条件 / 风险

- **native rebuild**：`better-sqlite3` 需与 Electron ABI 匹配。开发机 vs 打包机的 Node/Electron 版本需一致，或依赖 prebuild（Electron 常见版本有预编译 binary，但也预案 rebuild）。
- **允许脚本策略**：本机 allow-scripts 曾拦截 electron/esbuild postinstall；better-sqlite3 的 prebuild/编译脚本同样受此约束，需显式放行或本地编译（P5 注记已遇到）。
- **回归面**：数据库是插件数据（Diary 日记、Turntable 选项）的唯一持久化，迁移需保留一次原库字节级备份。
- **时区/函数差异**：sql.js 与 SQLite 都支持 `datetime('now','localtime')`（当前表结构大量使用），语义一致，无需改 SQL。

## 4. 建议

- 单独设阶段（P9）：先为 better-sqlite3 + electron-rebuild 打通一条 CI 验证路径，再替换实现；数据迁移放最后一步。
- 保留当前 sql.js 分支直到双跑验证通过（可用环境变量选择引擎做 A/B）。
- 现 P7 的插件异步数据库接口与子进程协议均与引擎无关，先行落地不会造成返工。

## 5. P9 落地记录（2026-08）

- **依赖**：better-sqlite3 `^12.11.1`（engines 覆盖 Node 20.x）+ `@types/better-sqlite3@^9.6.0`；`@electron/rebuild@^4.2.0` 入 devDependencies。
- **ABI 实测结论（重要）**：better-sqlite3 12.x **没有 N-API 通用 prebuild，是 per-ABI V8 prebuild**。13.x 的 prebuild 要求 Node≥22（NAPI 版本过高），在 Electron 34（Node 20.19 / NAPI 9）下 `new Database()` 直接崩溃（exit -36861）。**Electron 环境必须 `electron-rebuild` 编译 Electron ABI（132）**；prebuild-install 默认下载的是 node-v137（Node 24）产物，不能被 Electron 加载。
- **构建自动化**：`scripts/rebuild-native.js` 挂 postinstall（CI 环境跳过；必须以项目根为 cwd），保证 `npm install` 后 Electron 可加载。
- **双引擎**：`database/index.ts` 抽 `EngineDb` 接口 + `BetterEngine`/`SqlJsEngine`，`OPENBOX_DB_ENGINE=sqljs` A/B 切换；仓储层与插件 RPC 零改动。
- **数据迁移**：better 直接打开既有 `openbox.db`（sql.js 导出即完整 SQLite），启动时先字节级备份 `.bak-sqljs`；`PRAGMA user_version` 迁移与 30 天日志清理两引擎同路径。
- **验证**：better/sqljs 双引擎 dev 冒烟通过（引擎初始化 + 5 插件激活 + stderr 0 错误）；typecheck/lint/vitest 28 用例/build 全绿。
- **打包**：`asarUnpack: node_modules/better-sqlite3/**`（electron-builder 打包时自动 rebuild native）。
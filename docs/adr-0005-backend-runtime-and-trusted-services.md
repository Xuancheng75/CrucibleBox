# ADR-0005：插件 backend v2、utility process 与可信宿主服务

- 状态：Accepted
- 日期：2026-08-10
- 里程碑：M2.3

## 背景

旧 backend 通过 `child_process.fork` 运行并继承完整 `process.env`。插件可直接加载 Node
内置模块，因此经 `PluginContext` 执行的权限检查只能约束 SDK 调用，不能约束恶意 Node
代码。UniEnv 又确实需要文件、下载、解压和启动安装器，不能把这些粗粒度能力授予通用插件。

## 决策

1. backend 传输统一为 Electron `utilityProcess.fork`，删除进程内回退和完整环境继承。
2. 主进程与 worker 只交换 backend RPC v2 信封；每个运行实例使用随机 token，请求 ID、方法、
   精确字段、JSON 预算和最多 64 个并发请求均在分派前校验。
3. worker 环境仅保留运行所需的操作系统临时目录、区域和时区字段；不传递 `HOME`、云凭证、
   完整 `PATH` 等宿主环境。
4. `utilityProcess` 提供故障隔离、命名进程、受控退出和未来指标入口，但不是恶意代码安全沙箱。
   安装的任意 Node backend 仍属于可信代码；权限声明是 SDK 能力门控。
5. UniEnv 的进程、文件、下载、解压和 junction 实现编译进宿主的可信服务。插件包只保留 778
   字节左右的代理 main 与 renderer。`trusted:unienv` 必须同时满足名称、版本、API 版本、权限、
   完整运行文件集合和固定 SHA-256 摘要，任一字节变化都拒绝激活。

## 取舍

- 相比 `child_process`，utility process 与 Electron 生命周期和崩溃事件一致，且不会意外泄露完整
  环境；迁移成本可控。
- Node 24 permission model 不覆盖网络，也明确不是恶意代码边界，因此不作为授权依据。
- 若未来允许不可信 backend，必须改为 Chromium sandbox/OS sandbox 中的无 Node runtime，或取消
  任意 backend 代码；不能依赖本 ADR 的纵深控制宣称安全隔离。
- 固定摘要是第一方能力绑定，不替代发布签名。签名、SBOM 和更新源验证属于后续供应链里程碑。

## 兼容性

缺少 `backendApiVersion` 的旧 manifest 仍按 v1 SDK 语义加载，但传输层始终使用 v2。新模板和六个
生产插件均声明 `backendApiVersion: 2`。本阶段不修改用户数据库 schema 或插件配置格式。

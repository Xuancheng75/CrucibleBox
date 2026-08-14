# P2 Spike: quickjs-ng 加载插件 backend（1.8.2）

> 状态：2026-08-14。**结论：d1 方案（Rust sidecar + 内嵌 quickjs-ng）可行性已验证**——同步往返模型 + job-drain 即可，无需 AsyncRuntime 复杂桥接。

## 1. 验证了什么

工程 `poc-backend-sidecar/`：独立 Rust crate，用 rquickjs 0.12.2（quickjs-ng v0.15.1）加载**已发布的插件 dist 产物**（非源码重编译），注入最小 ctx（logger/storage 内存实现）+ console + crypto polyfill，验证 activate/onMessage/deactivate 同步往返形状。

| 插件 | 构建流派 | 结果 |
|---|---|---|
| gif-editor（esbuild 单文件 CJS stub） | `module.exports = __toCommonJS(...)` → `{default: plugin}` | ✅ 全通：activate(sync, 1 log) → onMessage(ping) `{"ok":true}` → deactivate → storage 往返 |
| diary（esbuild 单文件 CJS 真业务，async onMessage） | 同上 | ✅ 全通：**async onMessage 经 `Promise::finish` 返回真实结果** `{"error":"未知消息类型: ping"}`（与 backend 代码 default case 逐字一致） |
| turntable（tsc 多文件 CJS，相对 require） | `exports.default = plugin` + `require("./turntable-domain")` | ⚠️ 部分通过：activate/onMessage 形状 OK，但相对 require 被 stub 返回 undefined——**未触及 domain 模块的真实加载**（假象 PASS） |

## 2. 关键结论

1. **esbuild 派（diary/gif-editor/theme-manager）在 quickjs-ng 直接可行**：CJS shim（module/exports globals + eval）+ `Promise::finish` 驱动 async 路径，返回值与 Electron 契约一致。
2. **同步往返模型成立**：`ctx.with` + `execute_pending_job` 排空即可解析同步 settle 的 Promise；**不需要 AsyncRuntime/tokio 桥接**（避开了 oracle 判定的最大工程风险）。前提：插件方法不依赖宿主计时器/异步 I/O 驱动（现有 6 插件均满足——backend 零 setTimeout/零 Node builtin）。
3. **polyfill 面可控**：console→stderr、crypto.getRandomValues（spike 用 Math.random，真实实现换 getrandom/ring）、storage 内存 map。api.fetch 的 Response/Headers、readFile 的 Buffer 解码属 ctx 复刻项（1.8.2 完整实现时注入）。
4. **tsc 派需要真实 CommonJS loader**：turntable/unienv/dice-roller 有目录内相对 `require('./xxx')`。完整实现需自定义 require 解析（沿 Electron 的 resolvePluginMain 语义，目录内相对路径、防逃逸）。这是 1.8.2 完整实现的工作项，非方案障碍。

## 3. 门禁对照（oracle P2 8 项）

| # | 门禁项 | Spike 状态 |
|---|---|---|
| 1 | 已发布 dist/main.js 跑通 activate/deactivate/onMessage，形状一致 | ✅ esbuild 派验证；tsc 派形状 OK（loader 待补） |
| 2 | SDK 方法面 conformance（storage 全方法/logger/api/trusted） | ⏳ spike 覆盖 storage.get/set/delete + logger；全量在完整实现 |
| 3 | 信封 v2 校验移植 + 差分测试 | ⏳ 完整实现项 |
| 4 | 故障隔离：kill → 宿主无恙、backoff 恢复、3 次禁用 | ⏳ 完整实现项（纯策略已存在于 Rust 宿主侧） |
| 5 | 超时强杀 | ⏳ 完整实现项 |
| 6 | 并发正确性（diary/turntable 延迟存储测试） | ⏳ 完整实现项 |
| 7 | 内存/体积/启动延迟 | ⏳ 1.8.4 打包复测 |
| 8 | 引擎 conformance 矩阵（6 插件特性扫描） | ⏳ 完整实现项 |

## 4. 下一步（1.8.2 完整实现）

1. `cruciblebox-plugin-host` sidecar 二进制：真实 CommonJS loader（相对 require + 防逃逸）+ 帧协议（stdin/stdout 长度前缀帧）+ ctx 全量注入（storage/logger/database/api，RPC 转发宿主）+ crypto(getrandom)/Response polyfill。
2. Rust 宿主侧：spawn/停止/崩溃恢复/超时策略（复用 1.8.1 已建 policy 思路）+ 信封 v2 校验移植。
3. P2 门禁 8 项验收 → 合入后 1.8.3 开始 renderer 隔离迁移。

# cruciblebox-plugin-host（1.8.2）

插件 backend sidecar：用 quickjs-ng 在独立进程内运行插件 backend（纯 JS，零 Node builtin）。

## 架构

```
宿主 (Rust core) ── stdin/stdout 帧协议 ──► cruciblebox-plugin-host
    │ 信封 v2 (length-prefixed JSON)          │ rquickjs (quickjs-ng)
    │  worker: initialize/plugin.message/     │ 加载插件 dist/main.js (CJS)
    │          dispose                         │ 注入 ctx (logger/database/storage/api)
    ◄── host: db.*/storage.*/log.write/...  ──│ ctx 方法经 __hostRequest 同步往返
```

- **帧协议**（frame.rs）：4 字节大端长度 + UTF-8 JSON；MAX 8MB。
- **信封 v2**（envelope.rs）：token/requestId 正则、WORKER_METHODS 4 个、HOST_METHODS 19 个、payload 预算（256KB/深度16/数组512/对象键256/字符串64KB）、响应必带 token。
- **CJS loader**（loader.rs）：支持 esbuild 单文件 CJS（`module.exports.default`）与 tsc 多文件 CJS（`exports.default` + 相对 `require('./x')`）；绝对路径/相对 `..` 均须落在 plugin_dir 内；`__cjsLoad` 二次根校验（防任意读盘）。
- **ctx 注入**（ctx.rs）：后台读线程独占 stdin → channel；主循环与 `__hostRequest` 同步往返共享队列（`recv_timeout(30s)` 超时、不匹配帧入 pending 缓冲不丢失）；出站 method 白名单 + payload 预算。
- **生命周期**（main.rs）：`lifecycle.initialize`（加载→buildCtx→activate）/ `plugin.message`（onMessage，async 经 Promise::finish）/ `lifecycle.dispose`（deactivate→退出）。apiVersion 闸门（v2 only）。

## 使用

```
cargo build --release
cruciblebox-plugin-host.exe <pluginDir> <mainEntry> <apiVersion> <token>
```

token：32-128 位 `[A-Za-z0-9_-]`，宿主生成经 argv 传入（同用户进程可见，设计目标是防渲染进程伪造，非 OS 机密；宿主侧逐调用校验是真正边界）。

## 验证

- 16 项单元测试（帧/信封/loader/ctx）：`cargo test`
- 3 个真实插件 dist 产物端到端（`node host-sim.mjs`）：gif-editor（esbuild stub）、diary（esbuild 真业务 async onMessage）、turntable（tsc 多文件相对 require）——initialize/plugin.message/dispose 全部 PASS，async onMessage 返回值与 backend 代码逐字一致。

## 信任边界（重要）

本 sidecar 的隔离 = quickjs 无 fs/net + 单一 stdin/stdout 管道 + 路径/信封校验。**它不是安全沙箱**：插件 backend 仍是安装时用户明确信任的代码。真正的安全边界 = 插件安装信任决策 + 宿主侧 PermissionGuard（逐方法校验，宿主集成时落地，sidecar 侧仅有白名单/预算最小防护）。恶意插件仍可经 `__hostRequest` 调用白名单内方法（如 db.query），宿主权限守卫不可省略。

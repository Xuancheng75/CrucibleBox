# Document Engine 架构分析（历史参考）

> 本文档为 Document Engine 插件开发的 Phase 0 产物。
> 对应需求文档 `OCR.md` 第四阶段（架构分析）+ 第六十三阶段（最终报告 - 架构部分）。
> 实际源码参考：`E:\CrucibleBox_Sourses`。
> 当前实现以 `document-engine-v2.md` 为准：OCR Worker 使用 Rust + PaddleOCR ONNX，Python Worker 方案已废弃。

## 1. CrucibleBox 当前技术栈

| 项目 | 版本 / 说明 |
|------|------------|
| 运行框架 | **Tauri 2.11.x**（Electron 线已于 1.9.2 冻结归档） |
| 语言 | Rust（主进程 + 插件 backend sidecar） |
| 前端 | React 19 + Ant Design 6 + zustand |
| 数据库 | rusqlite bundled（WAL 模式，schema v3→v4） |
| 插件 backend | quickjs-ng sidecar（`cruciblebox-plugin-host` crate），帧协议 v2 |
| 插件 renderer | 跨源 sandboxed iframe + MessagePort RPC v2 |
| 构建 | electron-vite（前端）/ cargo（Rust）/ esbuild（插件自包含构建） |
| 打包 | NSIS（Windows x64） |

## 2. 插件模型（Manifest v2）

插件是一个**自包含工程**，宿主只消费：

```
plugin.json          # manifestVersion 2
dist/main.js         # backend bundle (CJS, quickjs-ng)
dist/renderer.js     # frontend bundle (browser IIFE)
```

插件生命周期（`packages/cruciblebox-plugin-api/src/index.ts`）：

```ts
interface PluginMain {
  activate(ctx: PluginContext): void | Promise<void>
  deactivate(): void | Promise<void>
  onMessage?(message: unknown): unknown | Promise<unknown>
}

interface PluginRenderProps {
  config: PluginConfig
  onConfigChange: (config: PluginConfig) => void
  theme?: Theme
  api: {
    sendToBackend(message: unknown): Promise<unknown>
    notify(title: string, body?: string): void
    confirm(options: {...}): Promise<boolean>
    onBackendMessage(handler: (msg: unknown) => void): () => void
    theme: {...}
  }
}
```

通信链：

```
renderer.tsx                         main.ts (sidecar quickjs-ng)
    │ sendToBackend(msg)                  │ onMessage(msg)
    │                                     │
    └──────── MessagePort ──────────────► │
                                  ctx.api.invokeTrustedService('document-engine', ...)
                                              │
                                              ▼
                                   envelope_host::host_dispatch
                                              │
                                              ▼
                              document_engine_service::dispatch (Rust)
```

## 3. 为什么是 Trusted Service

需求要求 Document Engine 具备以下能力，超出普通插件权限边界：

- 启动并管理 **Python 子进程**（PaddleOCR / MinerU）
- 长时间运行的重量级 AI 任务（OCR / PDF 解析 / 转换）
- 文件系统深度访问（模型目录、缓存目录、批量处理）
- GPU / CUDA 资源调度

CrucibleBox 已有的 **UniEnv** 正是此模式：宿主固定摘要可信服务，Rust 侧实现，
经 `invokeTrustedService('unienv', ...)` 调用。Document Engine 复用同一机制：

- `plugin.json` 声明 `"permissions": ["trusted:document-engine"]`
- `main.ts` 的 `activate` 调用 `ctx.api.invokeTrustedService!('document-engine', 'activate')`
- Rust 侧 `document_engine_service::dispatch` 处理 `message` 操作

权限模型（信任模型 A）：`trusted:*` 权限是宿主固定第一方实现专属门控，
非安全边界，但宿主侧 `PermissionGuard` 是唯一权威断言点。

## 4. 统一分发器设计

当前 `envelope_host.rs` 的 `trusted.invoke` 分支硬编码了 `unienv_service::dispatch`：

```rust
// 现有（修改前）
"trusted.invoke" => {
    crate::unienv_service::dispatch(&db, plugin_id, &service, &operation, payload)
}
```

改为按 `service` 参数路由的**统一分发器**：

```rust
"trusted.invoke" => {
    let service = params
        .get("service")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match service.as_str() {
        "unienv" => {
            crate::unienv_service::dispatch(&db, plugin_id, &service, &operation, payload)
        }
        "document-engine" => {
            crate::document_engine_service::dispatch(&db, plugin_id, &operation, payload)
        }
        _ => Err(format!("unknown trusted service: {service}")),
    }
}
```

`document_engine_service::dispatch` 签名与 `unienv_service::dispatch` 对齐：

```rust
pub fn dispatch(
    db: &Db,
    plugin_id: &str,
    operation: &str,
    payload: Option<&Value>,
) -> Result<Value, String>
```

> 注意：`document-engine` 不需要 `service` 参数透传（自身已是目标服务），
> 但 `unienv` 的旧签名保留 `service` 以兼容已有调用。

## 5. Python 运行时策略（已确认：检测系统 Python）

Document Engine 不自带 Python，而是**检测系统 PATH 上的 Python**：

1. 调用 `python --version` / `python3 --version` 探测
2. 要求 Python ≥ 3.10（PaddleOCR / MinerU 最低要求）
3. 检测 `pip` / `venv` 可用性
4. 检测 CUDA：运行 `nvidia-smi` 或检查 `torch.cuda.is_available()`
5. 模型与依赖安装到可配置目录（默认 `%APPDATA%\cruciblebox\document-engine\`）

`getStatus` API 返回：

```json
{
  "python": { "available": true, "version": "3.11.9", "path": "C:\\..." },
  "cuda":   { "available": true, "device": "NVIDIA GeForce RTX 3060" },
  "paddleocr": { "installed": false, "version": null },
  "mineru":     { "installed": false, "version": null },
  "workers": { "ocr": 0, "parser": 0, "converter": 0 }
}
```

## 6. Worker 隔离架构

重量级 Python AI 不运行在 Tauri 主进程。架构：

```
Tauri/Rust (主进程)
    │
    ├── document_engine_service (trusted service, 请求校验 + 任务调度)
    │        │
    │        └── TaskManager (多 resource key: ocr/parse/chunk/convert/batch)
    │
    └── Worker Manager (spawn Python 子进程)
             │
        ┌────┴─────┐
        ▼          ▼
   PaddleOCR   MinerU
   Worker      Worker
   (Python)    (Python)
```

Worker 通过 **stdin/stdout JSON 行协议** 与 Rust 通信（详见 `document-engine-worker-protocol.md`）。

初始 Worker 数（OCR/MinerU 各 1，依据显存动态调整）：

```rust
const DEFAULT_OCR_WORKERS: usize = 1;
const DEFAULT_MINERU_WORKERS: usize = 1;
```

## 7. 目录规划

```
E:\CrucibleBox_Sourses\
├── plugins\
│   └── document-engine\          # 插件工程（参照 unienv）
│       ├── plugin.json
│       ├── package.json
│       ├── tsconfig.json / tsconfig.build.json
│       ├── scripts\build-plugin-renderer.mjs
│       ├── src\
│       │   ├── main.ts           # backend entry
│       │   ├── renderer.tsx      # UI
│       │   └── renderer-entry.tsx
│       └── tests\
│
└── src-tauri\src\
    ├── document_engine_service.rs   # trusted service dispatch
    └── document_engine_task.rs      # 多 resource key 任务管理

E:\OCR\                        # OCR 上游源码（不耦合进 CrucibleBox）
├── PaddleOCR\
└── MinerU\
```

## 8. 统一数据模型（草案，详见 document-engine-data-model.md）

所有引擎结果统一为：

```
Document → Page[] → Block[]
Chunk[]（基于 Document 切分）
```

前端只调用 `Document Engine API`，不感知 PaddleOCR / MinerU / Python 实现细节。

## 9. 阶段开发顺序（本次 Phase 0-2）

| Phase | 内容 | 状态 |
|-------|------|------|
| 0 | 架构分析 | ✅ 本文档 |
| 1 | OCR 源码准备（克隆 + 环境验证） | 进行中 |
| 2 | Document Engine 插件骨架 + Rust trusted service | 进行中 |
| 3-15 | Analyzer / OCR / MinerU / Model / Chunker / Converter / Job / Cache / UI / 集成 / 测试 / 打包 | 后续 |

Phase 2 骨架实现：
- `getStatus`：返回引擎环境状态
- `document.jobs.*`：任务列表/详情/取消（基础）
- 其余 message 类型返回 `{ error, code: "not-implemented" }`（明确标记，不伪造）

## 10. 参考实现清单

| 参考 | 文件 |
|------|------|
| 插件 manifest 结构 | `plugins/unienv/plugin.json` |
| 插件 backend 入口 | `plugins/unienv/src/main.ts` |
| 插件 renderer UI | `plugins/unienv/src/renderer.tsx` |
| 自包含构建脚本 | `plugins/unienv/scripts/build-plugin-renderer.mjs` |
| Rust trusted service | `src-tauri/src/unienv_service.rs` |
| Rust 任务管理 | `src-tauri/src/unienv_task.rs` |
| 宿主分发器 | `src-tauri/src/envelope_host.rs` |
| 权限守卫 | `src-tauri/src/permissions.rs` |
| 插件 API 契约 | `packages/cruciblebox-plugin-api/src/index.ts` |

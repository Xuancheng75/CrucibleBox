# Document Engine Worker 通信协议（v1）

> 第一开发批次协议，对应桌面 `OCR.md` 的 Rust OCR Worker 架构。
> Worker 是独立 Rust 可执行文件；Python/MinerU 行协议属于历史方案，不再是运行时依赖。

## 1. 通信方式

Rust trusted service 通过 stdin/stdout JSON-lines 与 `ocr-worker.exe` 通信：

- 每行一条 JSON 消息（`\n` 分隔）
- Rust → Worker：通过 stdin 写入请求 JSON
- Worker → Rust：通过 stdout 读取响应/进度 JSON
- 诊断日志只写 stderr；stdout 只允许协议帧

所有请求和响应都带 `protocolVersion: 1` 与 `requestId`。请求行最大 64 KiB。

```text
Rust (document_engine_service)
   │ { "protocolVersion": 1, "task": "ocr", "requestId": "...", ... }
   ▼
Rust OCR Worker (ocr-worker.exe)
   │ { "protocolVersion": 1, "type": "progress", ... }
   │ { "protocolVersion": 1, "type": "result", ... }
   ▼
Rust (解析为 Document JSON → Task snapshot)
```

## 2. 请求帧

```json
{
  "protocolVersion": 1,
  "task": "ocr",
  "requestId": "uuid",
  "input": "C:\\...\\scan.png",
  "options": {
    "language": "ch",
    "device": "cpu",
    "modelDirectory": "C:\\...\\models",
    "dictionaryPath": "C:\\...\\models\\ppocr_keys_v1.txt"
  }
}
```

第一批次支持 `task: ocr`，支持 `device: auto|cpu`。GPU、PDF 和批量任务在后续协议扩展中加入，旧 Worker 必须拒绝未知版本和未知任务。

## 3. 响应帧

### 3.1 进度帧

```json
{
  "protocolVersion": 1,
  "type": "progress",
  "requestId": "uuid",
  "stage": "loading",
  "percent": 5,
  "message": "loading OCR models"
}
```

### 3.2 结果帧

```json
{
  "protocolVersion": 1,
  "type": "result",
  "requestId": "uuid",
  "text": "完整识别文本",
  "blocks": [
    {
      "text": "第一行",
      "polygon": [[0,0],[100,0],[100,30],[0,30]],
      "bbox": [0,0,100,30],
      "confidence": 0.98
    }
  ],
  "model": {
    "detectionSha256": "...",
    "recognitionSha256": "...",
    "dictionarySha256": "...",
    "device": "cpu"
  }
}
```

### 3.3 错误帧

```json
{
  "protocolVersion": 1,
  "type": "error",
  "requestId": "uuid",
  "code": "model-unavailable|invalid-request|ocr-failed|unsupported-protocol",
  "error": "human-readable message"
}
```

### 3.4 检查点帧

检查点属于 PDF/批处理阶段，第一批次不发送。后续扩展必须保留 `requestId` 并携带 `partialDocument`。

## 4. Worker 生命周期

| 事件 | Rust 侧动作 |
|------|------------|
| spawn | 启动 Rust Worker，首次请求加载模型 |
| idle | 保留进程与模型 session（warm start） |
| busy | 处理中，禁止同一 Worker 并发请求 |
| crash | 退出码非零 → 宿主重启 Worker，当前任务失败 |
| model-error | 缺文件、摘要不匹配或 shape 不兼容 → fail-closed |

## 5. 模型与设备

- 模型路径来自请求配置、`OCR_WORKER_MODEL_DIR`，或 Worker 旁的 `models` 目录；禁止机器特定绝对路径。
- 必需文件：`ch_PP-OCRv4_det.onnx`、`ch_PP-OCRv4_rec.onnx`、`ppocr_keys_v1.txt`。
- Worker 返回三个文件的 SHA-256；目录存在 `model-manifest.json` 时必须逐项匹配，否则拒绝加载。
- 第一批次只承诺 CPU；`auto` 在 CPU Worker 中解析为 CPU，显式 `gpu` 返回 `device-unavailable`。
- 模型 session 按目录缓存，连续请求不重复加载模型。

## 6. 取消协议

第一批次的取消由宿主负责终止当前 Worker 进程并丢弃未完成结果，避免伪造“已取消”。后续 Worker 版本可扩展：

```json
{ "protocolVersion": 1, "type": "cancel", "requestId": "uuid" }
```

## 7. 进程隔离与错误隔离

- Worker 是独立 Rust 进程，崩溃不影响 Tauri 主进程。
- stdout 不能混入日志，stderr 由宿主采集。
- 输入文件、图片像素数、请求行和模型文件均有大小限制。
- 批处理阶段单文件失败不能中断整体任务。

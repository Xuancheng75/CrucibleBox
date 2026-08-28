# OCR Worker

`ocr-worker` 是 Document Engine 的第一批次 Rust OCR 可执行文件。它通过 stdin/stdout 交换 JSON-lines，使用 PaddleOCR ONNX 检测/识别模型和 `ppocr_keys_v1.txt` 字典，不依赖 Python。

## 请求

```json
{
  "protocolVersion": 1,
  "requestId": "smoke-1",
  "task": "ocr",
  "input": "C:\\path\\image.png",
  "options": {
    "modelDirectory": "E:\\OCR\\Models",
    "dictionaryPath": "E:\\OCR\\Models\\ppocr_keys_v1.txt",
    "device": "cpu",
    "language": "ch"
  }
}
```

模型目录必须包含：

- `ch_PP-OCRv4_det.onnx`
- `ch_PP-OCRv4_rec.onnx`
- `ppocr_keys_v1.txt`
- 可选 `model-manifest.json`（存在时 SHA-256 必须完全匹配）

未提供 `modelDirectory` 时，Worker 依次使用 `OCR_WORKER_MODEL_DIR`，或可执行文件旁的 `models` 目录。不会使用机器特定的绝对路径。

## 本地验证

```powershell
cargo test --locked
$request = '{"protocolVersion":1,"requestId":"smoke-1","task":"ocr","input":"E:\\CrucibleBox_Sourses\\ocr-worker\\test.png","options":{"modelDirectory":"E:\\OCR\\Models","dictionaryPath":"E:\\OCR\\Models\\ppocr_keys_v1.txt","device":"cpu","language":"ch"}}'
$request | cargo run --locked
```

`test.png` 的固定真值是 `HelloOCR`。Worker 应先输出 `loading` 和 `ocr` 进度帧，再输出包含 `text: "HelloOCR"`、坐标、置信度和模型摘要的结果帧。

第一批次只实现 CPU 单图片 OCR；GPU、PDF、批量、取消、任务持久化和宿主 Tauri 接入属于后续批次。

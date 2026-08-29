// Document Engine trusted host service（Phase 3：OCR Worker 接口整合）
// 运行在宿主 Rust 进程：sidecar 内插件经 api.invokeTrustedService('document-engine', ...)
// → __hostRequest "trusted.invoke"（service="document-engine"）→ envelope_host 路由 → 本模块。
//
// 当前范围：
//   - activate / deactivate：任务管理器与 Worker 生命周期
//   - getStatus：Rust OCR Worker 可用性与模型目录状态
//   - document.analyze：文件分析（已实现）
//   - document.parse：PDF 文本层解析 → Unified Document JSON；扫描页明确路由到 OCR
//   - document.ocr：TaskManager → 常驻 OCR Worker → 结果/进度事件
//   - document.jobs.list / get / cancel：统一任务查询与实际 Worker 终止
//   - 未知 message 类型返回结构化错误；不伪造底层引擎结果

use crate::db::Db;
use crate::document_engine_task::{
    TaskContext, TaskManager, RESOURCE_BATCH, RESOURCE_CHUNK, RESOURCE_CONVERT, RESOURCE_OCR,
    RESOURCE_PARSE,
};
use crate::ocr_worker::{OcrWorkerManager, OcrWorkerRequest};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

fn tasks() -> &'static Arc<TaskManager> {
    static TASKS: OnceLock<Arc<TaskManager>> = OnceLock::new();
    TASKS.get_or_init(|| Arc::new(TaskManager::default()))
}

type Emitter = Arc<dyn Fn(&str, Value) + Send + Sync>;

static OCR_WORKER: OnceLock<Arc<OcrWorkerManager>> = OnceLock::new();
static EVENT_EMITTER: OnceLock<Emitter> = OnceLock::new();
static RETRY_REQUESTS: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
const DEFAULT_MODEL_ID: &str = "ppocrv4-mobile-zh-en";

fn worker_manager() -> Option<&'static Arc<OcrWorkerManager>> {
    OCR_WORKER.get()
}

/// Injected by `main.rs` during Tauri setup.  Keeping the service dispatcher
/// free of an AppHandle also keeps it unit-testable and host-call compatible.
pub fn configure_worker_manager(manager: Arc<OcrWorkerManager>) {
    let _ = OCR_WORKER.set(manager);
}

pub fn configure_emitter(emitter: Emitter) {
    let _ = EVENT_EMITTER.set(emitter);
}

fn emitter() -> Option<&'static Emitter> {
    EVENT_EMITTER.get()
}

fn retry_requests() -> &'static Mutex<HashMap<String, Value>> {
    RETRY_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_retry(task_id: &str, request: &Value) {
    if let Ok(mut requests) = retry_requests().lock() {
        requests.insert(task_id.to_string(), request.clone());
        if requests.len() > 100 {
            let stale = requests.keys().next().cloned();
            if let Some(stale) = stale {
                requests.remove(&stale);
            }
        }
    }
}

fn err(code: &str, message: String) -> Value {
    json!({ "error": message, "code": code })
}

/// Curated model choices shown by the Document Engine UI.
///
/// The worker currently requires this exact PP-OCRv4 triplet.  Keep the
/// catalog static and hash-pinned so a model update is an explicit source
/// change, not an arbitrary JSON/URL fetched at runtime.
fn model_catalog() -> Value {
    json!([
        {
            "id": "ppocrv4-mobile-zh-en",
            "name": "PP-OCRv4 中文/英文标准模型",
            "version": "4.0",
            "description": "适用于中文、英文及混合文档的本地 OCR；包含检测模型、识别模型和中文字符字典。",
            "recommended": true,
            "default": true,
            "offline": true,
            "license": "Apache-2.0",
            "totalBytes": 15568058u64,
            "artifacts": [
                {
                    "name": "ch_PP-OCRv4_det.onnx",
                    "purpose": "文本检测",
                    "bytes": 4729474u64,
                    "sources": [
                        "https://cdn.jsdelivr.net/gh/Xuancheng75/CrucibleBox@main/plugins/document-engine/assets/models/ppocrv4-mobile-zh-en/ch_PP-OCRv4_det.onnx",
                        "https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ch_PP-OCRv4_det.onnx",
                        "https://huggingface.co/anyforge/anyocr/resolve/645af1fbf520b16a1212124d432eac1f4929a561/anyocr/models/anyocr_det_ch_v4_lite.onnx"
                    ],
                    "url": "https://huggingface.co/anyforge/anyocr/resolve/645af1fbf520b16a1212124d432eac1f4929a561/anyocr/models/anyocr_det_ch_v4_lite.onnx",
                    "sha256": "69ce850fec741a2a4568c7c924bb025c9d4f1129e5f96ab428c799ccc5ef2275"
                },
                {
                    "name": "ch_PP-OCRv4_rec.onnx",
                    "purpose": "文本识别",
                    "bytes": 10812334u64,
                    "sources": [
                        "https://cdn.jsdelivr.net/gh/Xuancheng75/CrucibleBox@main/plugins/document-engine/assets/models/ppocrv4-mobile-zh-en/ch_PP-OCRv4_rec.onnx",
                        "https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ch_PP-OCRv4_rec.onnx",
                        "https://huggingface.co/cycloneboy/ch_PP-OCRv4_rec_infer/resolve/5f3c64a6e7a01c45e92c9284318b961bbe51d308/model.onnx"
                    ],
                    "url": "https://huggingface.co/cycloneboy/ch_PP-OCRv4_rec_infer/resolve/5f3c64a6e7a01c45e92c9284318b961bbe51d308/model.onnx",
                    "sha256": "ad7dd55f6759fa02333bff6eb179a4f51be5b89cbe6f710249c95f47d0211350"
                },
                {
                    "name": "ppocr_keys_v1.txt",
                    "purpose": "CTC 字典",
                    "bytes": 26250u64,
                    "sources": [
                        "https://cdn.jsdelivr.net/gh/Xuancheng75/CrucibleBox@main/plugins/document-engine/assets/models/ppocrv4-mobile-zh-en/ppocr_keys_v1.txt",
                        "https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ppocr_keys_v1.txt",
                        "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/main/ppocr/utils/ppocr_keys_v1.txt"
                    ],
                    "url": "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/main/ppocr/utils/ppocr_keys_v1.txt",
                    "sha256": "a1c84d9bdb9ab29043c58896224d32941783eb821629618416dcb08f12886492"
                }
            ]
        }
    ])
}

fn model_catalog_entry(model_id: &str) -> Result<Value, String> {
    model_catalog()
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["id"] == model_id))
        .cloned()
        .ok_or_else(|| "未找到可用的模型包".to_string())
}

fn plugin_install_root(db: &Db, plugin_id: &str) -> Option<PathBuf> {
    db.plugin_backend_record(plugin_id)
        .ok()
        .flatten()
        .map(|record| PathBuf::from(record.installed_path))
        .filter(|path| path.is_dir())
}

fn embedded_model_path(plugin_root: &Path, model_id: &str, name: &str) -> PathBuf {
    plugin_root
        .join("assets")
        .join("models")
        .join(model_id)
        .join(name)
}

fn artifact_sources(artifact: &Value) -> Vec<String> {
    let mut sources = artifact["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if sources.is_empty() {
        if let Some(url) = artifact["url"].as_str() {
            sources.push(url.to_string());
        }
    }
    sources
}

fn model_bundle_status(root: &Path, entry: &Value) -> Value {
    let mut missing = Vec::new();
    let mut invalid = Vec::new();
    let artifacts = entry["artifacts"].as_array().cloned().unwrap_or_default();
    for artifact in artifacts {
        let name = artifact["name"].as_str().unwrap_or_default();
        let expected = artifact["sha256"].as_str().unwrap_or_default();
        let path = root.join(name);
        if !path.is_file() {
            missing.push(name.to_string());
            continue;
        }
        match crate::document_engine_cache::file_hash(&path) {
            Ok(actual) if actual.eq_ignore_ascii_case(expected) => {}
            Ok(_) | Err(_) => invalid.push(name.to_string()),
        }
    }
    json!({
        "id": entry["id"],
        "version": entry["version"],
        "ready": missing.is_empty() && invalid.is_empty(),
        "missing": missing,
        "invalid": invalid,
        "offline": entry["offline"].as_bool().unwrap_or(false),
        "default": entry["default"].as_bool().unwrap_or(false)
    })
}

fn embedded_bundle_available(db: &Db, plugin_id: &str, model_id: &str, entry: &Value) -> bool {
    let Some(plugin_root) = plugin_install_root(db, plugin_id) else {
        return false;
    };
    entry["artifacts"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|artifact| {
            let name = artifact["name"].as_str().unwrap_or_default();
            let expected = artifact["sha256"].as_str().unwrap_or_default();
            let path = embedded_model_path(&plugin_root, model_id, name);
            path.is_file()
                && crate::document_engine_cache::file_hash(&path)
                    .is_ok_and(|hash| hash.eq_ignore_ascii_case(expected))
        })
}

fn ensure_default_model(db: &Db, plugin_id: &str, root: &Path) -> Value {
    let entry = match model_catalog_entry(DEFAULT_MODEL_ID) {
        Ok(entry) => entry,
        Err(error) => return json!({ "ready": false, "error": error }),
    };
    let current = model_bundle_status(root, &entry);
    if current["ready"].as_bool() == Some(true) {
        return current;
    }
    if embedded_bundle_available(db, plugin_id, DEFAULT_MODEL_ID, &entry) {
        if let Err(error) = install_model_bundle(db, plugin_id, root, DEFAULT_MODEL_ID) {
            return json!({
                "id": DEFAULT_MODEL_ID,
                "ready": false,
                "offline": true,
                "error": error
            });
        }
        return model_bundle_status(root, &entry);
    }
    current
}

fn install_model_bundle(
    db: &Db,
    plugin_id: &str,
    root: &Path,
    model_id: &str,
) -> Result<Vec<PathBuf>, String> {
    let entry = model_catalog_entry(model_id)?;
    let artifacts = entry["artifacts"]
        .as_array()
        .ok_or_else(|| "模型目录条目缺少文件清单".to_string())?;
    std::fs::create_dir_all(root).map_err(|error| format!("创建模型目录失败: {error}"))?;
    let staging = root.join(format!(".{model_id}.bundle-{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|error| format!("清理模型临时目录失败: {error}"))?;
    }
    std::fs::create_dir_all(&staging).map_err(|error| format!("创建模型临时目录失败: {error}"))?;

    let result = (|| {
        for artifact in artifacts {
            let name = artifact["name"]
                .as_str()
                .ok_or_else(|| "模型文件名缺失".to_string())?;
            let sha256 = artifact["sha256"]
                .as_str()
                .ok_or_else(|| format!("模型 {name} SHA-256 缺失"))?;
            let embedded = plugin_install_root(db, plugin_id)
                .map(|root| embedded_model_path(&root, model_id, name));
            let embedded_ready = embedded
                .as_ref()
                .filter(|path| path.is_file())
                .and_then(|path| crate::document_engine_cache::file_hash(path).ok())
                .is_some_and(|hash| hash.eq_ignore_ascii_case(sha256));
            if embedded_ready {
                std::fs::copy(
                    embedded.as_ref().expect("embedded path exists"),
                    staging.join(name),
                )
                .map_err(|error| format!("复制内置模型 {name} 失败: {error}"))?;
                continue;
            }
            let sources = artifact_sources(artifact);
            if sources.is_empty() {
                return Err(format!("模型 {name} 没有可用的下载源"));
            }
            crate::document_engine_cache::install_remote_from_sources(
                &staging, &sources, name, sha256, false,
            )?;
        }

        let mut installed = Vec::new();
        for artifact in artifacts {
            let name = artifact["name"].as_str().unwrap_or_default();
            let sha256 = artifact["sha256"].as_str().unwrap_or_default();
            let source = staging.join(name);
            let target = root.join(name);
            if target.is_file()
                && crate::document_engine_cache::file_hash(&target)
                    .is_ok_and(|hash| hash.eq_ignore_ascii_case(sha256))
            {
                installed.push(target);
                continue;
            }
            if target.exists() {
                std::fs::remove_file(&target)
                    .map_err(|error| format!("替换模型文件失败: {error}"))?;
            }
            std::fs::rename(&source, &target)
                .map_err(|error| format!("提交模型文件失败: {error}"))?;
            installed.push(target);
        }
        Ok(installed)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

// ---------------------------------------------------------------------------
// 配置（从插件 config_data 现读；缺失回退默认值）
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct DocumentEngineConfig {
    pub model_directory: String,
    pub dictionary_path: String,
    pub cache_directory: String,
    pub output_directory: String,
    pub device: String,
}

fn load_config(db: &Db, plugin_id: &str) -> DocumentEngineConfig {
    let raw = db
        .conn()
        .lock()
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT config_data FROM plugins WHERE id = ?1",
                [plugin_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .unwrap_or_else(|| "{}".into());
    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    let app_data = db
        .conn()
        .lock()
        .ok()
        .and_then(|_| std::env::var("APPDATA").ok())
        .unwrap_or_else(|| "C:\\".into());
    let base = PathBuf::from(&app_data)
        .join("cruciblebox")
        .join("document-engine");
    let model_directory = parsed
        .get("modelDirectory")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| base.join("models").to_string_lossy().into_owned());
    DocumentEngineConfig {
        dictionary_path: parsed
            .get("dictionaryPath")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                PathBuf::from(&model_directory)
                    .join("ppocr_keys_v1.txt")
                    .to_string_lossy()
                    .into_owned()
            }),
        model_directory,
        cache_directory: parsed
            .get("cacheDirectory")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| base.join("cache").to_string_lossy().into_owned()),
        output_directory: parsed
            .get("outputDirectory")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| base.join("output").to_string_lossy().into_owned()),
        device: parsed
            .get("device")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .to_string(),
    }
}

// ---------------------------------------------------------------------------
// 请求校验与分发
// ---------------------------------------------------------------------------

fn str_field<'a>(request: &'a Value, key: &str, max_len: usize) -> Result<Option<&'a str>, Value> {
    match request.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            if s.is_empty() || s.len() > max_len {
                Err(err(
                    "string-limit",
                    format!("{key} length must be 1..={max_len}"),
                ))
            } else {
                Ok(Some(s.as_str()))
            }
        }
        Some(_) => Err(err("invalid-value", format!("{key} must be a string"))),
    }
}

fn emit_progress(plugin_id: &str, task_id: &str, progress: &Value) {
    if let Some(emitter) = emitter() {
        emitter(
            "plugin:message",
            json!({
                "pluginId": plugin_id,
                "message": {
                    "type": "document.progress",
                    "taskId": task_id,
                    "progress": progress,
                },
            }),
        );
    }
}

fn ocr_model_version(cfg: &DocumentEngineConfig) -> String {
    let mut parts = Vec::new();
    for name in ["ch_PP-OCRv4_det.onnx", "ch_PP-OCRv4_rec.onnx"] {
        let path = PathBuf::from(&cfg.model_directory).join(name);
        let hash = crate::document_engine_cache::file_hash(&path).unwrap_or_default();
        parts.push(format!("{name}:{hash}"));
    }
    let dictionary =
        crate::document_engine_cache::file_hash(PathBuf::from(&cfg.dictionary_path).as_path())
            .unwrap_or_default();
    parts.push(format!("dictionary:{dictionary}"));
    format!("ocr-worker-v1:{}", parts.join("|"))
}

fn gpu_status() -> Value {
    #[cfg(windows)]
    {
        match std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=name,memory.total", "--format=csv,noheader"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let line = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                json!({
                    "available": !line.is_empty(),
                    "provider": "directml",
                    "device": line,
                })
            }
            _ => json!({ "available": false, "provider": "directml" }),
        }
    }
    #[cfg(not(windows))]
    {
        json!({ "available": false, "provider": "directml" })
    }
}

fn report_ocr_progress(
    ctx: &TaskContext,
    plugin_id: &str,
    mut progress: Value,
    scope: Option<(usize, usize)>,
) -> Result<(), String> {
    ctx.wait_if_paused()?;
    let local = progress
        .get("percent")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(100) as usize;
    if let Some((index, total)) = scope {
        let total = total.max(1);
        progress["percent"] = json!(((index * 100 + local) / total).min(100));
        progress["itemIndex"] = json!(index);
        progress["itemTotal"] = json!(total);
    }
    ctx.update_progress(
        progress
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or("ocr"),
        progress.get("percent").and_then(Value::as_u64).unwrap_or(0) as u32,
        progress
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("OCR 处理中"),
        Some(progress.clone()),
    );
    emit_progress(plugin_id, &ctx.task_id(), &progress);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_ocr_input(
    manager: &OcrWorkerManager,
    ctx: &TaskContext,
    plugin_id: &str,
    cfg: &DocumentEngineConfig,
    input: &str,
    language: Option<String>,
    device: Option<String>,
    scope: Option<(usize, usize)>,
) -> Result<Value, String> {
    let source_hash = crate::document_engine_cache::file_hash(PathBuf::from(input).as_path())?;
    let options = json!({
        "language": language,
        "device": device.clone().unwrap_or_else(|| cfg.device.clone()),
        "modelDirectory": cfg.model_directory,
        "dictionaryPath": cfg.dictionary_path,
    });
    let key = crate::document_engine_cache::cache_key(
        &source_hash,
        "paddleocr-onnx",
        &ocr_model_version(cfg),
        &options,
    );
    if let Ok(Some(cached)) = crate::document_engine_cache::read_result(
        PathBuf::from(&cfg.cache_directory).as_path(),
        &key,
    ) {
        let mut progress = json!({
            "stage": "cache",
            "percent": 100,
            "message": "命中 OCR 缓存",
            "cacheHit": true,
            "cacheKey": key,
        });
        if let Some((index, total)) = scope {
            progress["percent"] = json!(((index + 1) * 100 / total.max(1)).min(100));
            progress["itemIndex"] = json!(index);
            progress["itemTotal"] = json!(total);
        }
        ctx.update_progress(
            "cache",
            progress["percent"].as_u64().unwrap_or(100) as u32,
            "命中 OCR 缓存",
            Some(progress.clone()),
        );
        emit_progress(plugin_id, &ctx.task_id(), &progress);
        return Ok(cached);
    }

    let request = OcrWorkerRequest::new(
        format!(
            "{}-{}",
            ctx.task_id(),
            source_hash.get(..12).unwrap_or("input")
        ),
        input.to_string(),
        language,
        device.or_else(|| Some(cfg.device.clone())),
        Some(cfg.model_directory.clone()),
        Some(cfg.dictionary_path.clone()),
    );
    let result = manager.run(&request, ctx.cancel_flag(), &|progress| {
        // A paused task intentionally blocks the worker frame loop here. This
        // is the checkpoint boundary between OCR pages/frames and avoids
        // reporting "paused" while the native worker is still consuming CPU.
        if report_ocr_progress(ctx, plugin_id, progress, scope).is_err() {
            ctx.cancel_flag()
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    })?;
    ctx.check_cancelled()?;
    let _ = crate::document_engine_cache::write_result(
        PathBuf::from(&cfg.cache_directory).as_path(),
        &key,
        &result,
    );
    Ok(result)
}

fn is_image_path(path: &str) -> bool {
    matches!(
        PathBuf::from(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff"
    )
}

fn is_supported_document_path(path: &std::path::Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "bmp"
            | "tif"
            | "tiff"
            | "pdf"
            | "txt"
            | "text"
            | "md"
            | "markdown"
            | "html"
            | "htm"
            | "docx"
            | "pptx"
            | "xlsx"
    )
}

fn enumerate_document_paths(path: &str) -> Result<Vec<String>, String> {
    let root = PathBuf::from(path);
    let metadata =
        std::fs::symlink_metadata(&root).map_err(|error| format!("无法访问导入路径: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("导入路径不能是符号链接".into());
    }
    if metadata.is_file() {
        return if is_supported_document_path(&root) {
            Ok(vec![root.to_string_lossy().into_owned()])
        } else {
            Err("不支持的文档格式".into())
        };
    }
    if !metadata.is_dir() {
        return Err("导入路径必须是文件或文件夹".into());
    }
    let mut pending = vec![root];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .map_err(|error| format!("读取文件夹失败: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取文件夹失败: {error}"))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries.into_iter().rev() {
            let child = entry.path();
            let child_metadata = std::fs::symlink_metadata(&child)
                .map_err(|error| format!("读取导入项失败: {error}"))?;
            if child_metadata.file_type().is_symlink() {
                continue;
            }
            if child_metadata.is_dir() {
                pending.push(child);
            } else if child_metadata.is_file() && is_supported_document_path(&child) {
                files.push(child.to_string_lossy().into_owned());
                if files.len() > 1000 {
                    return Err("文件夹中的支持文件超过 1000 个上限".into());
                }
            }
        }
    }
    files.sort_by(|left, right| {
        let left_path = std::path::Path::new(left);
        let right_path = std::path::Path::new(right);
        left_path
            .file_name()
            .cmp(&right_path.file_name())
            .then_with(|| left.cmp(right))
    });
    if files.is_empty() {
        return Err("文件夹中没有支持的文档".into());
    }
    Ok(files)
}

fn merge_ocr_pages(
    mut parsed: Value,
    path: &str,
    manager: &OcrWorkerManager,
    ctx: &TaskContext,
    plugin_id: &str,
    cfg: &DocumentEngineConfig,
    _scope: Option<(usize, usize)>,
) -> Result<Value, String> {
    let page_numbers = parsed["ocrPageNumbers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_u64().map(|number| number as u32))
        .collect::<Vec<_>>();
    if page_numbers.is_empty() {
        return Ok(parsed);
    }
    let temp_dir =
        std::env::temp_dir().join(format!("cruciblebox-document-engine-{}", ctx.task_id()));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|error| format!("创建 PDF OCR 临时目录失败: {error}"))?;
    let result = (|| {
        for page_number in &page_numbers {
            ctx.wait_if_paused()?;
            let rendered = temp_dir.join(format!("page-{page_number}.png"));
            let dimensions = crate::pdf_parser::render_page_to_png(path, *page_number, &rendered)?;
            let ocr = run_ocr_input(
                manager,
                ctx,
                plugin_id,
                cfg,
                &rendered.to_string_lossy(),
                None,
                None,
                None,
            )?;
            let ocr_blocks = ocr["blocks"].as_array().cloned().unwrap_or_default();
            let mut blocks = Vec::with_capacity(ocr_blocks.len());
            for (block_index, block) in ocr_blocks.iter().enumerate() {
                let content = block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if content.is_empty() {
                    continue;
                }
                blocks.push(json!({
                    "id": format!("p{page_number}-b{}", block_index + 1),
                    "type": "text",
                    "content": content,
                    "bbox": block.get("bbox").cloned().unwrap_or(Value::Null),
                    "polygon": block.get("polygon").cloned().unwrap_or(Value::Null),
                    "confidence": block.get("confidence").cloned().unwrap_or(Value::Null),
                    "language": if content.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) { "zh" } else { "en" },
                }));
            }
            if let Some(pages) = parsed["document"]["pages"].as_array_mut() {
                if let Some(page) = pages
                    .iter_mut()
                    .find(|page| page["number"].as_u64() == Some(u64::from(*page_number)))
                {
                    page["width"] = json!(dimensions.0);
                    page["height"] = json!(dimensions.1);
                    page["blocks"] = Value::Array(blocks);
                }
            }
        }
        parsed["requiresOcr"] = json!(false);
        parsed["ocrPageNumbers"] = json!([]);
        parsed["ocrCompleted"] = json!(true);
        parsed["route"] = if parsed["route"] == "mixed" {
            json!("mixed")
        } else {
            json!("ocr")
        };
        if let Some(warnings) = parsed["warnings"].as_array_mut() {
            warnings.retain(|warning| warning["code"] != "pdf-render-unavailable");
        }
        parsed["document"]["metadata"]["hasOcrText"] = json!(true);
        Ok(parsed)
    })();
    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

fn handle_message(db: &Db, plugin_id: &str, payload: &Value) -> Value {
    let request = match payload {
        Value::Object(_) => payload,
        _ => return err("invalid-value", "message payload must be an object".into()),
    };
    let msg_type = request
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    match msg_type.as_str() {
        // ---- Phase 3 实现 ----
        "document.analyze" => {
            let path = match str_field(request, "path", 4096) {
                Ok(Some(p)) => p.to_string(),
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(e) => return e,
            };
            crate::document_analyzer::analyze_file(db, plugin_id, &path)
        }
        "document.files.enumerate" => {
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(Some(path)) => path,
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            match enumerate_document_paths(path) {
                Ok(paths) => json!({ "paths": paths, "count": paths.len() }),
                Err(message) => err("file-enumeration-failed", message),
            }
        }
        // ---- Phase 3 实现 ----
        "getStatus" => {
            let cfg = load_config(db, plugin_id);
            let default_model =
                ensure_default_model(db, plugin_id, Path::new(&cfg.model_directory));
            let cache_entries = crate::document_engine_cache::list_files(std::path::Path::new(
                &cfg.cache_directory,
            ))
            .map(|entries| entries.len())
            .unwrap_or(0);
            let worker = worker_manager()
                .map(|manager| manager.status())
                .unwrap_or_else(|| {
                    json!({
                        "available": false,
                        "running": false,
                        "reason": "OCR Worker manager is not configured"
                    })
                });
            json!({
                "status": {
                    "ocrWorker": worker,
                    "pdfium": crate::pdf_parser::renderer_status(),
                    "models": {
                        "default": default_model,
                        "directory": cfg.model_directory,
                    },
                    "gpu": gpu_status(),
                    "workers": {
                        "ocr": worker.get("running").and_then(Value::as_bool).unwrap_or(false) as u8,
                        "parser": 0,
                        "converter": 0
                    },
                    "config": {
                        "device": cfg.device,
                        "modelDirectory": cfg.model_directory,
                        "dictionaryPath": cfg.dictionary_path,
                        "cacheDirectory": cfg.cache_directory.clone(),
                        "outputDirectory": cfg.output_directory,
                    },
                    "cache": {
                        "entries": cache_entries,
                        "directory": cfg.cache_directory
                    },
                    "capabilities": {
                        "parse": ["pdf", "txt", "markdown", "html", "docx", "pptx", "xlsx"],
                        "convert": ["txt", "markdown", "html", "json", "docx", "pdf"],
                        "chunk": true,
                        "batch": ["ocr", "parse", "convert"]
                    }
                }
            })
        }
        "document.jobs.list" => {
            let list = tasks().list();
            json!({ "tasks": list })
        }
        "document.jobs.get" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id.to_string(),
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(e) => return e,
            };
            match tasks().get(&task_id) {
                Some(snapshot) => snapshot,
                None => err("task-not-found", "未找到指定任务".into()),
            }
        }
        "document.jobs.cancel" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id.to_string(),
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(e) => return e,
            };
            let is_ocr_task =
                tasks().active_task(RESOURCE_OCR).as_deref() == Some(task_id.as_str());
            if is_ocr_task {
                // Stop the process before releasing the resource slot.  This
                // prevents a new OCR task from starting and then being killed
                // by the cancellation of its predecessor.
                if let Some(manager) = worker_manager() {
                    manager.cancel_current();
                }
            }
            if tasks().cancel(&task_id) {
                json!({ "success": true, "taskId": task_id })
            } else {
                err("task-not-cancellable", "任务不存在或已结束".into())
            }
        }
        // ---- Phase 3：TaskManager → 常驻 OCR Worker ----
        "document.ocr" => {
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(Some(path)) => path.to_string(),
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            let options = request.get("options").unwrap_or(&Value::Null);
            if !options.is_null() && !options.is_object() {
                return err("invalid-value", "options must be an object".into());
            }
            let language = match str_field(options, "language", 16) {
                Ok(value) => value.map(ToOwned::to_owned),
                Err(error) => return error,
            };
            let requested_device = match str_field(options, "device", 16) {
                Ok(value) => value.map(ToOwned::to_owned),
                Err(error) => return error,
            };
            let cfg = load_config(db, plugin_id);
            let manager = match worker_manager() {
                Some(manager) => Arc::clone(manager),
                None => {
                    return err(
                        "worker-unavailable",
                        "OCR Worker manager is not configured".into(),
                    )
                }
            };
            let plugin_id = plugin_id.to_string();
            let device = requested_device;
            let manager = Arc::clone(&manager);
            let task_id = match tasks().start(
                RESOURCE_OCR,
                Box::new(move |ctx| {
                    ctx.update_progress("queued", 0, "等待 OCR Worker", None);
                    run_ocr_input(
                        &manager, ctx, &plugin_id, &cfg, &path, language, device, None,
                    )
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(&task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 4/6：统一文档解析任务 ----
        "document.parse" => {
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(Some(path)) => path.to_string(),
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            let extension_supported = PathBuf::from(&path)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "pdf"
                            | "txt"
                            | "text"
                            | "md"
                            | "markdown"
                            | "html"
                            | "htm"
                            | "docx"
                            | "pptx"
                            | "xlsx"
                    )
                });
            if !extension_supported {
                return err(
                    "unsupported-format",
                    "解析器支持 PDF/TXT/Markdown/HTML/DOCX/PPTX/XLSX".into(),
                );
            }
            let parse_cfg = load_config(db, plugin_id);
            let parse_manager = worker_manager().cloned();
            let parse_plugin_id = plugin_id.to_string();
            let task_id = match tasks().start(
                RESOURCE_PARSE,
                Box::new(move |ctx| {
                    ctx.update_progress("parse", 5, "读取文档", None);
                    let source_hash =
                        crate::document_engine_cache::file_hash(PathBuf::from(&path).as_path())?;
                    let cache_key = crate::document_engine_cache::cache_key(
                        &source_hash,
                        "document-parser",
                        "native-parser-v2",
                        &json!({ "ocrModel": ocr_model_version(&parse_cfg) }),
                    );
                    if let Ok(Some(cached)) = crate::document_engine_cache::read_result(
                        PathBuf::from(&parse_cfg.cache_directory).as_path(),
                        &cache_key,
                    ) {
                        ctx.update_progress(
                            "cache",
                            100,
                            "命中文档解析缓存",
                            Some(json!({ "cacheHit": true, "cacheKey": cache_key })),
                        );
                        return Ok(cached);
                    }
                    let mut result = crate::document_parser::parse_file(&path)?;
                    if result["requiresOcr"].as_bool().unwrap_or(false) {
                        let manager = parse_manager
                            .as_ref()
                            .ok_or_else(|| "扫描 PDF 需要已配置 OCR Worker".to_string())?;
                        result = merge_ocr_pages(
                            result,
                            &path,
                            manager,
                            ctx,
                            &parse_plugin_id,
                            &parse_cfg,
                            None,
                        )?;
                    }
                    ctx.check_cancelled()?;
                    let page_count = result["document"]["metadata"]["pageCount"]
                        .as_u64()
                        .unwrap_or(0);
                    let route = result["route"].as_str().unwrap_or("native");
                    ctx.update_progress(
                        "parse",
                        100,
                        if route == "native" && !result["ocrCompleted"].as_bool().unwrap_or(false) {
                            "文档解析完成"
                        } else {
                            "文档解析完成（已完成 OCR）"
                        },
                        Some(json!({ "page": page_count, "route": route })),
                    );
                    let _ = crate::document_engine_cache::write_result(
                        PathBuf::from(&parse_cfg.cache_directory).as_path(),
                        &cache_key,
                        &result,
                    );
                    Ok(result)
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(&task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 7：统一模型切分 ----
        "document.chunk" => {
            let options = request.get("options").cloned();
            let document = request.get("document").cloned();
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(value) => value.map(ToOwned::to_owned),
                Err(error) => return error,
            };
            if document.is_none() && path.is_none() {
                return err(
                    "invalid-value",
                    "document.chunk 需要 path 或 document".into(),
                );
            }
            let task_id = match tasks().start(
                RESOURCE_CHUNK,
                Box::new(move |ctx| {
                    ctx.update_progress("chunk", 5, "准备文档切分", None);
                    let parsed = if let Some(document) = document {
                        document
                    } else {
                        let path = path.ok_or_else(|| "缺少文档路径".to_string())?;
                        crate::document_parser::parse_file(&path)?["document"].clone()
                    };
                    let result =
                        crate::document_chunker::chunk_document(&parsed, options.as_ref())?;
                    ctx.check_cancelled()?;
                    ctx.update_progress(
                        "chunk",
                        100,
                        "文档切分完成",
                        Some(json!({ "count": result["count"] })),
                    );
                    Ok(result)
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(&task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 8：统一模型转换/导出 ----
        "document.convert" | "document.export" => {
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(Some(path)) => path.to_string(),
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            let target = match str_field(request, "target", 16) {
                Ok(Some(target)) => target.to_string(),
                Ok(None) => return err("invalid-value", "missing field: target".into()),
                Err(error) => return error,
            };
            let output_path = match str_field(request, "outputPath", 32 * 1024) {
                Ok(value) => value.map(ToOwned::to_owned),
                Err(error) => return error,
            };
            let convert_cfg = load_config(db, plugin_id);
            let convert_manager = worker_manager().cloned();
            let convert_plugin_id = plugin_id.to_string();
            let task_id = match tasks().start(
                RESOURCE_CONVERT,
                Box::new(move |ctx| {
                    ctx.update_progress("convert", 5, "解析源文档", None);
                    let mut parsed = crate::document_parser::parse_file(&path)?;
                    if parsed["requiresOcr"].as_bool().unwrap_or(false) {
                        let manager = convert_manager
                            .as_ref()
                            .ok_or_else(|| "扫描 PDF 转换需要已配置 OCR Worker".to_string())?;
                        parsed = merge_ocr_pages(
                            parsed,
                            &path,
                            manager,
                            ctx,
                            &convert_plugin_id,
                            &convert_cfg,
                            None,
                        )?;
                    }
                    let result = crate::document_converter::convert_document_with_cache(
                        &parsed["document"],
                        &target,
                        output_path.as_deref(),
                        Some(&convert_cfg.cache_directory),
                    )?;
                    ctx.check_cancelled()?;
                    ctx.update_progress("convert", 100, "转换完成", None);
                    Ok(result)
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(&task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 9：目录批处理（OCR / parse / convert） ----
        "document.batch" => {
            let paths = match request.get("paths").and_then(Value::as_array) {
                Some(paths) => paths,
                None => return err("invalid-value", "paths must be an array".into()),
            };
            if paths.is_empty() || paths.len() > 1000 {
                return err("invalid-value", "paths must contain 1..=1000 items".into());
            }
            let operation = match str_field(request, "operation", 16) {
                Ok(Some(operation)) => operation.to_string(),
                Ok(None) => "parse".to_string(),
                Err(error) => return error,
            };
            if !matches!(operation.as_str(), "ocr" | "parse" | "convert") {
                return err(
                    "unsupported-operation",
                    "batch 仅支持 ocr、parse 或 convert".into(),
                );
            }
            let paths = match paths
                .iter()
                .map(|path| path.as_str().map(ToOwned::to_owned))
                .collect::<Option<Vec<_>>>()
            {
                Some(paths) => paths,
                None => return err("invalid-value", "paths must contain strings".into()),
            };
            let target = request
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("txt")
                .to_string();
            let batch_cfg = load_config(db, plugin_id);
            let batch_manager = worker_manager().cloned();
            let batch_plugin_id = plugin_id.to_string();
            let task_id = match tasks().start(
                RESOURCE_BATCH,
                Box::new(move |ctx| {
                    let mut items = Vec::with_capacity(paths.len());
                    let mut succeeded = 0usize;
                    let mut failed = 0usize;
                    let mut failed_files = Vec::new();
                    for (index, path) in paths.iter().enumerate() {
                        ctx.wait_if_paused()?;
                        let percent = ((index * 100) / paths.len().max(1)) as u32;
                        ctx.update_progress(
                            "batch",
                            percent,
                            "处理批量文档",
                            Some(json!({ "index": index, "total": paths.len() })),
                        );
                        let result = match operation.as_str() {
                            "ocr" => {
                                let manager = batch_manager
                                    .as_ref()
                                    .ok_or_else(|| "批量 OCR 需要已配置 OCR Worker".to_string());
                                manager.and_then(|manager| {
                                    if is_image_path(path) {
                                        run_ocr_input(
                                            manager,
                                            ctx,
                                            &batch_plugin_id,
                                            &batch_cfg,
                                            path,
                                            None,
                                            None,
                                            None,
                                        )
                                    } else if PathBuf::from(path)
                                        .extension()
                                        .and_then(|value| value.to_str())
                                        .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
                                    {
                                        let parsed = crate::document_parser::parse_file(path)?;
                                        if parsed["requiresOcr"].as_bool().unwrap_or(false) {
                                            merge_ocr_pages(
                                                parsed,
                                                path,
                                                manager,
                                                ctx,
                                                &batch_plugin_id,
                                                &batch_cfg,
                                                None,
                                            )
                                        } else {
                                            Ok(parsed)
                                        }
                                    } else {
                                        Err("批量 OCR 仅支持图片和 PDF".into())
                                    }
                                })
                            }
                            "parse" => {
                                let mut parsed = crate::document_parser::parse_file(path)?;
                                if parsed["requiresOcr"].as_bool().unwrap_or(false) {
                                    let manager = batch_manager
                                        .as_ref()
                                        .ok_or_else(|| "扫描 PDF 需要已配置 OCR Worker".to_string())?;
                                    parsed = merge_ocr_pages(
                                        parsed,
                                        path,
                                        manager,
                                        ctx,
                                        &batch_plugin_id,
                                        &batch_cfg,
                                        None,
                                    )?;
                                }
                                Ok(parsed)
                            }
                            _ => {
                                let mut parsed = crate::document_parser::parse_file(path)?;
                                if parsed["requiresOcr"].as_bool().unwrap_or(false) {
                                    let manager = batch_manager
                                        .as_ref()
                                        .ok_or_else(|| "扫描 PDF 转换需要已配置 OCR Worker".to_string())?;
                                    parsed = merge_ocr_pages(
                                        parsed,
                                        path,
                                        manager,
                                        ctx,
                                        &batch_plugin_id,
                                        &batch_cfg,
                                        None,
                                    )?;
                                }
                                crate::document_converter::convert_document_with_cache(
                                    &parsed["document"],
                                    &target,
                                    None,
                                    Some(&batch_cfg.cache_directory),
                                )
                            }
                        };
                        match result {
                            Ok(result) => {
                                succeeded += 1;
                                items.push(json!({ "path": path, "result": result }));
                            }
                            Err(error) => {
                                failed += 1;
                                failed_files.push(json!({ "path": path, "error": error }));
                                items.push(json!({ "path": path, "error": error }));
                            }
                        }
                        ctx.update_progress(
                            "batch",
                            (((index + 1) * 100) / paths.len().max(1)) as u32,
                            "批量处理文档",
                            Some(json!({ "index": index + 1, "total": paths.len(), "succeeded": succeeded, "failed": failed })),
                        );
                    }
                    ctx.update_progress(
                        "batch",
                        100,
                        "批量处理完成",
                        Some(json!({ "total": items.len() })),
                    );
                    Ok(json!({
                        "operation": operation,
                        "items": items,
                        "count": paths.len(),
                        "succeeded": succeeded,
                        "failed": failed,
                        "failedFiles": failed_files
                    }))
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(&task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 9：任务暂停/恢复 ----
        "document.jobs.pause" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id,
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(error) => return error,
            };
            if tasks().pause(task_id) {
                json!({ "success": true, "taskId": task_id, "status": "paused" })
            } else {
                err("task-not-pausable", "任务不存在或当前状态不可暂停".into())
            }
        }
        "document.jobs.resume" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id,
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(error) => return error,
            };
            if tasks().resume(task_id) {
                json!({ "success": true, "taskId": task_id, "status": "running" })
            } else {
                err("task-not-resumable", "任务不存在或当前状态不可恢复".into())
            }
        }
        "document.jobs.retry" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id.to_string(),
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(error) => return error,
            };
            let Some(snapshot) = tasks().get(&task_id) else {
                return err("task-not-found", "未找到指定任务".into());
            };
            if !matches!(
                snapshot["status"].as_str(),
                Some("failed") | Some("cancelled")
            ) {
                return err("task-not-retryable", "只有失败或已取消任务可以重试".into());
            }
            let original = retry_requests()
                .lock()
                .ok()
                .and_then(|requests| requests.get(&task_id).cloned());
            match original {
                Some(original) => handle_message(db, plugin_id, &original),
                None => err("retry-unavailable", "任务请求已过期，无法重试".into()),
            }
        }
        "document.models.catalog" => {
            json!({ "catalog": model_catalog() })
        }
        "document.models.installBundle" => {
            let model_id = match str_field(request, "modelId", 128) {
                Ok(Some(id)) => id,
                Ok(None) => return err("invalid-value", "missing field: modelId".into()),
                Err(error) => return error,
            };
            let cfg = load_config(db, plugin_id);
            match install_model_bundle(
                db,
                plugin_id,
                PathBuf::from(&cfg.model_directory).as_path(),
                model_id,
            ) {
                Ok(installed) => json!({
                    "success": true,
                    "modelId": model_id,
                    "files": installed.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>()
                }),
                Err(message) => err("model-bundle-install-failed", message),
            }
        }
        "document.models.list" => {
            let cfg = load_config(db, plugin_id);
            match crate::document_engine_cache::list_files(std::path::Path::new(
                &cfg.model_directory,
            )) {
                Ok(models) => {
                    let count = models.len();
                    let bundles = model_catalog()
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|entry| model_bundle_status(Path::new(&cfg.model_directory), entry))
                        .collect::<Vec<_>>();
                    json!({
                        "directory": cfg.model_directory,
                        "models": models,
                        "count": count,
                        "bundles": bundles
                    })
                }
                Err(message) => err("model-list-failed", message),
            }
        }
        "document.models.install" => {
            let cfg = load_config(db, plugin_id);
            if let Some(url) = request.get("url").and_then(Value::as_str) {
                let name = request
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| url.rsplit('/').next())
                    .unwrap_or("model");
                let expected = match request.get("sha256").and_then(Value::as_str) {
                    Some(value) => value,
                    None => return err("invalid-value", "远程模型必须提供 sha256".into()),
                };
                return match crate::document_engine_cache::install_remote(
                    PathBuf::from(&cfg.model_directory).as_path(),
                    url,
                    name,
                    expected,
                    false,
                ) {
                    Ok(target) => {
                        json!({ "success": true, "path": target.to_string_lossy(), "name": name, "source": "remote" })
                    }
                    Err(message) => err("model-install-failed", message),
                };
            }
            let source = match str_field(request, "sourcePath", 32 * 1024) {
                Ok(Some(path)) => PathBuf::from(path),
                Ok(None) => return err("invalid-value", "missing field: sourcePath".into()),
                Err(error) => return error,
            };
            let name = match str_field(request, "name", 256) {
                Ok(Some(name)) => name.to_string(),
                Ok(None) => source
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("model")
                    .to_string(),
                Err(error) => return error,
            };
            match crate::document_engine_cache::install_local(
                std::path::Path::new(&cfg.model_directory),
                &source,
                &name,
            ) {
                Ok(target) => {
                    json!({ "success": true, "path": target.to_string_lossy(), "name": name })
                }
                Err(message) => err("model-install-failed", message),
            }
        }
        "document.models.update" => {
            let url = match str_field(request, "url", 32 * 1024) {
                Ok(Some(url)) => url,
                Ok(None) => return err("invalid-value", "missing field: url".into()),
                Err(error) => return error,
            };
            let expected = match str_field(request, "sha256", 128) {
                Ok(Some(value)) => value,
                Ok(None) => return err("invalid-value", "missing field: sha256".into()),
                Err(error) => return error,
            };
            let name = match str_field(request, "name", 256) {
                Ok(Some(value)) => value.to_string(),
                Ok(None) => return err("invalid-value", "missing field: name".into()),
                Err(error) => return error,
            };
            let cfg = load_config(db, plugin_id);
            match crate::document_engine_cache::install_remote(
                PathBuf::from(&cfg.model_directory).as_path(),
                url,
                &name,
                expected,
                true,
            ) {
                Ok(target) => {
                    json!({ "success": true, "path": target.to_string_lossy(), "name": name, "source": "remote", "updated": true })
                }
                Err(message) => err("model-update-failed", message),
            }
        }
        "document.models.remove" => {
            let relative = match str_field(request, "path", 4096) {
                Ok(Some(path)) => path.to_string(),
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            let cfg = load_config(db, plugin_id);
            let target = match crate::document_engine_cache::resolve_child(
                std::path::Path::new(&cfg.model_directory),
                &relative,
            ) {
                Ok(target) => target,
                Err(message) => return err("unsafe-model-path", message),
            };
            match crate::document_engine_cache::remove_child(
                std::path::Path::new(&cfg.model_directory),
                &target,
            ) {
                Ok(()) => json!({ "success": true, "path": relative }),
                Err(message) => err("model-remove-failed", message),
            }
        }
        "document.cache.clear" => {
            let cfg = load_config(db, plugin_id);
            match crate::document_engine_cache::clear_directory(std::path::Path::new(
                &cfg.cache_directory,
            )) {
                Ok(removed) => {
                    json!({ "success": true, "removed": removed, "directory": cfg.cache_directory })
                }
                Err(message) => err("cache-clear-failed", message),
            }
        }
        _ => err("unknown-type", format!("unknown message type: {msg_type}")),
    }
}

/// 统一入口：envelope_host::host_dispatch 分发 service="document-engine" 时调用。
/// params: { operation, payload? }。
pub fn dispatch(
    db: &Db,
    plugin_id: &str,
    operation: &str,
    payload: Option<&Value>,
) -> Result<Value, String> {
    if operation == "activate" {
        // 初始化任务表；Worker 仍按需启动，避免仅激活插件就加载模型。
        let _ = tasks();
        // 内置模型只做本地校验和复制，不在激活阶段主动阻塞网络下载。
        // 没有内置资源时由模型页的 installBundle 触发镜像回退。
        let cfg = load_config(db, plugin_id);
        let _ = ensure_default_model(db, plugin_id, Path::new(&cfg.model_directory));
        return Ok(Value::Null);
    }
    if operation == "deactivate" {
        tasks().cancel_all_active();
        if let Ok(mut requests) = retry_requests().lock() {
            requests.clear();
        }
        if let Some(manager) = worker_manager() {
            manager.shutdown();
        }
        return Ok(Value::Null);
    }
    if operation != "message" {
        return Err(format!("unknown trusted operation: {operation}"));
    }
    let payload = payload.ok_or_else(|| "message operation requires payload".to_string())?;
    Ok(handle_message(db, plugin_id, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempDb {
        dir: PathBuf,
        db: Db,
    }

    impl TempDb {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "cruciblebox-doceng-svc-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let db = Db::open(&dir.join("test.db")).unwrap();
            TempDb { dir, db }
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn get_status_returns_object() {
        let t = TempDb::new("status");
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "getStatus" })),
        )
        .unwrap();
        assert!(out.get("status").is_some(), "status field expected");
        assert!(out["status"]["ocrWorker"].is_object());
    }

    #[test]
    fn folder_import_enumerates_supported_files_deterministically() {
        let t = TempDb::new("enumerate");
        let root = t.dir.join("documents");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("b.txt"), "b").unwrap();
        std::fs::write(root.join("nested").join("a.pdf"), b"%PDF-1.4").unwrap();
        std::fs::write(root.join("ignored.bin"), b"ignored").unwrap();
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({
                "type": "document.files.enumerate",
                "path": root.to_string_lossy()
            })),
        )
        .unwrap();
        assert_eq!(out["count"], 2);
        let paths = out["paths"].as_array().unwrap();
        assert!(paths[0].as_str().unwrap().ends_with("a.pdf"));
        assert!(paths[1].as_str().unwrap().ends_with("b.txt"));
    }

    #[test]
    fn unknown_type_errors() {
        let t = TempDb::new("unknowntype");
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "nope" })),
        )
        .unwrap();
        assert_eq!(out["code"], "unknown-type");
    }

    #[test]
    fn ocr_requires_configured_worker() {
        let t = TempDb::new("notimpl");
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "document.ocr", "path": "C:\\missing.png" })),
        )
        .unwrap();
        assert_eq!(out["code"], "worker-unavailable");
    }

    #[test]
    fn analyze_routes_pdf_with_text_to_native() {
        let t = TempDb::new("analyze");
        let dir = std::env::temp_dir().join(format!("cb-de-analyze-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = b"%PDF-1.4\n1 0 obj<</Type/Page>>endobj\n2 0 obj<</Font>>endobj\nBT /F1 12 Tf (Hi) Tj ET\n%%EOF";
        let path = dir.join("doc.pdf");
        std::fs::write(&path, pdf).unwrap();
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(
                &json!({ "type": "document.analyze", "path": path.to_string_lossy().into_owned() }),
            ),
        )
        .unwrap();
        assert_eq!(out["category"], "pdf");
        assert_eq!(out["detail"]["hasTextLayer"], true);
        assert_eq!(out["recommendedEngine"], "native");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_pdf_task_returns_unified_document() {
        let t = TempDb::new("parse");
        let dir = std::env::temp_dir().join(format!("cb-de-parse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = b"%PDF-1.4\n1 0 obj\n<</Type /Page /MediaBox [0 0 200 100] /Contents 2 0 R>>\nendobj\n2 0 obj\n<</Length 23>>\nstream\nBT (Hi) Tj ET\nendstream\nendobj\n%%EOF";
        let path = dir.join("doc.pdf");
        std::fs::write(&path, pdf).unwrap();
        let accepted = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({
                "type": "document.parse",
                "path": path.to_string_lossy().into_owned()
            })),
        )
        .unwrap();
        let task_id = accepted["taskId"].as_str().unwrap().to_string();
        let mut snapshot = Value::Null;
        for _ in 0..100 {
            snapshot = dispatch(
                &t.db,
                "document-engine",
                "message",
                Some(&json!({ "type": "document.jobs.get", "taskId": task_id })),
            )
            .unwrap();
            if snapshot["status"] == "succeeded" || snapshot["status"] == "failed" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(snapshot["status"], "succeeded");
        assert_eq!(snapshot["result"]["route"], "native");
        assert_eq!(
            snapshot["result"]["document"]["pages"][0]["blocks"][0]["content"],
            "Hi"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_rejects_non_pdf_extension() {
        let t = TempDb::new("parse-format");
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "document.parse", "path": "C:\\input.xyz" })),
        )
        .unwrap();
        assert_eq!(out["code"], "unsupported-format");
    }

    #[test]
    fn chunk_task_returns_chunks_for_document_path() {
        let t = TempDb::new("chunk");
        let dir = std::env::temp_dir().join(format!("cb-de-chunk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");
        std::fs::write(&path, b"first paragraph\n\nsecond paragraph").unwrap();
        let accepted = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "document.chunk", "path": path.to_string_lossy() })),
        )
        .unwrap();
        let task_id = accepted["taskId"].as_str().unwrap().to_string();
        let mut snapshot = Value::Null;
        for _ in 0..100 {
            snapshot = dispatch(
                &t.db,
                "document-engine",
                "message",
                Some(&json!({ "type": "document.jobs.get", "taskId": task_id })),
            )
            .unwrap();
            if snapshot["status"] == "succeeded" || snapshot["status"] == "failed" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(snapshot["status"], "succeeded");
        assert!(snapshot["result"]["count"].as_u64().unwrap() >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_task_writes_markdown_output() {
        let t = TempDb::new("convert");
        let dir = std::env::temp_dir().join(format!("cb-de-convert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");
        let output = dir.join("note.md");
        std::fs::write(&path, b"hello").unwrap();
        let accepted = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({
                "type": "document.convert",
                "path": path.to_string_lossy(),
                "target": "md",
                "outputPath": output.to_string_lossy()
            })),
        )
        .unwrap();
        let task_id = accepted["taskId"].as_str().unwrap().to_string();
        let mut snapshot = Value::Null;
        for _ in 0..100 {
            snapshot = dispatch(
                &t.db,
                "document-engine",
                "message",
                Some(&json!({ "type": "document.jobs.get", "taskId": task_id })),
            )
            .unwrap();
            if snapshot["status"] == "succeeded" || snapshot["status"] == "failed" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(snapshot["status"], "succeeded");
        assert!(std::fs::read_to_string(&output).unwrap().contains("hello"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jobs_get_unknown_returns_not_found() {
        let t = TempDb::new("jobsget");
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "document.jobs.get", "taskId": "deadbeef" })),
        )
        .unwrap();
        assert_eq!(out["code"], "task-not-found");
    }

    #[test]
    fn model_catalog_exposes_worker_compatible_bundle() {
        let catalog = model_catalog();
        let entry = &catalog[0];
        assert_eq!(entry["id"], "ppocrv4-mobile-zh-en");
        assert_eq!(entry["recommended"], true);
        assert_eq!(entry["default"], true);
        assert_eq!(entry["offline"], true);
        assert_eq!(entry["artifacts"].as_array().map(Vec::len), Some(3));
        assert_eq!(entry["totalBytes"].as_u64(), Some(15_568_058));
        for artifact in entry["artifacts"].as_array().unwrap() {
            assert_eq!(artifact["sha256"].as_str().map(str::len), Some(64));
            assert!(artifact["url"].as_str().unwrap().starts_with("https://"));
            assert!(artifact["sources"]
                .as_array()
                .is_some_and(|sources| sources.len() >= 2));
        }

        let t = TempDb::new("model-catalog");
        let response = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "document.models.catalog" })),
        )
        .unwrap();
        assert_eq!(response["catalog"][0]["id"], "ppocrv4-mobile-zh-en");
    }

    #[test]
    fn model_bundle_status_reports_missing_files_without_network() {
        let t = TempDb::new("model-status");
        let root = t.dir.join("models");
        let entry = model_catalog_entry(DEFAULT_MODEL_ID).unwrap();
        let status = model_bundle_status(&root, &entry);
        assert_eq!(status["ready"], false);
        assert_eq!(status["offline"], true);
        assert_eq!(status["missing"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn unknown_model_bundle_is_rejected_before_download() {
        let t = TempDb::new("model-bundle");
        let out = dispatch(
            &t.db,
            "document-engine",
            "message",
            Some(&json!({
                "type": "document.models.installBundle",
                "modelId": "missing-model"
            })),
        )
        .unwrap();
        assert_eq!(out["code"], "model-bundle-install-failed");
        assert_eq!(out["error"], "未找到可用的模型包");
    }

    #[test]
    fn unknown_operation_rejected() {
        let t = TempDb::new("op");
        assert!(dispatch(&t.db, "document-engine", "other", None).is_err());
    }
}

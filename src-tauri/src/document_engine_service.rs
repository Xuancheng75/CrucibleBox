// Document Engine trusted host service（beta7：文字来源与模型方案收敛）
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
    TaskContext, TaskManager, RESOURCE_BATCH, RESOURCE_CHUNK, RESOURCE_CONVERT, RESOURCE_MODELS,
    RESOURCE_OCR, RESOURCE_PARSE, RESOURCE_SPLIT,
};
use crate::ocr_worker::{OcrWorkerManager, OcrWorkerRequest};
use base64::Engine as _;
use image::codecs::jpeg::JpegEncoder;
use image::ImageReader;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct Service {
    tasks: Arc<TaskManager>,
    retry_requests: Mutex<HashMap<String, Value>>,
    worker: Option<Arc<OcrWorkerManager>>,
    resources: Option<PathBuf>,
    document_worker: Mutex<Arc<cruciblebox_document_worker::client::Client>>,
    document_runtime: Arc<cruciblebox_document_worker::runtime::Store>,
    gpu_status: std::sync::OnceLock<Value>,
}
impl Service {
    #[cfg(test)]
    pub fn new(runtime: Arc<crate::task_runtime::TaskRuntime>) -> Self {
        Self::with_worker(runtime, None, None, None)
    }
    pub fn with_worker(
        runtime: Arc<crate::task_runtime::TaskRuntime>,
        worker: Option<Arc<OcrWorkerManager>>,
        resources: Option<PathBuf>,
        runtime_root: Option<PathBuf>,
    ) -> Self {
        let root = runtime_root
            .unwrap_or_else(|| std::env::temp_dir().join("cruciblebox-document-runtime-tests"));
        let document_runtime = cruciblebox_document_worker::runtime::Store::new(root.clone());
        let catalog: cruciblebox_document_worker::runtime::Catalog =
            serde_json::from_str(include_str!("../../shared/document-runtime-catalog.json"))
                .expect("embedded document runtime catalog");
        let client = match document_runtime.lease(&catalog) {
            Ok(lease) => cruciblebox_document_worker::client::Client::leased(
                lease,
                root.join("jobs"),
                std::time::Duration::from_secs(15 * 60),
            ),
            Err(_) => cruciblebox_document_worker::client::Client::new(
                root.join("unavailable/document-worker.exe"),
                None,
                root.join("jobs"),
                std::time::Duration::from_secs(15 * 60),
            ),
        };
        #[cfg(test)]
        let client = if let Some(exe) = std::env::var_os("DOCUMENT_WORKER_ACCEPTANCE_EXE") {
            cruciblebox_document_worker::client::Client::new(
                exe.into(),
                std::env::var_os("DOCUMENT_WORKER_ACCEPTANCE_PDFIUM").map(PathBuf::from),
                root.join("jobs"),
                std::time::Duration::from_secs(15 * 60),
            )
        } else {
            client
        };
        let document_worker = Mutex::new(Arc::new(client));
        Self {
            document_worker,
            document_runtime,
            tasks: Arc::new(TaskManager::with_runtime(runtime)),
            retry_requests: Mutex::new(HashMap::new()),
            worker,
            resources,
            gpu_status: std::sync::OnceLock::new(),
        }
    }
    fn document_client(&self) -> Arc<cruciblebox_document_worker::client::Client> {
        self.document_worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn install_document_runtime(&self, source: &Path) -> Result<Value, String> {
        let catalog: cruciblebox_document_worker::runtime::Catalog =
            serde_json::from_str(include_str!("../../shared/document-runtime-catalog.json"))
                .map_err(|e| e.to_string())?;
        let lease = self.document_runtime.install(source, &catalog)?;
        let identity = lease.identity.clone();
        let jobs = std::env::temp_dir().join("cruciblebox-document-jobs");
        let client = cruciblebox_document_worker::client::Client::leased(
            lease,
            jobs,
            std::time::Duration::from_secs(15 * 60),
        );
        *self
            .document_worker
            .lock()
            .map_err(|_| "DOCUMENT_RUNTIME_STATE_UNAVAILABLE")? = Arc::new(client);
        Ok(json!({"installed":true,"version":catalog.version,"identity":identity}))
    }
    pub fn set_emitter(&self, emitter: Emitter) {
        self.tasks.set_progress_emitter(emitter);
    }
    pub fn set_observer(&self, observer: Arc<dyn Fn(Value) + Send + Sync>) {
        self.tasks.set_observer(observer);
    }
}

type Emitter = Arc<dyn Fn(&str, Value) + Send + Sync>;

const DEFAULT_MODEL_ID: &str = "ppocrv6-small-det-v5-mobile-rec";
const FAST_MODEL_PROFILE: &str = "onnx-text-fast";
const FORMULA_MODEL_PROFILE: &str = "onnx-doclayout-m-rapidlatex";
const FORMULANET_MODEL_PROFILE: &str = "onnx-doclayout-m-formulanet-plus-s";
const PIPELINE_VERSION: &str = "document-ir-v5-preserve-rapidocr-order";
const OCR_CONFIG_VERSION: &str = "ocr-config-v5-rapidocr-formula";
const LAYOUT_MODEL_VERSION: &str = "ppdoclayout-m-v1";
const FORMULA_DETECTION_VERSION: &str = "layout-formula-region-v2";

fn ocr_preview(path: &Path) -> Result<Value, String> {
    let metadata = std::fs::metadata(path).map_err(|error| format!("预览文件不可读取：{error}"))?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err("预览仅支持不超过 32MB 的图片文件".into());
    }
    let reader = ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|error| format!("图片格式不可读取：{error}"))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| format!("图片尺寸不可读取：{error}"))?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 40_000_000 {
        return Err("预览图片尺寸超出限制".into());
    }
    let image = ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|error| format!("图片格式不可读取：{error}"))?
        .decode()
        .map_err(|error| format!("图片解码失败：{error}"))?;
    for longest_edge in [720, 560, 420, 320] {
        let thumbnail = image.thumbnail(longest_edge, longest_edge).to_rgb8();
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, 68)
            .encode(
                thumbnail.as_raw(),
                thumbnail.width(),
                thumbnail.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|error| format!("图片预览编码失败：{error}"))?;
        if encoded.len() <= 160 * 1024 {
            return Ok(json!({
                "width": width,
                "height": height,
                "dataUrl": format!(
                    "data:image/jpeg;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(encoded)
                ),
            }));
        }
    }
    Err("图片预览仍超出通信预算".into())
}

fn formula_python(cfg: &DocumentEngineConfig) -> Option<PathBuf> {
    std::env::var_os("CRUCIBLEBOX_FORMULA_ONNX_PYTHON")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            cfg.resources
                .as_ref()
                .map(|root| root.join("formula-ocr/python/python.exe"))
                .filter(|path| path.is_file())
        })
}

fn formula_model_directory(cfg: &DocumentEngineConfig) -> PathBuf {
    if let Some(path) = std::env::var_os("CRUCIBLEBOX_FORMULA_ONNX_MODELS")
        .map(PathBuf::from)
        .filter(|path| {
            path.join("formula-onnx/pp_formulanet_plus_s.onnx")
                .is_file()
        })
    {
        return path;
    }
    let configured = PathBuf::from(&cfg.model_directory);
    if configured
        .join("formula-onnx/pp_formulanet_plus_s.onnx")
        .is_file()
    {
        return configured;
    }
    cfg.resources
        .as_ref()
        .map(|root| root.join("formula-ocr/models"))
        .filter(|path| {
            path.join("formula-onnx/pp_formulanet_plus_s.onnx")
                .is_file()
        })
        .unwrap_or(configured)
}

fn paddle_formula_directory(cfg: &DocumentEngineConfig) -> PathBuf {
    if let Some(path) = std::env::var_os("CRUCIBLEBOX_PADDLE_FORMULA_MODELS")
        .map(PathBuf::from)
        .filter(|path| path.join("paddle-formula/PP-DocLayout-M").is_dir())
    {
        return path;
    }
    let configured = PathBuf::from(&cfg.model_directory);
    let separate_addon = configured.join("high/models");
    if separate_addon
        .join("paddle-formula/PP-DocLayout-M")
        .is_dir()
    {
        return separate_addon;
    }
    if !cfg.formula_addon_directory.is_empty() {
        let addon = PathBuf::from(&cfg.formula_addon_directory);
        let models = addon.join("models");
        return if models.is_dir() { models } else { addon };
    }
    if configured.join("paddle-formula/PP-DocLayout-M").is_dir() {
        return configured;
    }
    cfg.resources
        .as_ref()
        .map(|root| root.join("formula-ocr/high/models"))
        .filter(|path| path.join("paddle-formula/PP-DocLayout-M").is_dir())
        .unwrap_or(configured)
}

fn high_precision_runtime_ready(cfg: &DocumentEngineConfig) -> bool {
    let models = paddle_formula_directory(cfg);
    if !models
        .join("paddle-formula/PP-DocLayout-M/inference.pdiparams")
        .is_file()
        || !models
            .join("paddle-formula/PP-FormulaNet_plus-L/inference.pdiparams")
            .is_file()
    {
        return false;
    }
    let addon = models.parent();
    let python = std::env::var_os("CRUCIBLEBOX_FORMULA_PYTHON")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| addon.map(|root| root.join("python/python.exe")))
        .filter(|path| path.is_file())
        .or_else(|| {
            cfg.resources
                .as_ref()
                .map(|root| root.join("formula-ocr/high/python/python.exe"))
                .filter(|path| path.is_file())
        });
    let script = std::env::var_os("CRUCIBLEBOX_FORMULA_WORKER_SCRIPT")
        .map(PathBuf::from)
        .or_else(|| {
            cfg.resources
                .as_ref()
                .map(|root| root.join("formula-ocr/high/formula-ocr-worker.py"))
                .filter(|path| path.is_file())
        })
        .or_else(|| addon.map(|root| root.join("formula-ocr-worker.py")));
    python.is_some() && script.is_some_and(|path| path.is_file())
}

fn unimernet_runtime_ready(cfg: &DocumentEngineConfig) -> bool {
    let models = paddle_formula_directory(cfg);
    if !models
        .join("paddle-formula/PP-DocLayout-M/inference.pdiparams")
        .is_file()
        || !models
            .join("paddle-formula/UniMERNet/inference.pdiparams")
            .is_file()
    {
        return false;
    }
    let addon = models.parent();
    let python = std::env::var_os("CRUCIBLEBOX_FORMULA_PYTHON")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| addon.map(|root| root.join("python/python.exe")))
        .filter(|path| path.is_file())
        .or_else(|| {
            cfg.resources
                .as_ref()
                .map(|root| root.join("formula-ocr/high/python/python.exe"))
                .filter(|path| path.is_file())
        });
    let script = std::env::var_os("CRUCIBLEBOX_FORMULA_WORKER_SCRIPT")
        .map(PathBuf::from)
        .or_else(|| addon.map(|root| root.join("formula-ocr-worker.py")));
    python.is_some() && script.is_some_and(|path| path.is_file())
}

const HIGH_FORMULA_ADDON_SHA256: &str =
    "16dd075ba1852452c5a1c08cd29835d09d08effcf9410ef68d1b6a85ce3dff14";
const HIGH_FORMULA_ADDON_BYTES: u64 = 972_509_504;

fn install_high_formula_addon(
    cfg: &DocumentEngineConfig,
    archive: &Path,
    ctx: &TaskContext,
) -> Result<Value, String> {
    ctx.update_progress("formula-addon", 5, "验证附加包", None);
    if !archive.is_file() || archive.extension().and_then(|v| v.to_str()) != Some("7z") {
        return Err("请选择 CrucibleBox 高精度公式 .7z 附加包".into());
    }
    if std::fs::metadata(archive)
        .map_err(|error| format!("读取附加包大小失败：{error}"))?
        .len()
        != HIGH_FORMULA_ADDON_BYTES
    {
        return Err("附加包大小或 SHA-256 不匹配，拒绝安装".into());
    }
    let hash = crate::document_engine_cache::file_hash(archive)?;
    if !hash.eq_ignore_ascii_case(HIGH_FORMULA_ADDON_SHA256) {
        return Err("附加包 SHA-256 不匹配，拒绝安装".into());
    }
    ctx.check_cancelled()?;
    let seven_zip = cfg
        .resources
        .as_ref()
        .map(|root| root.join("7zip/7za.exe"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/7zip/7za.exe")
        });
    if !seven_zip.is_file() {
        return Err("安装包缺少 7-Zip 解压器".into());
    }
    let model_root = PathBuf::from(&cfg.model_directory);
    std::fs::create_dir_all(&model_root).map_err(|error| format!("模型目录不可写：{error}"))?;
    let target = model_root.join("high");
    if target.exists() {
        return Err("高精度附加包目录已存在；请先检查现有目录，不自动覆盖".into());
    }
    let staging = model_root.join(format!(".high-import-{}", ctx.task_id()));
    std::fs::create_dir(&staging).map_err(|error| format!("无法建立附加包暂存目录：{error}"))?;
    let install = (|| -> Result<Value, String> {
        ctx.update_progress("formula-addon", 20, "解压高精度附加包", None);
        let mut child = std::process::Command::new(&seven_zip)
            .arg("x")
            .arg("-y")
            .arg("-bd")
            .arg("-bso0")
            .arg(format!("-o{}", staging.display()))
            .arg(archive)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("无法启动附加包解压器：{error}"))?;
        loop {
            if ctx.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err("附加包导入已取消".into());
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(status)) => return Err(format!("附加包解压失败：{status}")),
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(error) => return Err(format!("等待解压器失败：{error}")),
            }
        }
        ctx.check_cancelled()?;
        let extracted = staging.join("formula-high-addon");
        for relative in [
            "python/python.exe",
            "formula-ocr-worker.py",
            "models/paddle-formula/PP-DocLayout-M/inference.pdiparams",
            "models/paddle-formula/PP-FormulaNet_plus-L/inference.pdiparams",
        ] {
            if !extracted.join(relative).is_file() {
                return Err(format!("附加包缺少 {relative}"));
            }
        }
        ctx.update_progress("formula-addon", 90, "提交高精度附加包", None);
        std::fs::rename(&extracted, &target).map_err(|error| format!("附加包提交失败：{error}"))?;
        ctx.update_progress("formula-addon", 100, "高精度附加包已就绪", None);
        Ok(json!({
            "addonDirectory": target,
            "warning": "该模型单页实测超过 3.4GB，且公式仍需逐式校对；须主动选择 L 模式。"
        }))
    })();
    let _ = std::fs::remove_dir_all(&staging);
    install
}

fn remember_retry(context: &Service, task_id: &str, request: &Value) {
    if let Ok(mut requests) = context.retry_requests.lock() {
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
/// Keep model choices static and hash-pinned.  The default bundle is the
/// lighter PP-OCRv6-small detector + PP-OCRv5 mobile recognizer profile;
/// the legacy v4 bundle remains available for existing installations.
fn model_catalog() -> Value {
    json!([
        {
            "id": "ppocrv6-small-det-v5-mobile-rec",
            "name": "PP-OCRv6 Small + PP-OCRv5 Mobile 轻量模型",
            "version": "6.0-det+5.0-rec",
            "description": "内置中英混排文字 OCR；公式区域由独立的版面检测与 Formula Recognizer 处理。",
            "recommended": true,
            "default": true,
            "offline": true,
            "license": "Apache-2.0",
            "totalBytes": 26634912u64,
            "artifacts": [
                {
                    "name": "ppocrv6_small_det.onnx",
                    "purpose": "文字区域检测",
                    "bytes": 9929594u64,
                    "sources": ["https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv6/det/PP-OCRv6_det_small.onnx"],
                    "url": "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv6/det/PP-OCRv6_det_small.onnx",
                    "sha256": "090f04abcd9d9a7498bc4ebf677e4cb9bdce1fe4197ddb7e529f1ef44e1ff94f"
                },
                {
                    "name": "PP-OCRv5_mobile_rec.onnx",
                    "purpose": "中文/英文/日文混排文字识别",
                    "bytes": 16631306u64,
                    "sources": ["https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx"],
                    "url": "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/onnx/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile.onnx",
                    "sha256": "5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5"
                },
                {
                    "name": "ppocrv5_dict.txt",
                    "purpose": "中英混排 CTC 字典",
                    "bytes": 74012u64,
                    "sources": ["https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/paddle/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile/ppocrv5_dict.txt"],
                    "url": "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.9.2/paddle/PP-OCRv5/rec/ch_PP-OCRv5_rec_mobile/ppocrv5_dict.txt",
                    "sha256": "d1979e9f794c464c0d2e0b70a7fe14dd978e9dc644c0e71f14158cdf8342af1b"
                }
            ]
        },
        {
            "id": "ppocrv4-mobile-zh-en",
            "name": "PP-OCRv4 中文/英文标准模型",
            "version": "4.0",
            "description": "适用于中文、英文及混合文档的本地 OCR；包含检测模型、识别模型和中文字符字典。",
            "recommended": false,
            "default": false,
            "offline": true,
            "license": "Apache-2.0",
            "totalBytes": 15568058u64,
            "artifacts": [
                {
                    "name": "ch_PP-OCRv4_det.onnx",
                    "purpose": "文本检测",
                    "bytes": 4729474u64,
                    "sources": ["https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ch_PP-OCRv4_det.onnx"],
                    "url": "https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ch_PP-OCRv4_det.onnx",
                    "sha256": "69ce850fec741a2a4568c7c924bb025c9d4f1129e5f96ab428c799ccc5ef2275"
                },
                {
                    "name": "ch_PP-OCRv4_rec.onnx",
                    "purpose": "文本识别",
                    "bytes": 10812334u64,
                    "sources": ["https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ch_PP-OCRv4_rec.onnx"],
                    "url": "https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ch_PP-OCRv4_rec.onnx",
                    "sha256": "ad7dd55f6759fa02333bff6eb179a4f51be5b89cbe6f710249c95f47d0211350"
                },
                {
                    "name": "ppocr_keys_v1.txt",
                    "purpose": "CTC 字典",
                    "bytes": 26250u64,
                    "sources": ["https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ppocr_keys_v1.txt"],
                    "url": "https://github.com/Xuancheng75/CrucibleBox/releases/download/document-engine-models-v0.1.2/ppocr_keys_v1.txt",
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

#[derive(Clone, Debug, Default)]
pub struct DocumentEngineConfig {
    pub model_directory: String,
    pub formula_addon_directory: String,
    pub dictionary_path: String,
    pub model_profile: String,
    pub text_recognition_mode: String,
    pub cache_directory: String,
    pub output_directory: String,
    pub device: String,
    /// Host-supplied resource root; never read from plugin configuration JSON.
    resources: Option<PathBuf>,
}

fn load_config_for_service(context: &Service, db: &Db, plugin_id: &str) -> DocumentEngineConfig {
    let mut config = load_config(db, plugin_id);
    config.resources = context.resources.clone();
    config
}

fn load_config(db: &Db, plugin_id: &str) -> DocumentEngineConfig {
    let raw = db
        .plugin_find_by_id(plugin_id)
        .ok()
        .flatten()
        .map(|record| record.config_data)
        .unwrap_or_else(|| "{}".into());
    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    let app_data = std::env::var("APPDATA").unwrap_or_else(|_| "C:\\".into());
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
        resources: None,
        formula_addon_directory: parsed
            .get("formulaAddonDirectory")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        dictionary_path: parsed
            .get("dictionaryPath")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_default(),
        model_profile: parsed
            .get("modelProfile")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("auto")
            .to_string(),
        text_recognition_mode: parsed
            .get("textRecognitionMode")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("auto")
            .to_string(),
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

fn document_stem(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let mut safe = stem
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    while safe.ends_with('.') || safe.ends_with(' ') {
        safe.pop();
    }
    if safe.trim().is_empty() {
        "document".into()
    } else {
        safe
    }
}

/// Convert Document IR outline entries into inclusive PDF page ranges.  A
/// chapter split is deliberately conservative: only headings with a valid
/// page number are considered, and non-increasing/duplicate pages are
/// discarded so a malformed OCR heading cannot create an invalid PDF.
fn chapter_ranges_from_document(document: &Value) -> Vec<(usize, usize)> {
    let page_count = document["metadata"]["pageCount"].as_u64().unwrap_or(0) as usize;
    if page_count == 0 {
        return Vec::new();
    }
    let mut starts = document["structure"]["outline"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["page"].as_u64().map(|page| page as usize))
        .filter(|page| (1..=page_count).contains(page))
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    if starts.first().copied() != Some(1) {
        starts.insert(0, 1);
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts
                .get(index + 1)
                .copied()
                .unwrap_or(page_count)
                .saturating_sub(1)
                .max(*start);
            (*start, end)
        })
        .collect()
}

fn export_parsed_result(
    _document_worker: &cruciblebox_document_worker::client::Client,
    mut result: Value,
    path: &str,
    output_directory: &Path,
    ctx: &crate::document_engine_task::TaskContext,
) -> Result<Value, String> {
    let bundle = crate::document_converter::export_document_bundle_with_publication(
        _document_worker,
        result
            .get("document")
            .ok_or_else(|| "解析器未返回 Document".to_string())?,
        output_directory,
        &document_stem(path),
        Some(ctx.runtime_context()),
    )?;
    result["outputs"] = bundle;
    result["outputDirectory"] = json!(output_directory.to_string_lossy());
    Ok(result)
}

/// Parse a document once and reuse the completed result across parse, chunk,
/// convert and batch operations.  The previous implementation only cached the
/// final `document.parse` response; chunk/convert therefore started a fresh
/// page render + OCR pass every time.  Cache the raw unified document before
/// exporting operation-specific files so every consumer can share it.
fn parse_document_with_cache(
    _document_worker: &cruciblebox_document_worker::client::Client,
    path: &str,
    cfg: &DocumentEngineConfig,
    manager: Option<&OcrWorkerManager>,
    ctx: &TaskContext,
    plugin_id: &str,
) -> Result<Value, String> {
    let source_hash = crate::document_engine_cache::file_hash(Path::new(path))?;
    let cache_key = crate::document_engine_cache::cache_key(
        &source_hash,
        "document-parser",
        "native-parser-v4",
        &json!({
            "ocrModel": ocr_model_version(cfg),
            "pipeline": PIPELINE_VERSION,
            "renderDpi": crate::pdf_parser::PDF_RENDER_DPI
        }),
    );
    if let Ok(Some(cached)) =
        crate::document_engine_cache::read_result(Path::new(&cfg.cache_directory), &cache_key)
    {
        if cached.get("document").is_some() {
            ctx.update_progress(
                "cache",
                100,
                "命中页面解析缓存",
                Some(json!({ "cacheHit": true, "cacheKey": cache_key })),
            );
            return Ok(cached);
        }
    }

    ctx.update_progress("classify", 8, "判断页面类型", None);
    let mut parsed =
        crate::document_parser::parse_file(_document_worker, path, ctx.runtime_context())?;
    if parsed["requiresOcr"].as_bool().unwrap_or(false) {
        let manager = manager.ok_or_else(|| "扫描 PDF 需要已配置 OCR Worker".to_string())?;
        parsed = merge_ocr_pages(_document_worker, parsed, path, manager, ctx, plugin_id, cfg)?;
    }
    let sanitization = crate::document_text::sanitize_document(&mut parsed["document"]);
    let native_quality =
        crate::document_quality::annotate_native_text_quality(&mut parsed["document"]);
    rebuild_document_structure(&mut parsed["document"]);
    let dehyphenation = crate::document_text::repair_body_dehyphenation(&mut parsed["document"]);
    enrich_formula_blocks(&mut parsed["document"]);
    refresh_semantic_metadata(&mut parsed["document"]);
    parsed["document"]["metadata"]["pipeline"] = json!({
        "version": PIPELINE_VERSION,
        "renderDpi": crate::pdf_parser::PDF_RENDER_DPI,
        "textDetection": "ppocrv6_small_det.onnx",
        "textRecognition": ocr_worker_profile(cfg),
        "layout": LAYOUT_MODEL_VERSION,
        "formulaDetection": FORMULA_DETECTION_VERSION,
        "formula": "layout-gated/text-adapter-v1",
        "readingOrder": "column-aware-v1"
    });
    parsed["document"]["metadata"]["quality"] = crate::document_quality::report(
        &parsed["document"],
        sanitization.invalid_control_chars_removed,
    );
    parsed["document"]["metadata"]["nativeTextQuality"] = native_quality;
    parsed["document"]["metadata"]["dehyphenation"] = json!({
        "count": dehyphenation.merged_count,
        "explicitHyphenCount": dehyphenation.explicit_hyphen_count,
        "inferredSplitCount": dehyphenation.inferred_split_count,
    });
    ctx.check_cancelled()?;
    ctx.update_progress("quality", 96, "检查解析质量", None);
    let _ = crate::document_engine_cache::write_result(
        Path::new(&cfg.cache_directory),
        &cache_key,
        &parsed,
    );
    Ok(parsed)
}

fn block_bbox(block: &Value) -> Option<[f32; 4]> {
    let values = block.get("bbox")?.as_array()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
        values[3].as_f64()? as f32,
    ])
}

/// Restore reading order for OCR blocks.  A large horizontal gap is treated
/// as a column break; blocks inside each column remain top-to-bottom.  This
/// deterministic fallback keeps the pipeline useful without requiring a
/// heavyweight layout model.
fn restore_reading_order(blocks: &mut [Value], page_width: u32) {
    let page_width = page_width.max(1) as f32;
    let mut centers = blocks
        .iter()
        .filter_map(block_bbox)
        .map(|bbox| (bbox[0] + bbox[2]) / 2.0)
        .collect::<Vec<_>>();
    centers.sort_by(|left, right| left.total_cmp(right));
    let split = centers
        .windows(2)
        .max_by(|left, right| (left[1] - left[0]).total_cmp(&(right[1] - right[0])));
    let column_gap = split.map(|pair| pair[1] - pair[0]).unwrap_or(0.0);
    let column_break = if column_gap > page_width * 0.16 {
        split.map(|pair| (pair[0] + pair[1]) / 2.0)
    } else {
        None
    };
    blocks.sort_by(|left, right| {
        let left_bbox = block_bbox(left).unwrap_or([0.0, 0.0, 0.0, 0.0]);
        let right_bbox = block_bbox(right).unwrap_or([0.0, 0.0, 0.0, 0.0]);
        let left_center = (left_bbox[0] + left_bbox[2]) / 2.0;
        let right_center = (right_bbox[0] + right_bbox[2]) / 2.0;
        let left_column = column_break
            .map(|boundary| usize::from(left_center >= boundary))
            .unwrap_or(0);
        let right_column = column_break
            .map(|boundary| usize::from(right_center >= boundary))
            .unwrap_or(0);
        left_column
            .cmp(&right_column)
            .then_with(|| left_bbox[1].total_cmp(&right_bbox[1]))
            .then_with(|| left_bbox[0].total_cmp(&right_bbox[0]))
    });
}

/// RapidOCR already orders recognized text. Keep that sequence, especially for
/// tables and headers, and place formula candidates into it by page position.
fn insert_formulas_into_ocr_order(blocks: &mut Vec<Value>) {
    let mut formulas = Vec::new();
    let mut text = Vec::new();
    for block in blocks.drain(..) {
        if block["type"] == "formula" {
            formulas.push(block);
        } else {
            text.push(block);
        }
    }
    formulas.sort_by(|left, right| {
        let left_bbox = block_bbox(left).unwrap_or([0.0; 4]);
        let right_bbox = block_bbox(right).unwrap_or([0.0; 4]);
        left_bbox[1]
            .total_cmp(&right_bbox[1])
            .then_with(|| left_bbox[0].total_cmp(&right_bbox[0]))
    });
    for formula in formulas {
        let bbox = block_bbox(&formula).unwrap_or([0.0; 4]);
        let center_y = (bbox[1] + bbox[3]) / 2.0;
        let position = text.iter().position(|block| {
            let other = block_bbox(block).unwrap_or([0.0; 4]);
            other[1] > center_y
                || (other[1] <= center_y && other[3] >= center_y && other[0] > bbox[0])
        });
        text.insert(position.unwrap_or(text.len()), formula);
    }
    *blocks = text;
}

fn rebuild_document_structure(document: &mut Value) {
    crate::document_structure::rebuild(document);
    if let Some(pages) = document.get_mut("pages").and_then(Value::as_array_mut) {
        for page in pages {
            if let Some(blocks) = page.get_mut("blocks").and_then(Value::as_array_mut) {
                crate::pdf_parser::coalesce_native_text_fragments(blocks);
            }
        }
    }
}

pub(crate) use cruciblebox_document::enrichment::enrich_formula_blocks;

fn refresh_semantic_metadata(document: &mut Value) {
    let existing_has_images = document["metadata"]["hasImages"].as_bool().unwrap_or(false);
    let existing_has_tables = document["metadata"]["hasTables"].as_bool().unwrap_or(false);
    let mut formula_count = 0usize;
    let mut matrix_count = 0usize;
    let mut image_count = 0usize;
    let mut table_count = 0usize;
    if let Some(pages) = document["pages"].as_array() {
        for block in pages
            .iter()
            .filter_map(|page| page["blocks"].as_array())
            .flatten()
        {
            match block["type"].as_str().unwrap_or_default() {
                "formula" => formula_count += 1,
                "matrix" => {
                    formula_count += 1;
                    matrix_count += 1;
                }
                "image" | "figure" => image_count += 1,
                "table" => table_count += 1,
                _ => {}
            }
        }
    }
    document["metadata"]["hasFormulas"] = json!(formula_count > 0);
    document["metadata"]["hasImages"] = json!(existing_has_images || image_count > 0);
    document["metadata"]["hasTables"] = json!(existing_has_tables || table_count > 0);
    document["metadata"]["formulaBlockCount"] = json!(formula_count);
    document["metadata"]["matrixBlockCount"] = json!(matrix_count);
    document["metadata"]["imageBlockCount"] = json!(image_count);
    document["metadata"]["tableBlockCount"] = json!(table_count);
}

fn ocr_model_version(cfg: &DocumentEngineConfig) -> String {
    let directory =
        if effective_formula_profile(cfg).is_some_and(|profile| profile.starts_with("onnx-")) {
            formula_model_directory(cfg)
        } else {
            PathBuf::from(&cfg.model_directory)
        };
    let profile = ocr_worker_profile(cfg);
    let legacy = profile.contains("v4");
    let english = profile.contains("en-rec");
    let detection_name = if legacy {
        "ch_PP-OCRv4_det.onnx"
    } else {
        "ppocrv6_small_det.onnx"
    };
    let recognition_name = if legacy {
        "ch_PP-OCRv4_rec.onnx"
    } else if english {
        "en_PP-OCRv5_mobile_rec.onnx"
    } else {
        "PP-OCRv5_mobile_rec.onnx"
    };
    let dictionary_name = if legacy {
        "ppocr_keys_v1.txt"
    } else if english {
        "en_ppocrv5_dict.txt"
    } else {
        "ppocrv5_dict.txt"
    };
    let artifacts = [
        (detection_name, directory.join(detection_name)),
        (recognition_name, directory.join(recognition_name)),
        (
            dictionary_name,
            if cfg.dictionary_path.trim().is_empty() {
                directory.join(dictionary_name)
            } else {
                PathBuf::from(&cfg.dictionary_path)
            },
        ),
    ];
    let parts = artifacts
        .iter()
        .map(|(name, path)| {
            format!(
                "{name}:{}",
                crate::document_engine_cache::file_hash(path).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    let formula_parts = if effective_formula_profile(cfg) == Some(FORMULANET_MODEL_PROFILE) {
        let formula_directory = formula_model_directory(cfg);
        ["PP-DocLayout-M.onnx", "pp_formulanet_plus_s.onnx"]
            .iter()
            .map(|name| {
                format!(
                    "{name}:{}",
                    crate::document_engine_cache::file_hash(
                        &formula_directory.join("formula-onnx").join(name)
                    )
                    .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    } else if matches!(
        effective_formula_profile(cfg),
        Some("pp-doclayout-m-formulanet-plus-l" | "pp-doclayout-m-unimernet")
    ) {
        let formula_directory = paddle_formula_directory(cfg);
        let formula_name = if effective_formula_profile(cfg) == Some("pp-doclayout-m-unimernet") {
            "UniMERNet"
        } else {
            "PP-FormulaNet_plus-L"
        };
        ["PP-DocLayout-M", formula_name]
            .iter()
            .map(|name| {
                format!(
                    "{name}:{}",
                    crate::document_engine_cache::file_hash(
                        &formula_directory
                            .join("paddle-formula")
                            .join(name)
                            .join("inference.pdiparams")
                    )
                    .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    } else {
        String::new()
    };
    format!(
        "ocr-worker-v5:{}:{}:{}:{}:{}:{}:{}",
        ocr_worker_profile(cfg),
        cfg.text_recognition_mode,
        LAYOUT_MODEL_VERSION,
        FORMULA_DETECTION_VERSION,
        effective_formula_profile(cfg).unwrap_or(OCR_CONFIG_VERSION),
        parts.join("|"),
        formula_parts
    )
}

fn ocr_language_for_config(cfg: &DocumentEngineConfig) -> &'static str {
    if cfg.text_recognition_mode.eq_ignore_ascii_case("english")
        || cfg.model_profile.to_ascii_lowercase().contains("en-rec")
    {
        "en"
    } else {
        "mix"
    }
}

fn ocr_worker_profile(cfg: &DocumentEngineConfig) -> String {
    let configured = cfg.model_profile.trim();
    if formula_model_enabled(cfg) {
        return DEFAULT_MODEL_ID.into();
    }
    if !configured.is_empty() && !configured.eq_ignore_ascii_case("auto") {
        if configured.eq_ignore_ascii_case("english") || configured.eq_ignore_ascii_case("en") {
            return "ppocrv6-small-det-v5-en-rec".into();
        }
        return configured.into();
    }
    if cfg.text_recognition_mode.eq_ignore_ascii_case("english") {
        "ppocrv6-small-det-v5-en-rec".into()
    } else {
        DEFAULT_MODEL_ID.into()
    }
}

fn effective_formula_profile(cfg: &DocumentEngineConfig) -> Option<&str> {
    let selected = cfg.model_profile.as_str();
    if selected.eq_ignore_ascii_case("standard") {
        return Some("pp-doclayout-m-unimernet");
    }
    if selected.eq_ignore_ascii_case("fast") || selected.eq_ignore_ascii_case(FAST_MODEL_PROFILE) {
        return Some(FAST_MODEL_PROFILE);
    }
    if selected.eq_ignore_ascii_case(FORMULA_MODEL_PROFILE) {
        return Some(FORMULA_MODEL_PROFILE);
    }
    if selected.eq_ignore_ascii_case(FORMULANET_MODEL_PROFILE) {
        return Some(FORMULANET_MODEL_PROFILE);
    }
    if selected.eq_ignore_ascii_case("pp-doclayout-m-formulanet-s") {
        return Some("pp-doclayout-m-formulanet-s");
    }
    if selected.eq_ignore_ascii_case("pp-doclayout-m-formulanet-plus-s") {
        return Some("pp-doclayout-m-formulanet-plus-s");
    }
    if selected.eq_ignore_ascii_case("pp-doclayout-m-formulanet-plus-l") {
        return Some("pp-doclayout-m-formulanet-plus-l");
    }
    if selected.eq_ignore_ascii_case("pp-doclayout-m-unimernet") {
        return Some("pp-doclayout-m-unimernet");
    }
    if selected.eq_ignore_ascii_case("auto")
        && formula_python(cfg).is_some()
        && formula_model_directory(cfg)
            .join("formula-onnx/pp_formulanet_plus_s.onnx")
            .is_file()
        && formula_model_directory(cfg)
            .join("formula-onnx/PP-DocLayout-M.onnx")
            .is_file()
    {
        return Some(FORMULANET_MODEL_PROFILE);
    }
    None
}

fn formula_model_enabled(cfg: &DocumentEngineConfig) -> bool {
    effective_formula_profile(cfg).is_some()
}

fn formula_worker_manager(cfg: &DocumentEngineConfig) -> Result<Option<OcrWorkerManager>, String> {
    if !formula_model_enabled(cfg) {
        return Ok(None);
    }
    let selected = effective_formula_profile(cfg).ok_or("没有可用的公式模型")?;
    let low_memory = selected == FAST_MODEL_PROFILE
        || selected == FORMULA_MODEL_PROFILE
        || selected == FORMULANET_MODEL_PROFILE;
    let high_addon_root = (!low_memory)
        .then(|| paddle_formula_directory(cfg))
        .and_then(|directory| directory.parent().map(Path::to_path_buf));
    let python_env = if low_memory {
        "CRUCIBLEBOX_FORMULA_ONNX_PYTHON"
    } else {
        "CRUCIBLEBOX_FORMULA_PYTHON"
    };
    let script_env = if low_memory {
        "CRUCIBLEBOX_FORMULA_ONNX_WORKER_SCRIPT"
    } else {
        "CRUCIBLEBOX_FORMULA_WORKER_SCRIPT"
    };
    let script_name = if low_memory {
        "formula-ocr-onnx-worker.py"
    } else {
        "formula-ocr-worker.py"
    };
    let python = if low_memory {
        formula_python(cfg)
    } else {
        std::env::var_os(python_env)
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                high_addon_root
                    .as_ref()
                    .map(|root| root.join("python/python.exe"))
                    .filter(|path| path.is_file())
            })
            .or_else(|| {
                cfg.resources
                    .as_ref()
                    .map(|root| root.join("formula-ocr/high/python/python.exe"))
                    .filter(|path| path.is_file())
            })
    }
    .ok_or_else(|| format!("公式模式需要受管 Python 运行时 ({python_env})"))?;
    let script = std::env::var_os(script_env)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            cfg.resources
                .as_ref()
                .map(|root| {
                    if low_memory {
                        root.join("formula-ocr").join(script_name)
                    } else {
                        root.join("formula-ocr/high").join(script_name)
                    }
                })
                .filter(|path| path.is_file())
                .or_else(|| {
                    high_addon_root
                        .as_ref()
                        .map(|root| root.join(script_name))
                        .filter(|path| path.is_file())
                })
                .unwrap_or_else(|| {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../scripts")
                        .join(script_name)
                })
        });
    if !python.is_file() || !script.is_file() {
        return Err(format!(
            "高精度公式运行时不完整：Python={}，worker={}",
            python.display(),
            script.display()
        ));
    }
    Ok(Some(OcrWorkerManager::new_external(
        python,
        vec![script.to_string_lossy().into_owned()],
        std::time::Duration::from_secs(if low_memory { 300 } else { 900 }),
    )))
}

fn ocr_model_identity(cfg: &DocumentEngineConfig) -> Value {
    let directory = PathBuf::from(&cfg.model_directory);
    let profile = ocr_worker_profile(cfg);
    let legacy = profile.contains("v4");
    let english = profile.contains("en-rec");
    let paths = [
        directory.join(if legacy {
            "ch_PP-OCRv4_det.onnx"
        } else {
            "ppocrv6_small_det.onnx"
        }),
        directory.join(if legacy {
            "ch_PP-OCRv4_rec.onnx"
        } else if english {
            "en_PP-OCRv5_mobile_rec.onnx"
        } else {
            "PP-OCRv5_mobile_rec.onnx"
        }),
        if cfg.dictionary_path.trim().is_empty() {
            directory.join(if legacy {
                "ppocr_keys_v1.txt"
            } else if english {
                "en_ppocrv5_dict.txt"
            } else {
                "ppocrv5_dict.txt"
            })
        } else {
            PathBuf::from(&cfg.dictionary_path)
        },
    ];
    json!({
        "profile": profile,
        "configVersion": OCR_CONFIG_VERSION,
        "modelVersion": ocr_model_version(cfg),
        "detectionPath": paths[0].to_string_lossy(),
        "recognitionPath": paths[1].to_string_lossy(),
        "dictionaryPath": paths[2].to_string_lossy(),
        "detectionSha256": crate::document_engine_cache::file_hash(&paths[0]).unwrap_or_default(),
        "recognitionSha256": crate::document_engine_cache::file_hash(&paths[1]).unwrap_or_default(),
        "dictionarySha256": crate::document_engine_cache::file_hash(&paths[2]).unwrap_or_default(),
    })
}

fn gpu_status(context: &Service) -> Value {
    context.gpu_status.get_or_init(gpu_status_uncached).clone()
}

fn gpu_status_uncached() -> Value {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut command = Command::new("nvidia-smi");
        command
            .args(["--query-gpu=name,memory.total", "--format=csv,noheader"])
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        match command.output() {
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
    // TaskRecord clamps progress and assigns a sequence number. Emit exactly
    // that canonical snapshot instead of the raw worker frame; otherwise a
    // late 5%/20% frame can overwrite a newer polling snapshot in the UI.
    let canonical = ctx.progress_snapshot();
    ctx.emit_progress(plugin_id, &canonical);
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
    let integrated_profile =
        effective_formula_profile(cfg).filter(|profile| profile.starts_with("onnx-"));
    let integrated_ocr = integrated_profile.is_some();
    let selected_language = language.unwrap_or_else(|| ocr_language_for_config(cfg).to_string());
    let model_directory = if integrated_ocr {
        formula_model_directory(cfg).to_string_lossy().into_owned()
    } else {
        cfg.model_directory.clone()
    };
    let model_profile = integrated_profile
        .map(str::to_string)
        .unwrap_or_else(|| ocr_worker_profile(cfg));
    let source_hash = crate::document_engine_cache::file_hash(PathBuf::from(input).as_path())?;
    let options = json!({
        "language": selected_language,
        "device": device.clone().unwrap_or_else(|| cfg.device.clone()),
        "modelDirectory": model_directory,
        "dictionaryPath": if cfg.dictionary_path.trim().is_empty() { Value::Null } else { json!(cfg.dictionary_path) },
        "modelProfile": model_profile,
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
        ctx.emit_progress(plugin_id, &ctx.progress_snapshot());
        return Ok(cached);
    }

    let request = OcrWorkerRequest::new(
        format!(
            "{}-{}",
            ctx.task_id(),
            source_hash.get(..12).unwrap_or("input")
        ),
        input.to_string(),
        Some(selected_language),
        device.or_else(|| Some(cfg.device.clone())),
        Some(model_directory),
        (!cfg.dictionary_path.trim().is_empty()).then(|| cfg.dictionary_path.clone()),
        Some(model_profile),
    );
    ctx.update_progress(
        "model",
        1,
        "加载 OCR 模型",
        Some(json!({
            "cacheHit": false,
            "ocrEngine": if integrated_ocr { "rapidocr-formula-onnx" } else { "paddleocr-onnx" },
            "model": ocr_model_identity(cfg),
            "language": request.options.as_ref().and_then(|options| options.language.clone()),
        })),
    );
    let integrated_manager = if integrated_ocr {
        formula_worker_manager(cfg)?
    } else {
        None
    };
    let selected_manager = integrated_manager.as_ref().unwrap_or(manager);
    let result = selected_manager.run(&request, ctx.cancel_flag(), &|progress| {
        // A paused task intentionally blocks the worker frame loop here. This
        // is the checkpoint boundary between OCR pages/frames and avoids
        // reporting "paused" while the native worker is still consuming CPU.
        if report_ocr_progress(ctx, plugin_id, progress, scope).is_err() {
            ctx.cancel_flag()
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    })?;
    ctx.check_cancelled()?;
    report_ocr_progress(
        ctx,
        plugin_id,
        json!({ "stage": "done", "percent": 100, "message": "OCR 完成" }),
        scope,
    )?;
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
    _document_worker: &cruciblebox_document_worker::client::Client,
    mut parsed: Value,
    path: &str,
    manager: &OcrWorkerManager,
    ctx: &TaskContext,
    plugin_id: &str,
    cfg: &DocumentEngineConfig,
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
        let page_total = page_numbers.len();
        let source_hash = crate::document_engine_cache::file_hash(Path::new(path))?;
        let model_version = ocr_model_version(cfg);
        let formula_manager = formula_worker_manager(cfg)?;
        let mut actual_model: Option<Value> = None;
        for (page_index, page_number) in page_numbers.iter().enumerate() {
            ctx.wait_if_paused()?;
            let page_cache_key = crate::document_engine_cache::cache_key(
                &source_hash,
                "ocr-page",
                "ocr-page-v3",
                &json!({
                    "page": page_number,
                    "ocrModel": model_version,
                    "renderDpi": crate::pdf_parser::PDF_RENDER_DPI,
                    "pipeline": PIPELINE_VERSION
                }),
            );
            if let Ok(Some(cached_page)) = crate::document_engine_cache::read_result(
                Path::new(&cfg.cache_directory),
                &page_cache_key,
            ) {
                if let Some(model) = cached_page.get("model") {
                    actual_model = Some(model.clone());
                }
                if let Some(pages) = parsed["document"]["pages"].as_array_mut() {
                    if let Some(page) = pages
                        .iter_mut()
                        .find(|page| page["number"].as_u64() == Some(u64::from(*page_number)))
                    {
                        if let Some(cached_document_page) = cached_page.get("page") {
                            *page = cached_document_page.clone();
                        }
                    }
                }
                ctx.update_progress(
                    "ocr-cache",
                    (((page_index + 1) * 100) / page_total.max(1)) as u32,
                    &format!(
                        "命中 OCR 页面缓存：第 {} / {} 页",
                        page_index + 1,
                        page_total
                    ),
                    Some(json!({
                        "pageIndex": page_index,
                        "pageTotal": page_total,
                        "pageNumber": page_number,
                        "cacheHit": true,
                        "model": cached_page.get("model").cloned().unwrap_or_else(|| ocr_model_identity(cfg))
                    })),
                );
                continue;
            }
            let page_percent = ((page_index * 100) / page_total.max(1)) as u32;
            ctx.update_progress(
                "render",
                page_percent,
                &format!("渲染 PDF 第 {} / {} 页", page_index + 1, page_total),
                Some(json!({
                    "pageIndex": page_index,
                    "pageTotal": page_total,
                    "pageNumber": page_number
                })),
            );
            let rendered = temp_dir.join(format!("page-{page_number}.png"));
            let dimensions = crate::pdf_parser::render_page_to_png(
                _document_worker,
                path,
                *page_number,
                &rendered,
                ctx.runtime_context(),
            )?;
            let integrated_ocr = formula_manager.is_some()
                && effective_formula_profile(cfg)
                    .is_some_and(|profile| profile.starts_with("onnx-"));
            let ocr = if integrated_ocr {
                let request = OcrWorkerRequest::new(
                    format!("{}-page-{page_number}", ctx.task_id()),
                    rendered.to_string_lossy().into_owned(),
                    Some(ocr_language_for_config(cfg).to_string()),
                    Some("cpu".into()),
                    Some(formula_model_directory(cfg).to_string_lossy().into_owned()),
                    None,
                    Some(
                        effective_formula_profile(cfg)
                            .unwrap_or_default()
                            .to_string(),
                    ),
                );
                formula_manager.as_ref().ok_or("公式模型不可用")?.run(
                    &request,
                    ctx.cancel_flag(),
                    &|progress| {
                        let _ = report_ocr_progress(
                            ctx,
                            plugin_id,
                            progress,
                            Some((page_index, page_total)),
                        );
                    },
                )?
            } else {
                run_ocr_input(
                    manager,
                    ctx,
                    plugin_id,
                    cfg,
                    &rendered.to_string_lossy(),
                    Some(ocr_language_for_config(cfg).to_string()),
                    None,
                    Some((page_index, page_total)),
                )?
            };
            if let Some(model) = ocr.get("model") {
                actual_model = Some(model.clone());
            }
            let ocr_blocks = ocr["blocks"].as_array().cloned().unwrap_or_default();
            let mut blocks = Vec::with_capacity(ocr_blocks.len());
            for (block_index, block) in ocr_blocks.iter().enumerate() {
                if block["type"] == "formula" {
                    continue;
                }
                let content = block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if content.is_empty() {
                    continue;
                }
                let region_kind = crate::document_layout::classify_fallback(
                    content,
                    block_bbox(block),
                    dimensions.1 as f32,
                );
                let block_type = region_kind.as_str();
                let formula_result = (block_type == "formula").then(|| {
                    crate::formula_ocr::recognize_region(&crate::document_layout::FormulaRegion {
                        page: *page_number as usize,
                        bbox: block_bbox(block),
                        text: content.to_string(),
                        display: true,
                    })
                });
                let normalized_content = formula_result
                    .as_ref()
                    .map(|result| result.normalized_latex.clone())
                    .unwrap_or_else(|| content.to_string());
                let level = if block_type == "heading" {
                    let lower = content.to_ascii_lowercase();
                    if lower.starts_with("chapter ") || content.starts_with('第') {
                        Some(1u8)
                    } else {
                        Some(2u8)
                    }
                } else {
                    None
                };
                blocks.push(json!({
                    "id": format!("p{page_number}-b{}", block_index + 1),
                    "type": block_type,
                    "content": normalized_content,
                    "rawText": content,
                    "latex": formula_result.as_ref().map(|result| result.normalized_latex.clone()),
                    "rawLatex": formula_result.as_ref().map(|result| result.raw_latex.clone()),
                    "normalizedLatex": formula_result.as_ref().map(|result| result.normalized_latex.clone()),
                    "plainText": formula_result.as_ref().map(|result| result.plain_text.clone()),
                    "formulaEngine": formula_result.as_ref().map(|result| result.engine.clone()),
                    "formulaModelVersion": formula_result.as_ref().map(|result| result.model_version.clone()),
                    "formulaConfidence": formula_result.as_ref().map(|result| result.confidence),
                    "displayOrInline": formula_result.as_ref().map(|result| result.display_or_inline),
                    "level": level,
                    "semanticType": if block_type == "heading" { json!("section_heading") } else { Value::Null },
                    "region": block_type,
                    "source": if block_type == "formula" { "ocr/formula_ocr" } else { "ocr" },
                    "excludedFromRag": region_kind.excluded_from_rag(),
                    "ocrNoiseCandidate": crate::document_quality::is_ocr_noise_candidate(
                        content,
                        block.get("confidence").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        block_type,
                        block_bbox(block),
                        dimensions.1 as f32,
                    ),
                    "bbox": block.get("bbox").cloned().unwrap_or(Value::Null),
                    "polygon": block.get("polygon").cloned().unwrap_or(Value::Null),
                    "confidence": block.get("confidence").cloned().unwrap_or(Value::Null),
                    "language": if content.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) { "zh" } else { "en" },
                }));
            }
            if let Some(formula_manager) = formula_manager.as_ref() {
                let request = OcrWorkerRequest::new(
                    format!("{}-formula-{page_number}", ctx.task_id()),
                    rendered.to_string_lossy().into_owned(),
                    None,
                    Some("cpu".into()),
                    Some(
                        if effective_formula_profile(cfg)
                            .is_some_and(|profile| profile.starts_with("pp-doclayout"))
                        {
                            paddle_formula_directory(cfg).to_string_lossy().into_owned()
                        } else {
                            formula_model_directory(cfg).to_string_lossy().into_owned()
                        },
                    ),
                    None,
                    Some(
                        effective_formula_profile(cfg)
                            .unwrap_or_default()
                            .to_string(),
                    ),
                );
                let formula_response = if integrated_ocr {
                    ocr.clone()
                } else {
                    formula_manager.run(&request, ctx.cancel_flag(), &|progress| {
                        let _ = report_ocr_progress(
                            ctx,
                            plugin_id,
                            progress,
                            Some((page_index, page_total)),
                        );
                    })?
                };
                let formula_blocks = formula_response["blocks"]
                    .as_array()
                    .ok_or("高精度公式 worker 未返回公式块数组")?;
                if formula_blocks
                    .iter()
                    .filter(|block| block["type"] == "formula")
                    .count()
                    > 256
                {
                    return Err("高精度公式块数量超过页面预算".into());
                }
                for (formula_index, formula) in formula_blocks.iter().enumerate() {
                    if formula["type"] != "formula" {
                        continue;
                    }
                    let Some(latex) = formula["text"].as_str().filter(|text| !text.is_empty())
                    else {
                        continue;
                    };
                    let Some(bbox) = block_bbox(formula) else {
                        continue;
                    };
                    if latex.len() > 16_384
                        || bbox[0] < 0.0
                        || bbox[1] < 0.0
                        || bbox[2] > dimensions.0 as f32
                        || bbox[3] > dimensions.1 as f32
                        || bbox[2] <= bbox[0]
                        || bbox[3] <= bbox[1]
                    {
                        continue;
                    }
                    blocks.push(json!({
                        "id": format!("p{page_number}-f{}", formula_index + 1),
                        "type": "formula",
                        "content": latex,
                        "rawText": latex,
                        "latex": latex,
                        "rawLatex": latex,
                        "normalizedLatex": latex,
                        "plainText": latex,
                        "formulaEngine": formula["formulaEngine"],
                        "formulaModelVersion": formula["modelVersion"],
                        "formulaConfidence": formula["confidence"],
                        "recognitionConfidence": Value::Null,
                        "requiresReview": true,
                        "displayOrInline": "display",
                        "level": Value::Null,
                        "semanticType": Value::Null,
                        "region": "formula",
                        "source": "ocr/formula-model",
                        "excludedFromRag": false,
                        "ocrNoiseCandidate": false,
                        "bbox": formula["bbox"],
                        "polygon": formula["polygon"],
                        "confidence": formula["confidence"],
                        "language": "math",
                    }));
                }
            }
            if integrated_ocr {
                insert_formulas_into_ocr_order(&mut blocks);
            } else {
                restore_reading_order(&mut blocks, dimensions.0);
            }
            let page_value = json!({
                "number": page_number,
                "width": dimensions.0,
                "height": dimensions.1,
                "blocks": blocks
            });
            if let Some(pages) = parsed["document"]["pages"].as_array_mut() {
                if let Some(page) = pages
                    .iter_mut()
                    .find(|page| page["number"].as_u64() == Some(u64::from(*page_number)))
                {
                    *page = page_value.clone();
                }
            }
            let _ = crate::document_engine_cache::write_result(
                Path::new(&cfg.cache_directory),
                &page_cache_key,
                &json!({ "page": page_value, "model": ocr.get("model").cloned().unwrap_or_else(|| ocr_model_identity(cfg)) }),
            );
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
        parsed["document"]["metadata"]["ocrModel"] =
            actual_model.unwrap_or_else(|| ocr_model_identity(cfg));
        parsed["document"]["metadata"]["pipeline"] = json!({
            "version": PIPELINE_VERSION,
            "renderDpi": crate::pdf_parser::PDF_RENDER_DPI,
            "ocrLanguage": ocr_language_for_config(cfg),
            "ocrModel": parsed["document"]["metadata"]["ocrModel"],
            "layout": LAYOUT_MODEL_VERSION,
            "formulaDetection": FORMULA_DETECTION_VERSION,
            "formula": effective_formula_profile(cfg).unwrap_or("layout-gated/text-adapter-v1"),
            "readingOrder": "column-aware-v1"
        });
        rebuild_document_structure(&mut parsed["document"]);
        parsed["document"]["metadata"]["hasFormulas"] = json!(parsed["document"]["pages"]
            .as_array()
            .map(|pages| {
                pages.iter().any(|page| {
                    page["blocks"]
                        .as_array()
                        .is_some_and(|blocks| blocks.iter().any(|block| block["type"] == "formula"))
                })
            })
            .unwrap_or(false));
        Ok(parsed)
    })();
    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

fn handle_message(context: &Service, db: &Db, plugin_id: &str, payload: &Value) -> Value {
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
        "document.runtime.install" => {
            let source = match str_field(request, "directory", 32 * 1024) {
                Ok(Some(path)) => path,
                Ok(None) => return err("invalid-value", "missing directory".into()),
                Err(error) => return error,
            };
            match context.install_document_runtime(Path::new(source)) {
                Ok(result) => result,
                Err(message) => err("document-runtime-install-failed", message),
            }
        }

        "document.ocr.preview" => {
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(Some(path)) => path,
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            match ocr_preview(Path::new(path)) {
                Ok(preview) => preview,
                Err(message) => err("ocr-preview-failed", message),
            }
        }
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
            let cfg = load_config_for_service(context, db, plugin_id);
            let default_model =
                ensure_default_model(db, plugin_id, Path::new(&cfg.model_directory));
            let cache_entries = crate::document_engine_cache::list_files(std::path::Path::new(
                &cfg.cache_directory,
            ))
            .map(|entries| entries.len())
            .unwrap_or(0);
            let worker = context
                .worker
                .as_ref()
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
                    "pdfium": crate::pdf_parser::renderer_status(&context.document_client()),
                    "models": {
                        "default": default_model,
                        "directory": cfg.model_directory,
                        "formula": {
                            "profile": effective_formula_profile(&cfg),
                            "lowMemoryReady": formula_python(&cfg).is_some()
                                && formula_model_directory(&cfg).join("formula-onnx/PP-DocLayout-M.onnx").is_file()
                                && formula_model_directory(&cfg).join("formula-onnx/pp_formulanet_plus_s.onnx").is_file(),
                            "highPrecisionReady": high_precision_runtime_ready(&cfg),
                            "standardReady": unimernet_runtime_ready(&cfg),
                            "deepReady": false,
                            "mode": match cfg.model_profile.as_str() { "fast" => "fast", "standard" => "standard", _ => "legacy" },
                        },
                    },
                    "gpu": gpu_status(context),
                    "workers": {
                        "ocr": worker.get("running").and_then(Value::as_bool).unwrap_or(false) as u8,
                        "parser": 0,
                        "converter": 0
                    },
                    "config": {
                        "device": cfg.device,
                        "modelDirectory": cfg.model_directory,
                        "formulaAddonDirectory": cfg.formula_addon_directory,
                        "dictionaryPath": cfg.dictionary_path,
                        "modelProfile": cfg.model_profile,
                        "textRecognitionMode": cfg.text_recognition_mode,
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
                        "chunk": ["hybrid", "pages", "chapters", "structure", "semantic"],
                        "pdfSplit": ["pages", "fixed", "ranges", "chapters", "custom"],
                        "batch": ["ocr", "parse", "convert"]
                    }
                }
            })
        }
        "document.jobs.list" => {
            let list = context.tasks.list();
            json!({ "tasks": list })
        }
        "document.jobs.get" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id.to_string(),
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(e) => return e,
            };
            match context.tasks.get(&task_id) {
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
                context.tasks.active_task(RESOURCE_OCR).as_deref() == Some(task_id.as_str());
            if is_ocr_task {
                // Stop the process before releasing the resource slot.  This
                // prevents a new OCR task from starting and then being killed
                // by the cancellation of its predecessor.
                if let Some(manager) = context.worker.as_ref() {
                    manager.cancel_current();
                }
            }
            if context.tasks.cancel(&task_id) {
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
            let cfg = load_config_for_service(context, db, plugin_id);
            let manager = match context.worker.as_ref() {
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
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
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
            remember_retry(context, &task_id, request);
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
            let parse_options = request.get("options").cloned().unwrap_or(Value::Null);
            if !parse_options.is_null() && !parse_options.is_object() {
                return err("invalid-value", "options must be an object".into());
            }
            let parse_cfg = load_config_for_service(context, db, plugin_id);
            let parse_output_directory = parse_options
                .get("outputDirectory")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(&parse_cfg.output_directory));
            let parse_manager = context.worker.as_ref().cloned();
            let parse_plugin_id = plugin_id.to_string();
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
                RESOURCE_PARSE,
                Box::new(move |ctx| {
                    ctx.update_progress("parse", 5, "读取文档", None);
                    let parsed = parse_document_with_cache(
                        &_document_worker,
                        &path,
                        &parse_cfg,
                        parse_manager.as_deref(),
                        ctx,
                        &parse_plugin_id,
                    )?;
                    let result = export_parsed_result(
                        &_document_worker,
                        parsed,
                        &path,
                        &parse_output_directory,
                        ctx,
                    )?;
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
                    Ok(result)
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(context, &task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 7：PDF 物理拆分 ----
        "document.pdf.split" | "document.split" => {
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(Some(path)) => path.to_string(),
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            if !Path::new(&path)
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
            {
                return err("unsupported-format", "PDF 拆分只支持 .pdf 文件".into());
            }
            let options = request.get("options").cloned().unwrap_or(Value::Null);
            if !options.is_null() && !options.is_object() {
                return err("invalid-value", "options must be an object".into());
            }
            let cfg = load_config_for_service(context, db, plugin_id);
            let output_directory = options
                .get("outputDirectory")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(&cfg.output_directory));
            let pages_per_file = options
                .get("pagesPerFile")
                .and_then(Value::as_u64)
                .unwrap_or(50) as usize;
            if !(1..=crate::pdf_parser::MAX_PDF_PAGES).contains(&pages_per_file) {
                return err(
                    "invalid-value",
                    format!(
                        "pagesPerFile 必须在 1..={} 之间",
                        crate::pdf_parser::MAX_PDF_PAGES
                    ),
                );
            }
            let split_mode = options
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("fixed")
                .to_ascii_lowercase();
            let explicit_ranges = options
                .get("ranges")
                .and_then(Value::as_array)
                .map(|ranges| {
                    ranges
                        .iter()
                        .filter_map(|range| {
                            let start = range.get("start").and_then(Value::as_u64)? as usize;
                            let end = range.get("end").and_then(Value::as_u64)? as usize;
                            Some((start, end))
                        })
                        .collect::<Vec<_>>()
                });
            if matches!(split_mode.as_str(), "range" | "ranges" | "custom")
                && explicit_ranges.as_ref().is_none_or(Vec::is_empty)
            {
                return err("invalid-value", "range/custom 模式需要 ranges 数组".into());
            }
            let split_cfg = load_config_for_service(context, db, plugin_id);
            let split_manager = context.worker.as_ref().cloned();
            let split_plugin_id = plugin_id.to_string();
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
                RESOURCE_SPLIT,
                Box::new(move |ctx| {
                    ctx.update_progress("split", 5, "读取 PDF 页面", None);
                    let result = if matches!(split_mode.as_str(), "range" | "ranges" | "custom") {
                        crate::pdf_parser::split_pdf_file_with_ranges_with_publication(
                            &_document_worker,
                            &path,
                            &output_directory,
                            explicit_ranges.as_deref().unwrap_or_default(),
                            Some(ctx.runtime_context()),
                        )?
                    } else if split_mode == "chapters" {
                        let parsed = parse_document_with_cache(
                            &_document_worker,
                            &path,
                            &split_cfg,
                            split_manager.as_deref(),
                            ctx,
                            &split_plugin_id,
                        )?;
                        let ranges = chapter_ranges_from_document(&parsed["document"]);
                        if ranges.is_empty() {
                            crate::pdf_parser::split_pdf_file_with_publication(
                                &_document_worker,
                                &path,
                                &output_directory,
                                pages_per_file,
                                Some(ctx.runtime_context()),
                            )?
                        } else {
                            crate::pdf_parser::split_pdf_file_with_ranges_with_publication(
                                &_document_worker,
                                &path,
                                &output_directory,
                                &ranges,
                                Some(ctx.runtime_context()),
                            )?
                        }
                    } else if split_mode == "pages" {
                        crate::pdf_parser::split_pdf_file_with_publication(
                            &_document_worker,
                            &path,
                            &output_directory,
                            1,
                            Some(ctx.runtime_context()),
                        )?
                    } else {
                        crate::pdf_parser::split_pdf_file_with_publication(
                            &_document_worker,
                            &path,
                            &output_directory,
                            pages_per_file,
                            Some(ctx.runtime_context()),
                        )?
                    };
                    ctx.check_cancelled()?;
                    ctx.update_progress(
                        "split",
                        100,
                        "PDF 拆分完成",
                        Some(json!({
                            "pageCount": result["pageCount"],
                            "fileCount": result["fileCount"]
                        })),
                    );
                    Ok(result)
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(context, &task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        "document.pdf.merge" | "document.pdf.reorder" | "document.pdf.rotate" => {
            let operation = request
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let output = match str_field(request, "outputPath", 32 * 1024) {
                Ok(Some(path)) if path.to_ascii_lowercase().ends_with(".pdf") => {
                    PathBuf::from(path)
                }
                Ok(_) => return err("invalid-value", "请提供 .pdf 输出文件路径".into()),
                Err(error) => return error,
            };
            let paths = request
                .get("paths")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let path = request
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let pages = request
                .get("pages")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_u64)
                        .map(|page| page as usize)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let degrees = request.get("degrees").and_then(Value::as_u64).unwrap_or(90) as u16;
            if operation == "document.pdf.merge" && paths.len() < 2 {
                return err("invalid-value", "PDF 合并至少需要两个源文件".into());
            }
            if operation == "document.pdf.reorder" && (path.is_empty() || pages.is_empty()) {
                return err("invalid-value", "PDF 重排需要源文件及页码顺序".into());
            }
            if operation == "document.pdf.rotate" && path.is_empty() {
                return err("invalid-value", "PDF 旋转需要源文件".into());
            }
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
                RESOURCE_SPLIT,
                Box::new(move |ctx| {
                    ctx.update_progress("pdf-pages", 10, "读取 PDF 页面", None);
                    let result = if operation == "document.pdf.merge" {
                        crate::pdf_parser::merge_pdf_files_with_publication(
                            &_document_worker,
                            &paths,
                            &output,
                            Some(ctx.runtime_context()),
                        )?
                    } else if operation == "document.pdf.rotate" {
                        crate::pdf_parser::rotate_pdf_pages_with_publication(
                            &_document_worker,
                            &path,
                            &pages,
                            degrees,
                            &output,
                            Some(ctx.runtime_context()),
                        )?
                    } else {
                        crate::pdf_parser::reorder_pdf_pages_with_publication(
                            &_document_worker,
                            &path,
                            &pages,
                            &output,
                            Some(ctx.runtime_context()),
                        )?
                    };
                    ctx.update_progress("pdf-pages", 100, "PDF 页面处理完成", None);
                    Ok(result)
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(context, &task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        "document.pdf.extractImages" => {
            let path = match str_field(request, "path", 32 * 1024) {
                Ok(Some(path)) => path.to_string(),
                Ok(None) => return err("invalid-value", "请选择 PDF 文件".into()),
                Err(error) => return error,
            };
            let output_directory = match str_field(request, "outputDirectory", 32 * 1024) {
                Ok(Some(path)) => PathBuf::from(path),
                Ok(None) => return err("invalid-value", "请选择图片输出目录".into()),
                Err(error) => return error,
            };
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
                RESOURCE_SPLIT,
                Box::new(move |ctx| {
                    ctx.update_progress("pdf-images", 10, "提取 PDF 内嵌图片", None);
                    let result = crate::pdf_parser::extract_pdf_images_with_publication(
                        &_document_worker,
                        &path,
                        &output_directory,
                        Some(ctx.runtime_context()),
                    )?;
                    ctx.update_progress("pdf-images", 100, "图片提取完成", None);
                    Ok(result)
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(context, &task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 8：统一模型切分 ----
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
            let chunk_cfg = load_config_for_service(context, db, plugin_id);
            // A chunk run may override the configured destination.  This lets
            // the renderer offer a folder picker while retaining the stable
            // plugin default for API callers that do not provide one.
            let output_directory = options
                .as_ref()
                .and_then(|value| value.get("outputDirectory"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| chunk_cfg.output_directory.clone());
            let chunk_manager = context.worker.as_ref().cloned();
            let chunk_plugin_id = plugin_id.to_string();
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
                RESOURCE_CHUNK,
                Box::new(move |ctx| {
                    ctx.update_progress("chunk", 5, "准备文档切分", None);
                    let parsed = if let Some(document) = document {
                        document
                    } else {
                        let path = path.ok_or_else(|| "缺少文档路径".to_string())?;
                        let parsed = parse_document_with_cache(
                            &_document_worker,
                            &path,
                            &chunk_cfg,
                            chunk_manager.as_deref(),
                            ctx,
                            &chunk_plugin_id,
                        )?;
                        parsed["document"].clone()
                    };
                    let mut result =
                        crate::document_chunker::chunk_document(&parsed, options.as_ref())?;
                    if result["count"].as_u64() == Some(0) {
                        return Err(
                            "未提取到可分块文本；请确认 PDF 包含文本层或 OCR 已识别内容".into()
                        );
                    }
                    if !output_directory.trim().is_empty() {
                        let directory = PathBuf::from(&output_directory);
                        std::fs::create_dir_all(&directory)
                            .map_err(|error| format!("创建切分输出目录失败: {error}"))
                            .and_then(|_| {
                                let manifest_path = directory
                                    .join(format!("document-chunks-{}.json", ctx.task_id()));
                                let jsonl_path = directory
                                    .join(format!("document-chunks-{}.jsonl", ctx.task_id()));
                                let bytes = serde_json::to_vec_pretty(&result)
                                    .map_err(|error| format!("序列化切分结果失败: {error}"))?;
                                std::fs::write(&manifest_path, bytes)
                                    .map_err(|error| format!("写入切分清单失败: {error}"))?;
                                let mut jsonl = String::new();
                                if let Some(chunks) = result["chunks"].as_array() {
                                    for chunk in chunks {
                                        let line =
                                            serde_json::to_string(chunk).map_err(|error| {
                                                format!("序列化 Chunk 失败: {error}")
                                            })?;
                                        jsonl.push_str(&line);
                                        jsonl.push('\n');
                                    }
                                }
                                std::fs::write(&jsonl_path, jsonl.as_bytes())
                                    .map_err(|error| format!("写入 Chunk JSONL 失败: {error}"))?;
                                result["outputPath"] = json!(jsonl_path.to_string_lossy());
                                result["manifestPath"] = json!(manifest_path.to_string_lossy());
                                result["outputFormat"] = json!("jsonl");
                                Ok::<(), String>(())
                            })
                            .unwrap_or_else(|error| {
                                // A read-only or unavailable output directory must not discard
                                // an otherwise successful in-memory chunking task.  Keep the
                                // chunks in the task/cache response and expose the reason so the
                                // renderer can offer another directory.
                                result["outputError"] = json!(error);
                            });
                    }
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
            remember_retry(context, &task_id, request);
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
            let output_directory = match str_field(request, "outputDirectory", 32 * 1024) {
                Ok(value) => value.map(ToOwned::to_owned),
                Err(error) => return error,
            };
            let convert_cfg = load_config_for_service(context, db, plugin_id);
            let convert_output_directory =
                output_directory.unwrap_or_else(|| convert_cfg.output_directory.clone());
            let convert_manager = context.worker.as_ref().cloned();
            let convert_plugin_id = plugin_id.to_string();
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
                RESOURCE_CONVERT,
                Box::new(move |ctx| {
                    ctx.update_progress("convert", 5, "解析源文档（可复用页面缓存）", None);
                    let parsed = parse_document_with_cache(
                        &_document_worker,
                        &path,
                        &convert_cfg,
                        convert_manager.as_deref(),
                        ctx,
                        &convert_plugin_id,
                    )?;
                    let resolved_output_path = match output_path.as_deref() {
                        Some(path) => PathBuf::from(path),
                        None => crate::document_converter::output_path_in_directory(
                            &path,
                            &target,
                            &convert_output_directory,
                        )?,
                    };
                    let resolved_output_path = resolved_output_path.to_string_lossy().into_owned();
                    ctx.check_cancelled()?;
                    crate::document_converter::convert_document_with_publication(
                        &_document_worker,
                        &parsed["document"],
                        &target,
                        Some(&resolved_output_path),
                        Some(&convert_cfg.cache_directory),
                        Some((ctx.runtime_context(), true)),
                    )
                }),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(context, &task_id, request);
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
            let batch_cfg = load_config_for_service(context, db, plugin_id);
            let batch_manager = context.worker.as_ref().cloned();
            let batch_plugin_id = plugin_id.to_string();
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
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
                                        parse_document_with_cache(&_document_worker,
                                            path,
                                            &batch_cfg,
                                            Some(manager),
                                            ctx,
                                            &batch_plugin_id,
                                        )
                                    } else {
                                        Err("批量 OCR 仅支持图片和 PDF".into())
                                    }
                                })
                            }
                            "parse" => {
                                let parsed = parse_document_with_cache(&_document_worker,
                                    path,
                                    &batch_cfg,
                                    batch_manager.as_deref(),
                                    ctx,
                                    &batch_plugin_id,
                                )?;
                                export_parsed_result(&_document_worker,
                                    parsed,
                                    path,
                                    Path::new(&batch_cfg.output_directory),
                                    ctx,
                                )
                            }
                            _ => {
                                let parsed = parse_document_with_cache(&_document_worker,
                                    path,
                                    &batch_cfg,
                                    batch_manager.as_deref(),
                                    ctx,
                                    &batch_plugin_id,
                                )?;
                                crate::document_converter::convert_document_with_publication(&_document_worker,
                                    &parsed["document"],
                                    &target,
                                    None,
                                    Some(&batch_cfg.cache_directory),
                                    Some((ctx.runtime_context(), false)),
                                )
                            }
                        };
                        match result {
                            Ok(result) => {
                                succeeded += 1;
                                items.push(json!({ "path": path, "result": result }));
                            }
                            Err(error) => {
                                if ctx.runtime_context().publication_pending() {
                                    return Err(error);
                                }
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
            remember_retry(context, &task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        // ---- Phase 9：任务暂停/恢复 ----
        "document.jobs.pause" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id,
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(error) => return error,
            };
            if context.tasks.pause(task_id) {
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
            if context.tasks.resume(task_id) {
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
            let Some(snapshot) = context.tasks.get(&task_id) else {
                return err("task-not-found", "未找到指定任务".into());
            };
            if !matches!(
                snapshot["status"].as_str(),
                Some("failed") | Some("cancelled")
            ) {
                return err("task-not-retryable", "只有失败或已取消任务可以重试".into());
            }
            let original = context
                .retry_requests
                .lock()
                .ok()
                .and_then(|requests| requests.get(&task_id).cloned());
            match original {
                Some(original) => handle_message(context, db, plugin_id, &original),
                None => err("retry-unavailable", "任务请求已过期，无法重试".into()),
            }
        }
        "document.models.catalog" => {
            json!({ "catalog": model_catalog() })
        }
        "document.models.importFormulaAddon" => {
            let archive = match str_field(request, "sourcePath", 32 * 1024) {
                Ok(Some(path)) => PathBuf::from(path),
                Ok(None) => return err("invalid-value", "请选择高精度公式附加包".into()),
                Err(error) => return error,
            };
            let cfg = load_config_for_service(context, db, plugin_id);
            let _document_worker = context.document_client();
            let task_id = match context.tasks.start(
                RESOURCE_MODELS,
                Box::new(move |ctx| install_high_formula_addon(&cfg, &archive, ctx)),
            ) {
                Ok(task_id) => task_id,
                Err(message) => return err("task-busy", message),
            };
            remember_retry(context, &task_id, request);
            json!({ "taskId": task_id, "status": "queued" })
        }
        "document.models.installBundle" => {
            let model_id = match str_field(request, "modelId", 128) {
                Ok(Some(id)) => id,
                Ok(None) => return err("invalid-value", "missing field: modelId".into()),
                Err(error) => return error,
            };
            let cfg = load_config_for_service(context, db, plugin_id);
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
            let cfg = load_config_for_service(context, db, plugin_id);
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
            let cfg = load_config_for_service(context, db, plugin_id);
            if request.get("url").and_then(Value::as_str).is_some() {
                return install_remote_model(&cfg, request, false);
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
            let cfg = load_config_for_service(context, db, plugin_id);
            install_remote_model(&cfg, request, true)
        }
        "document.models.remove" => {
            let relative = match str_field(request, "path", 4096) {
                Ok(Some(path)) => path.to_string(),
                Ok(None) => return err("invalid-value", "missing field: path".into()),
                Err(error) => return error,
            };
            let cfg = load_config_for_service(context, db, plugin_id);
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
            let cfg = load_config_for_service(context, db, plugin_id);
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

fn install_remote_model(cfg: &DocumentEngineConfig, request: &Value, update: bool) -> Value {
    let url = match str_field(request, "url", 32 * 1024) {
        Ok(Some(url)) => url,
        Ok(None) => return err("invalid-value", "missing field: url".into()),
        Err(error) => return error,
    };
    let name = match str_field(request, "name", 256) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => url.rsplit('/').next().unwrap_or("model").to_string(),
        Err(error) => return error,
    };
    match crate::document_engine_cache::install_remote(
        PathBuf::from(&cfg.model_directory).as_path(),
        url,
        &name,
        "",
        update,
    ) {
        Ok(target) => json!({
            "success": true,
            "path": target.to_string_lossy(),
            "name": name,
            "source": "remote",
            "updated": update
        }),
        Err(message) => err(
            if update {
                "model-update-failed"
            } else {
                "model-install-failed"
            },
            message,
        ),
    }
}

/// Read the short-lived configuration under the host DB mutex, then let the
/// caller perform a remote model download after releasing that mutex.
pub(crate) fn load_config_for_host(
    context: &Service,
    db: &Db,
    plugin_id: &str,
) -> DocumentEngineConfig {
    load_config_for_service(context, db, plugin_id)
}

pub(crate) fn dispatch_remote_model(
    config: &DocumentEngineConfig,
    payload: &Value,
) -> Result<Value, String> {
    let request = payload
        .as_object()
        .map(|_| payload)
        .ok_or_else(|| "message payload must be an object".to_string())?;
    let msg_type = request.get("type").and_then(Value::as_str).unwrap_or("");
    match msg_type {
        "document.models.install" => Ok(install_remote_model(config, request, false)),
        "document.models.update" => Ok(install_remote_model(config, request, true)),
        _ => Err(format!("unsupported remote model operation: {msg_type}")),
    }
}

/// 统一入口：envelope_host::host_dispatch 分发 service="document-engine" 时调用。
/// params: { operation, payload? }。
pub fn dispatch(
    context: &Service,
    db: &Db,
    plugin_id: &str,
    operation: &str,
    payload: Option<&Value>,
) -> Result<Value, String> {
    if operation == "activate" {
        // 初始化任务表；Worker 仍按需启动，避免仅激活插件就加载模型。
        // 内置模型只做本地校验和复制，不在激活阶段主动阻塞网络下载。
        // 没有内置资源时由模型页的 installBundle 触发官方直连下载。
        let cfg = load_config_for_service(context, db, plugin_id);
        let _ = ensure_default_model(db, plugin_id, Path::new(&cfg.model_directory));
        return Ok(Value::Null);
    }
    if operation == "deactivate" {
        context.tasks.cancel_all_active();
        if let Ok(mut requests) = context.retry_requests.lock() {
            requests.clear();
        }
        if let Some(manager) = context.worker.as_ref() {
            manager.shutdown();
        }
        return Ok(Value::Null);
    }
    if operation != "message" {
        return Err(format!("unknown trusted operation: {operation}"));
    }
    let payload = payload.ok_or_else(|| "message operation requires payload".to_string())?;
    Ok(handle_message(context, db, plugin_id, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ocr_mode_profiles_keep_fast_text_only_and_unimernet_explicit() {
        let fast = DocumentEngineConfig {
            model_profile: "fast".into(),
            ..Default::default()
        };
        assert_eq!(effective_formula_profile(&fast), Some(FAST_MODEL_PROFILE));
        let standard_candidate = DocumentEngineConfig {
            model_profile: "standard".into(),
            ..Default::default()
        };
        assert_eq!(
            effective_formula_profile(&standard_candidate),
            Some("pp-doclayout-m-unimernet")
        );
    }

    #[test]
    fn ocr_preview_returns_bounded_thumbnail_with_source_dimensions() {
        let fixture = TempDb::new("ocr-preview");
        let source = image::RgbImage::from_pixel(1600, 800, image::Rgb([242, 248, 255]));
        for extension in ["png", "jpg"] {
            let path = fixture.dir.join(format!("source.{extension}"));
            source.save(&path).unwrap();
            let result = handle_message(
                &fixture.service,
                &fixture.db,
                "document-engine",
                &json!({"type": "document.ocr.preview", "path": path}),
            );
            assert_eq!(result["width"], 1600);
            assert_eq!(result["height"], 800);
            let encoded = result["dataUrl"]
                .as_str()
                .unwrap()
                .strip_prefix("data:image/jpeg;base64,")
                .unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap();
            assert!(bytes.len() <= 160 * 1024);
            assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 720);
        }
    }

    #[test]
    fn high_precision_status_detects_selected_addon_directory() {
        let fixture = TempDb::new("formula-addon-status");
        let addon = fixture.dir.join("high");
        let models = addon.join("models");
        let cfg = DocumentEngineConfig {
            model_directory: fixture.dir.join("default").to_string_lossy().into_owned(),
            formula_addon_directory: addon.to_string_lossy().into_owned(),
            model_profile: "pp-doclayout-m-formulanet-plus-l".into(),
            ..Default::default()
        };
        assert!(!high_precision_runtime_ready(&cfg));
        std::fs::create_dir_all(models.join("paddle-formula/PP-DocLayout-M")).unwrap();
        std::fs::create_dir_all(models.join("paddle-formula/PP-FormulaNet_plus-L")).unwrap();
        std::fs::write(
            models.join("paddle-formula/PP-DocLayout-M/inference.pdiparams"),
            [],
        )
        .unwrap();
        std::fs::write(
            models.join("paddle-formula/PP-FormulaNet_plus-L/inference.pdiparams"),
            [],
        )
        .unwrap();
        std::fs::create_dir_all(addon.join("python")).unwrap();
        std::fs::write(addon.join("python/python.exe"), []).unwrap();
        std::fs::write(addon.join("formula-ocr-worker.py"), []).unwrap();
        assert!(high_precision_runtime_ready(&cfg));
        let managed = DocumentEngineConfig {
            model_directory: fixture.dir.to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert!(high_precision_runtime_ready(&managed));
    }

    #[test]
    #[ignore = "requires DOCUMENT_ENGINE_ADDON_ARCHIVE and DOCUMENT_ENGINE_ADDON_IMPORT_ROOT"]
    fn imports_pinned_formula_addon_through_host_task() {
        let archive = std::env::var("DOCUMENT_ENGINE_ADDON_ARCHIVE").unwrap();
        let root = PathBuf::from(std::env::var("DOCUMENT_ENGINE_ADDON_IMPORT_ROOT").unwrap());
        assert!(!root.exists(), "use a fresh model root for addon import");
        let fixture = TempDb::new("formula-addon-import");
        {
            let conn = fixture.db.conn().lock().unwrap();
            conn.execute(
                "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path, permissions, config_data)
                 VALUES ('document-engine', 'document-engine', '0.1.0', 'Document Engine', 'dist/main.js', '.', '[]', ?1)",
                [json!({ "modelDirectory": root.to_string_lossy() }).to_string()],
            )
            .unwrap();
        }
        let accepted = handle_message(
            &fixture.service,
            &fixture.db,
            "document-engine",
            &json!({ "type": "document.models.importFormulaAddon", "sourcePath": archive }),
        );
        let task_id = accepted["taskId"].as_str().expect("addon task ID");
        let mut snapshot = Value::Null;
        for _ in 0..1800 {
            snapshot = handle_message(
                &fixture.service,
                &fixture.db,
                "document-engine",
                &json!({ "type": "document.jobs.get", "taskId": task_id }),
            );
            if matches!(
                snapshot["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert_eq!(snapshot["status"], "succeeded", "{snapshot}");
        let cfg = load_config(&fixture.db, "document-engine");
        assert!(high_precision_runtime_ready(&cfg));
        assert!(root.join("high/python/python.exe").is_file());
    }

    #[test]
    fn formula_addon_import_rejects_unpinned_archive() {
        let fixture = TempDb::new("formula-addon-reject");
        let archive = fixture.dir.join("tampered.7z");
        std::fs::write(&archive, b"not the trusted addon").unwrap();
        {
            let conn = fixture.db.conn().lock().unwrap();
            conn.execute(
                "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path, permissions, config_data)
                 VALUES ('document-engine', 'document-engine', '0.1.0', 'Document Engine', 'dist/main.js', '.', '[]', ?1)",
                [json!({ "modelDirectory": fixture.dir.join("models") }).to_string()],
            )
            .unwrap();
        }
        let accepted = handle_message(
            &fixture.service,
            &fixture.db,
            "document-engine",
            &json!({ "type": "document.models.importFormulaAddon", "sourcePath": archive }),
        );
        let task_id = accepted["taskId"].as_str().unwrap();
        let mut snapshot = Value::Null;
        for _ in 0..100 {
            snapshot = handle_message(
                &fixture.service,
                &fixture.db,
                "document-engine",
                &json!({ "type": "document.jobs.get", "taskId": task_id }),
            );
            if snapshot["status"] == "failed" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(snapshot["status"], "failed");
        assert!(snapshot["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("SHA-256"));
        assert!(!fixture.dir.join("models/high").exists());
    }

    #[test]
    #[ignore = "requires separately packaged DOCUMENT_RUNTIME_ACCEPTANCE_DIRECTORY"]
    fn pinned_document_runtime_installs_and_runs_without_ocr_resources() {
        let root = tempfile::tempdir().unwrap();
        let service = Service::with_worker(
            crate::task_runtime::TaskRuntime::memory(),
            None,
            None,
            Some(root.path().join("runtime")),
        );
        let source =
            std::env::var_os("DOCUMENT_RUNTIME_ACCEPTANCE_DIRECTORY").expect("packaged runtime");
        let result = service
            .install_document_runtime(Path::new(&source))
            .unwrap();
        assert_eq!(result["installed"], true);
        let status = crate::pdf_parser::renderer_status(&service.document_client());
        assert_eq!(status["available"], true, "{status}");
        let source_file = root.path().join("source.pdf");
        let pdf = b"%PDF-1.4\n1 0 obj\n<</Type /Page /MediaBox [0 0 200 100] /Contents 2 0 R>>\nendobj\n2 0 obj\n<</Length 23>>\nstream\nBT (Hi) Tj ET\nendstream\nendobj\n%%EOF";
        std::fs::write(&source_file, pdf).unwrap();
        let parsed = service
            .document_client()
            .run(
                &cruciblebox_document_worker::protocol::Operation::Parse {
                    path: source_file.to_string_lossy().into(),
                },
                &|| false,
            )
            .unwrap();
        assert_eq!(parsed.value["route"], "native");
        assert_eq!(parsed.value["document"]["metadata"]["pageCount"], 1);
        assert_eq!(
            parsed.value["document"]["pages"][0]["blocks"][0]["content"],
            "Hi"
        );
        assert!(service.worker.is_none());
        drop(service);
        let restarted = Service::with_worker(
            crate::task_runtime::TaskRuntime::memory(),
            None,
            None,
            Some(root.path().join("runtime")),
        );
        assert_eq!(
            crate::pdf_parser::renderer_status(&restarted.document_client())["available"],
            true
        );
    }

    struct TempDb {
        dir: PathBuf,
        db: Db,
        service: Service,
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
            TempDb {
                dir,
                db,
                service: Service::new(crate::task_runtime::TaskRuntime::memory()),
            }
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn formula_insertion_preserves_worker_text_sequence() {
        let mut blocks = vec![
            json!({ "type": "text", "content": "first", "bbox": [10, 200, 60, 220] }),
            json!({ "type": "text", "content": "second", "bbox": [10, 100, 70, 120] }),
            json!({ "type": "formula", "content": "x^2", "bbox": [10, 150, 70, 170] }),
        ];
        insert_formulas_into_ocr_order(&mut blocks);
        let text = blocks
            .iter()
            .filter(|block| block["type"] == "text")
            .map(|block| block["content"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(text, ["first", "second"]);
        assert_eq!(blocks[0]["type"], "formula");
    }

    #[test]
    fn get_status_returns_object() {
        let t = TempDb::new("status");
        let out = dispatch(
            &t.service,
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
            &t.service,
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
            &t.service,
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "nope" })),
        )
        .unwrap();
        assert_eq!(out["code"], "unknown-type");
    }

    #[test]
    fn rebuild_structure_keeps_heading_parent_ids_non_self_referential() {
        let mut document = json!({
            "metadata": { "pageCount": 1 },
            "pages": [{ "number": 1, "height": 800, "blocks": [
                { "id": "h1", "type": "heading", "content": "Chapter 1", "bbox": [10, 10, 100, 30] },
                { "id": "p1", "type": "paragraph", "content": "Body", "bbox": [10, 40, 200, 60] },
                { "id": "h2", "type": "heading", "content": "Section 1.1", "bbox": [10, 70, 120, 90] }
            ]}],
            "structure": {}
        });
        rebuild_document_structure(&mut document);
        let blocks = document["pages"][0]["blocks"].as_array().unwrap();
        assert_eq!(blocks[0]["parentId"], Value::Null);
        assert_eq!(blocks[1]["parentId"], "h1");
        assert_eq!(blocks[2]["parentId"], "h1");
        assert_eq!(document["structure"]["outline"][0]["parentId"], Value::Null);
        assert_eq!(
            document["structure"]["outline"][0]["children"][0]["parentId"],
            "h1"
        );
    }

    #[test]
    fn semantic_metadata_is_recomputed_from_structured_blocks() {
        let mut document = json!({
            "metadata":{"hasImages":true,"hasFormulas":false,"hasTables":false},
            "pages":[{"blocks":[
                {"type":"formula"},
                {"type":"matrix"},
                {"type":"figure"},
                {"type":"table"}
            ]}]
        });
        refresh_semantic_metadata(&mut document);
        assert_eq!(document["metadata"]["hasFormulas"], true);
        assert_eq!(document["metadata"]["formulaBlockCount"], 2);
        assert_eq!(document["metadata"]["matrixBlockCount"], 1);
        assert_eq!(document["metadata"]["hasImages"], true);
        assert_eq!(document["metadata"]["hasTables"], true);
    }

    #[test]
    fn ocr_requires_configured_worker() {
        let t = TempDb::new("notimpl");
        let out = dispatch(
            &t.service,
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
            &t.service,
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
    #[ignore = "requires the separately packaged document worker runtime"]
    fn parse_pdf_task_returns_unified_document() {
        let t = TempDb::new("parse");
        let dir = std::env::temp_dir().join(format!("cb-de-parse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = b"%PDF-1.4\n1 0 obj\n<</Type /Page /MediaBox [0 0 200 100] /Contents 2 0 R>>\nendobj\n2 0 obj\n<</Length 23>>\nstream\nBT (Hi) Tj ET\nendstream\nendobj\n%%EOF";
        let path = dir.join("doc.pdf");
        std::fs::write(&path, pdf).unwrap();
        let accepted = dispatch(
            &t.service,
            &t.db,
            "document-engine",
            "message",
            Some(&json!({
                "type": "document.parse",
                "path": path.to_string_lossy().into_owned(),
                "options": { "outputDirectory": dir.to_string_lossy().into_owned() }
            })),
        )
        .unwrap();
        let task_id = accepted["taskId"].as_str().unwrap().to_string();
        let mut snapshot = Value::Null;
        for _ in 0..500 {
            snapshot = dispatch(
                &t.service,
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
    fn model_formula_keeps_original_latex_and_engine_for_review() {
        let mut document = json!({
            "pages": [{"number": 1, "blocks": [{
                "type": "formula",
                "source": "ocr/formula-model",
                "content": "\\begin{aligned}x&=1\\\\y&=2\\end{aligned}",
                "rawLatex": "\\begin{aligned}x&=1\\\\y&=2\\end{aligned}",
                "normalizedLatex": "\\begin{aligned}x&=1\\\\y&=2\\end{aligned}",
                "formulaEngine": "PP-FormulaNet_plus-S",
                "requiresReview": true
            }]}]
        });
        enrich_formula_blocks(&mut document);
        let block = &document["pages"][0]["blocks"][0];
        assert_eq!(
            block["content"],
            "\\begin{aligned}x&=1\\\\y&=2\\end{aligned}"
        );
        assert_eq!(block["formulaEngine"], "PP-FormulaNet_plus-S");
        assert_eq!(block["math"]["quality"], "needs-review");
    }

    #[test]
    #[ignore = "requires DOCUMENT_ENGINE_SCAN_FIXTURE_PDF and local OCR models"]
    fn parses_scanned_pdf_through_full_ocr_pipeline() {
        let context = Service::new(crate::task_runtime::TaskRuntime::memory());
        let path = std::env::var("DOCUMENT_ENGINE_SCAN_FIXTURE_PDF").unwrap_or_else(|_| {
            "C:\\Users\\hjc\\Desktop\\fogharbor_botanical_field_notes_scanned.pdf".into()
        });
        let worker_path = std::env::var("OCR_WORKER_EXE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from("E:\\CrucibleBox_Sourses\\ocr-worker\\target\\debug\\ocr-worker.exe")
            });
        let root = std::env::temp_dir().join(format!(
            "cb-de-scan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let manager = Arc::new(OcrWorkerManager::new(
            worker_path,
            std::time::Duration::from_secs(180),
        ));
        let config = DocumentEngineConfig {
            model_directory: std::env::var("DOCUMENT_ENGINE_MODEL_DIRECTORY")
                .unwrap_or_else(|_| "E:\\OCR\\Models".into()),
            formula_addon_directory: std::env::var("DOCUMENT_ENGINE_FORMULA_ADDON_DIRECTORY")
                .unwrap_or_default(),
            dictionary_path: std::env::var("DOCUMENT_ENGINE_DICTIONARY_PATH").unwrap_or_default(),
            model_profile: std::env::var("DOCUMENT_ENGINE_MODEL_PROFILE")
                .unwrap_or_else(|_| "auto".into()),
            text_recognition_mode: std::env::var("DOCUMENT_ENGINE_TEXT_RECOGNITION_MODE")
                .unwrap_or_else(|_| "mixed".into()),
            cache_directory: root.join("cache").to_string_lossy().into_owned(),
            output_directory: root.join("output").to_string_lossy().into_owned(),
            device: "cpu".into(),
            resources: None,
        };
        let _document_worker = context.document_client();
        let task_id = context
            .tasks
            .start(
                RESOURCE_PARSE,
                Box::new(move |ctx| {
                    parse_document_with_cache(
                        &_document_worker,
                        &path,
                        &config,
                        Some(&manager),
                        ctx,
                        "document-engine",
                    )
                }),
            )
            .unwrap();
        let mut snapshot = Value::Null;
        for _ in 0..720 {
            snapshot = context.tasks.get(&task_id).unwrap();
            if matches!(
                snapshot["status"].as_str(),
                Some("succeeded") | Some("failed")
            ) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert_eq!(snapshot["status"], "succeeded", "{snapshot}");
        assert!(matches!(
            snapshot["result"]["route"].as_str(),
            Some("ocr") | Some("mixed")
        ));
        assert!(
            snapshot["result"]["document"]["metadata"]["pageCount"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
        assert!(snapshot["result"]["document"]["metadata"]["quality"]["invalidControlChars"] == 0);
        // TaskManager intentionally returns a bounded preview for large
        // documents. Regression assertions must use the authoritative cache
        // result, otherwise a multi-page scan can be mistaken for a short
        // document when the preview budget is reached.
        let full_result = std::fs::read_dir(root.join("cache"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| std::fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .find_map(|entry| {
                entry["result"]["document"]
                    .is_object()
                    .then(|| entry["result"].clone())
            })
            .expect("full document result should be available in the cache");
        let document = &full_result["document"];
        let quality = &document["metadata"]["quality"];
        let pages = document["pages"].as_array().cloned().unwrap_or_default();
        let blocks = pages
            .iter()
            .flat_map(|page| page["blocks"].as_array().cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        let all_text = blocks
            .iter()
            .filter_map(|block| block["content"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = crate::document_chunker::chunk_document(document, None).unwrap();
        eprintln!(
            "fogharbor regression: pages={} blocks={} quality={} chunks={}",
            pages.len(),
            blocks.len(),
            json!({
                "headingCount": quality["headingCount"],
                "formulaBlockCount": quality["formulaBlockCount"],
                "nativeTextBlockCount": quality["nativeTextBlockCount"],
                "ocrTextBlockCount": quality["ocrTextBlockCount"],
                "ragQuality": quality["ragQuality"],
                "qualityFlags": quality["qualityFlags"]
            }),
            chunks["count"]
        );
        assert_eq!(quality["nativeTextBlockCount"], 0);
        assert!(quality["ocrTextBlockCount"].as_u64().unwrap_or(0) > 0);
        assert!(quality["headingCount"].as_u64().unwrap_or(0) > 0);
        assert!(chunks["count"].as_u64().unwrap_or(0) > 1);
        for forbidden_formula_text in [
            "FIELD ARCHIVE / FOGHARBOR",
            "SCANNED FIELD-EDITION",
            "TIDE / WIND / MEMORY",
            "03/10",
            "08:05",
            "09:40",
        ] {
            assert!(
                !blocks.iter().any(|block| {
                    block["type"] == "formula"
                        && block["content"]
                            .as_str()
                            .is_some_and(|content| content.contains(forbidden_formula_text))
                }),
                "forbidden formula candidate accepted: {forbidden_formula_text}"
            );
        }
        for expected_text in [
            "潮汐灯笼花",
            "玻璃苔",
            "月盐藤",
            "雨声蕨",
            "灯塔果",
            "纸鸢藻",
        ] {
            assert!(
                all_text.contains(expected_text),
                "expected OCR text missing: {expected_text}"
            );
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "requires DOCUMENT_ENGINE_GT_PDF, DOCUMENT_ENGINE_GT_OUTPUT and local OCR models"]
    fn exports_annotated_pdf_ocr_for_accuracy_scoring() {
        let context = Service::new(crate::task_runtime::TaskRuntime::memory());
        let path = std::env::var("DOCUMENT_ENGINE_GT_PDF").expect("annotated PDF path is required");
        let output =
            std::env::var("DOCUMENT_ENGINE_GT_OUTPUT").expect("OCR output path is required");
        let worker_path = std::env::var("OCR_WORKER_EXE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(
                    "E:\\CrucibleBox_Sourses\\ocr-worker\\target\\release\\ocr-worker.exe",
                )
            });
        let root = std::env::temp_dir().join(format!(
            "cb-de-ground-truth-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let manager = Arc::new(OcrWorkerManager::new(
            worker_path,
            std::time::Duration::from_secs(180),
        ));
        let config = DocumentEngineConfig {
            model_directory: std::env::var("DOCUMENT_ENGINE_MODEL_DIRECTORY")
                .unwrap_or_else(|_| "E:\\OCR\\Models".into()),
            formula_addon_directory: std::env::var("DOCUMENT_ENGINE_FORMULA_ADDON_DIRECTORY")
                .unwrap_or_default(),
            dictionary_path: std::env::var("DOCUMENT_ENGINE_DICTIONARY_PATH").unwrap_or_default(),
            model_profile: std::env::var("DOCUMENT_ENGINE_MODEL_PROFILE")
                .unwrap_or_else(|_| "auto".into()),
            text_recognition_mode: std::env::var("DOCUMENT_ENGINE_TEXT_RECOGNITION_MODE")
                .unwrap_or_else(|_| "mixed".into()),
            cache_directory: root.join("cache").to_string_lossy().into_owned(),
            output_directory: root.join("output").to_string_lossy().into_owned(),
            device: "cpu".into(),
            resources: None,
        };
        let _document_worker = context.document_client();
        let task_id = context
            .tasks
            .start(
                RESOURCE_PARSE,
                Box::new(move |ctx| {
                    parse_document_with_cache(
                        &_document_worker,
                        &path,
                        &config,
                        Some(&manager),
                        ctx,
                        "document-engine",
                    )
                }),
            )
            .unwrap();
        let mut snapshot = Value::Null;
        for _ in 0..2400 {
            snapshot = context.tasks.get(&task_id).unwrap();
            if matches!(
                snapshot["status"].as_str(),
                Some("succeeded") | Some("failed")
            ) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert_eq!(snapshot["status"], "succeeded", "{snapshot}");
        let full_result = std::fs::read_dir(root.join("cache"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| std::fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .find_map(|entry| {
                entry["result"]["document"]
                    .is_object()
                    .then(|| entry["result"].clone())
            })
            .expect("full OCR result should be cached");
        assert_eq!(full_result["route"], "ocr");
        assert_eq!(
            full_result["document"]["metadata"]["quality"]["nativeTextBlockCount"],
            0
        );
        if let Ok(ir_output) = std::env::var("DOCUMENT_ENGINE_GT_IR_OUTPUT") {
            std::fs::write(ir_output, serde_json::to_vec_pretty(&full_result).unwrap()).unwrap();
        }
        let text = full_result["document"]["pages"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|page| page["blocks"].as_array().into_iter().flatten())
            .filter_map(|block| block["content"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.trim().is_empty());
        std::fs::write(output, text).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parse_rejects_non_pdf_extension() {
        let t = TempDb::new("parse-format");
        let out = dispatch(
            &t.service,
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
            &t.service,
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "document.chunk", "path": path.to_string_lossy() })),
        )
        .unwrap();
        let task_id = accepted["taskId"].as_str().unwrap().to_string();
        let mut snapshot = Value::Null;
        for _ in 0..500 {
            snapshot = dispatch(
                &t.service,
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
    #[ignore = "requires the separately packaged document worker runtime"]
    fn convert_task_writes_markdown_output() {
        let t = TempDb::new("convert");
        let dir = std::env::temp_dir().join(format!("cb-de-convert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");
        let output = dir.join("note.md");
        std::fs::write(&path, b"hello").unwrap();
        let accepted = dispatch(
            &t.service,
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
        for _ in 0..500 {
            snapshot = dispatch(
                &t.service,
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
            &t.service,
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
        assert_eq!(entry["id"], DEFAULT_MODEL_ID);
        assert_eq!(entry["recommended"], true);
        assert_eq!(entry["default"], true);
        assert_eq!(entry["offline"], true);
        assert_eq!(entry["artifacts"].as_array().map(Vec::len), Some(3));
        assert_eq!(entry["totalBytes"].as_u64(), Some(26_634_912));
        for artifact in entry["artifacts"].as_array().unwrap() {
            assert_eq!(artifact["sha256"].as_str().map(str::len), Some(64));
            assert!(artifact["url"].as_str().unwrap().starts_with("https://"));
            assert!(artifact["sources"]
                .as_array()
                .is_some_and(|sources| sources.len() == 1));
        }

        let t = TempDb::new("model-catalog");
        let response = dispatch(
            &t.service,
            &t.db,
            "document-engine",
            "message",
            Some(&json!({ "type": "document.models.catalog" })),
        )
        .unwrap();
        assert_eq!(response["catalog"][0]["id"], DEFAULT_MODEL_ID);
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
            &t.service,
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
        assert!(dispatch(&t.service, &t.db, "document-engine", "other", None).is_err());
    }
}

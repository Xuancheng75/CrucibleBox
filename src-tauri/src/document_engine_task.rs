//! Compatibility projection over the shared durable task authority.
use crate::task_runtime::{Context, TaskRuntime};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
type TaskObserver = Arc<dyn Fn(Value) + Send + Sync>;
pub type ProgressEmitter = Arc<dyn Fn(&str, Value) + Send + Sync>;
pub struct TaskContext {
    inner: Context,
    progress_emitter: Option<ProgressEmitter>,
}
impl TaskContext {
    pub fn runtime_context(&self) -> &Context {
        &self.inner
    }
    pub fn emit_progress(&self, plugin_id: &str, progress: &Value) {
        if let Some(emitter) = &self.progress_emitter {
            emitter(
                "plugin:message",
                json!({"pluginId":plugin_id,"message":{"type":"document.progress","taskId":self.task_id(),"progress":progress}}),
            );
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
    pub fn cancel_flag(&self) -> &AtomicBool {
        self.inner.cancel_flag()
    }
    pub fn check_cancelled(&self) -> Result<(), String> {
        self.inner.check_cancelled()
    }
    pub fn task_id(&self) -> String {
        self.inner.task_id()
    }
    pub fn wait_if_paused(&self) -> Result<(), String> {
        self.inner.wait_if_paused()
    }
    pub fn progress_snapshot(&self) -> Value {
        self.inner.progress_snapshot()
    }
    pub fn update_progress(&self, stage: &str, percent: u32, message: &str, extra: Option<Value>) {
        self.inner.update_progress(stage, percent, message, extra);
    }
}
pub type TaskExecutor = Box<dyn FnOnce(&TaskContext) -> Result<Value, String> + Send>;
pub struct TaskManager {
    runtime: Mutex<Arc<TaskRuntime>>,
    progress_emitter: Mutex<Option<ProgressEmitter>>,
}
#[cfg(test)]
impl Default for TaskManager {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(TaskRuntime::memory()),
            progress_emitter: Mutex::new(None),
        }
    }
}
impl TaskManager {
    pub fn with_runtime(runtime: Arc<TaskRuntime>) -> Self {
        Self {
            runtime: Mutex::new(runtime),
            progress_emitter: Mutex::new(None),
        }
    }
    fn runtime(&self) -> Arc<TaskRuntime> {
        self.runtime.lock().unwrap().clone()
    }
    pub fn set_progress_emitter(&self, emitter: ProgressEmitter) {
        *self.progress_emitter.lock().unwrap() = Some(emitter);
    }
    pub fn set_observer(&self, observer: TaskObserver) {
        self.runtime().set_observer("document-engine", observer);
    }
    pub fn start(
        self: &Arc<Self>,
        resource: &str,
        executor: TaskExecutor,
    ) -> Result<String, String> {
        let progress_emitter = self.progress_emitter.lock().unwrap().clone();
        self.runtime().start(
            "document-engine",
            resource,
            Box::new(move |context| {
                let context = TaskContext {
                    inner: context.clone(),
                    progress_emitter,
                };
                executor(&context)
                    .map(|result| compact_snapshot(&json!({"result":result}))["result"].clone())
            }),
        )
    }
    pub fn cancel(&self, id: &str) -> bool {
        self.runtime().cancel("document-engine", id)
    }
    pub fn get(&self, id: &str) -> Option<Value> {
        self.runtime()
            .get("document-engine", id)
            .map(|v| compact_snapshot(&v))
    }
    pub fn active_task(&self, resource: &str) -> Option<String> {
        self.runtime().active("document-engine", resource)
    }
    pub fn cancel_all_active(&self) -> usize {
        self.runtime()
            .list("document-engine")
            .iter()
            .filter_map(|v| v["taskId"].as_str())
            .filter(|id| self.cancel(id))
            .count()
    }
    pub fn pause(&self, id: &str) -> bool {
        self.runtime().pause("document-engine", id)
    }
    pub fn resume(&self, id: &str) -> bool {
        self.runtime().resume("document-engine", id)
    }
    pub fn list(&self) -> Vec<Value> {
        self.runtime()
            .list("document-engine")
            .iter()
            .map(compact_snapshot)
            .collect()
    }
}
pub const RESOURCE_OCR: &str = "ocr";
pub const RESOURCE_PARSE: &str = "parse";
pub const RESOURCE_CHUNK: &str = "chunk";
pub const RESOURCE_SPLIT: &str = "split";
pub const RESOURCE_CONVERT: &str = "convert";
pub const RESOURCE_BATCH: &str = "batch";
pub const RESOURCE_MODELS: &str = "models";
const MAX_RESULT_PREVIEW_BYTES: usize = 192 * 1024;
const MAX_RESULT_PREVIEW_NODES: usize = 3072;
const MAX_RESULT_PREVIEW_DEPTH: usize = 12;

/// Return a bounded task envelope. The full result is written to the
/// Document Engine cache by each executor; transporting thousands of pages in
/// every 500 ms poll would otherwise trip the renderer JSON budget.
fn compact_snapshot(snapshot: &Value) -> Value {
    let encoded = snapshot
        .get("result")
        .and_then(|result| serde_json::to_vec(result).ok());
    let Some(encoded) = encoded else {
        return snapshot.clone();
    };
    let shape = JsonShape::measure(&snapshot["result"]);
    if encoded.len() <= MAX_RESULT_PREVIEW_BYTES
        && shape.nodes <= MAX_RESULT_PREVIEW_NODES
        && shape.max_depth <= MAX_RESULT_PREVIEW_DEPTH
    {
        return snapshot.clone();
    }

    let mut compact = snapshot.clone();
    let result = snapshot.get("result").cloned().unwrap_or(Value::Null);
    compact["resultBytes"] = json!(encoded.len());
    compact["resultTruncated"] = json!(true);
    compact["result"] = compact_result(&result, &mut CompactBudget::default());
    compact
}

#[derive(Debug, Default)]
struct CompactBudget {
    nodes: usize,
}

impl CompactBudget {
    fn take(&mut self) -> bool {
        if self.nodes >= MAX_RESULT_PREVIEW_NODES {
            return false;
        }
        self.nodes += 1;
        true
    }
}

#[derive(Debug, Default)]
struct JsonShape {
    nodes: usize,
    max_depth: usize,
}

impl JsonShape {
    fn measure(value: &Value) -> Self {
        fn visit(value: &Value, depth: usize, result: &mut JsonShape) {
            result.nodes = result.nodes.saturating_add(1);
            result.max_depth = result.max_depth.max(depth);
            match value {
                Value::Array(values) => values
                    .iter()
                    .for_each(|value| visit(value, depth.saturating_add(1), result)),
                Value::Object(values) => values
                    .values()
                    .for_each(|value| visit(value, depth.saturating_add(1), result)),
                _ => {}
            }
        }
        let mut result = Self::default();
        visit(value, 0, &mut result);
        result
    }
}

fn compact_result(result: &Value, budget: &mut CompactBudget) -> Value {
    if !budget.take() {
        return Value::Null;
    }
    match result {
        Value::Object(object) => {
            let mut out = serde_json::Map::new();
            for key in [
                "kind",
                "type",
                "route",
                "requiresOcr",
                "documentId",
                "strategy",
                "count",
                "target",
                "outputPath",
                "manifestPath",
                "outputFormat",
                "outputDirectory",
                "outputs",
                "sourcePath",
                "pageCount",
                "pagesPerFile",
                "fileCount",
                "files",
                "bytes",
                "message",
            ] {
                if let Some(value) = object.get(key) {
                    out.insert(key.to_string(), value.clone());
                }
            }
            if let Some(document) = object.get("document").and_then(Value::as_object) {
                let mut doc = serde_json::Map::new();
                for key in ["id", "source", "metadata", "structure"] {
                    if let Some(value) = document.get(key) {
                        doc.insert(key.to_string(), compact_nested(value, 16, budget));
                    }
                }
                if let Some(pages) = document.get("pages").and_then(Value::as_array) {
                    doc.insert(
                        "pages".into(),
                        Value::Array(
                            pages
                                .iter()
                                .take(3)
                                .map(|page| compact_nested(page, 16, budget))
                                .collect(),
                        ),
                    );
                    doc.insert("pagePreviewCount".into(), json!(pages.len().min(3)));
                    doc.insert("pageTotal".into(), json!(pages.len()));
                }
                out.insert("document".into(), Value::Object(doc));
            }
            for key in ["warnings", "ocrPageNumbers"] {
                if let Some(values) = object.get(key).and_then(Value::as_array) {
                    out.insert(
                        key.to_string(),
                        Value::Array(values.iter().take(32).cloned().collect()),
                    );
                }
            }
            for key in ["chunks", "items", "blocks"] {
                if let Some(values) = object.get(key).and_then(Value::as_array) {
                    out.insert(
                        key.to_string(),
                        Value::Array(
                            values
                                .iter()
                                .take(32)
                                .map(|v| compact_nested(v, 16, budget))
                                .collect(),
                        ),
                    );
                    out.insert(format!("{key}PreviewCount"), json!(values.len().min(32)));
                }
            }
            Value::Object(out)
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .take(32)
                .map(|v| compact_nested(v, 16, budget))
                .collect(),
        ),
        _ => result.clone(),
    }
}

fn compact_nested(value: &Value, max_chars: usize, budget: &mut CompactBudget) -> Value {
    if !budget.take() {
        return Value::Null;
    }
    match value {
        Value::String(text) if text.len() > max_chars => Value::String(format!(
            "{}…",
            text.chars().take(max_chars).collect::<String>()
        )),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .take(32)
                .map(|(key, value)| (key.clone(), compact_nested(value, max_chars, budget)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .take(32)
                .map(|value| compact_nested(value, max_chars, budget))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn executor_panic_records_failure_and_releases_resource() {
        let mgr = Arc::new(TaskManager::default());
        let id = mgr
            .start(
                "panic-resource",
                Box::new(|_| panic!("injected executor crash")),
            )
            .unwrap();
        for _ in 0..200 {
            if mgr.active_task("panic-resource").is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(mgr.get(&id).unwrap()["status"], "failed");
        assert!(mgr.get(&id).unwrap()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("异常退出"));
        assert!(mgr.active_task("panic-resource").is_none());
        mgr.start("panic-resource", Box::new(|_| Ok(Value::Null)))
            .unwrap();
    }

    use super::*;
    use std::time::Duration;

    fn manager() -> Arc<TaskManager> {
        Arc::new(TaskManager::default())
    }

    #[test]
    fn lifecycle_success_releases_resource() {
        let mgr = manager();
        let id = mgr
            .start(
                RESOURCE_OCR,
                Box::new(|ctx| {
                    ctx.update_progress("ocr", 50, "half", None);
                    Ok(json!({ "kind": "ocr" }))
                }),
            )
            .unwrap();
        for _ in 0..100 {
            let snap = mgr.get(&id).unwrap();
            if snap["status"] == "succeeded" {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let snap = mgr.get(&id).unwrap();
        assert_eq!(snap["status"], "succeeded");
        assert_eq!(snap["result"]["kind"], "ocr");
        assert!(mgr.active_task(RESOURCE_OCR).is_none());
    }

    #[test]
    fn conflict_when_resource_busy() {
        let mgr = manager();
        let first = mgr
            .start(
                RESOURCE_OCR,
                Box::new(|ctx| {
                    for _ in 0..200 {
                        if ctx.is_cancelled() {
                            return Err("操作已取消".into());
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(Value::Null)
                }),
            )
            .unwrap();
        let err = mgr
            .start(RESOURCE_OCR, Box::new(|_| Ok(Value::Null)))
            .unwrap_err();
        assert!(err.contains(&first), "conflict should name owner: {err}");
    }

    #[test]
    fn cancel_terminalizes_after_worker_exits() {
        let mgr = manager();
        let id = mgr
            .start(
                RESOURCE_OCR,
                Box::new(|ctx| {
                    for _ in 0..200 {
                        if ctx.is_cancelled() {
                            return Err("操作已取消".into());
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(Value::Null)
                }),
            )
            .unwrap();
        assert!(mgr.cancel(&id));
        for _ in 0..100 {
            if mgr.get(&id).unwrap()["status"] == "cancelled" {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(mgr.get(&id).unwrap()["status"], "cancelled");
    }

    #[test]
    fn pause_and_resume_update_task_state() {
        let mgr = manager();
        let id = mgr
            .start(
                RESOURCE_CHUNK,
                Box::new(|ctx| {
                    for _ in 0..20 {
                        ctx.wait_if_paused()?;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(Value::Null)
                }),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(10));
        assert!(mgr.pause(&id));
        // Paused is published only when the cooperative executor acknowledges stopping.
        for _ in 0..100 {
            if mgr.get(&id).unwrap()["status"] == "paused" {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(mgr.get(&id).unwrap()["status"], "paused");
        assert!(mgr.resume(&id));
        assert_eq!(mgr.get(&id).unwrap()["status"], "running");
        assert!(mgr.cancel(&id));
    }

    #[test]
    fn compacts_node_heavy_result_even_when_bytes_are_small() {
        let result = json!({
            "route": "native",
            "document": {
                "metadata": { "pageCount": 1 },
                "pages": [{
                    "number": 1,
                    "blocks": (0..5000).map(|index| json!({
                        "id": format!("b{index}"),
                        "content": "x"
                    })).collect::<Vec<_>>()
                }]
            }
        });
        let snapshot = json!({ "status": "succeeded", "result": result });
        let compact = compact_snapshot(&snapshot);
        assert_eq!(compact["resultTruncated"], true);
        assert!(JsonShape::measure(&compact["result"]).nodes <= MAX_RESULT_PREVIEW_NODES);
        assert!(serde_json::to_vec(&compact).unwrap().len() < MAX_RESULT_PREVIEW_BYTES);
        assert_eq!(compact["result"]["route"], "native");
    }
}

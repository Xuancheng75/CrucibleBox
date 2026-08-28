// Document Engine 任务管理器（Phase 2 骨架）
// 复用 UniEnv TaskManager 的可观测语义（queued → running → succeeded|failed|cancelled），
// 但支持多 resource key（ocr / parse / chunk / convert / batch），各资源默认单飞。
// 后续 Phase 可在此扩展 Worker 池与断点 checkpoint。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const RESOURCE_OCR: &str = "ocr";
pub const RESOURCE_PARSE: &str = "parse";
pub const RESOURCE_CHUNK: &str = "chunk";
pub const RESOURCE_CONVERT: &str = "convert";
pub const RESOURCE_BATCH: &str = "batch";
const MAX_RETAINED_TASKS: usize = 100;

pub struct TaskContext {
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    record: Arc<TaskRecord>,
}

#[allow(dead_code)]
impl TaskContext {
    /// Stable identifier allocated before the executor thread starts.  Worker
    /// requests use the same value so progress/result frames can be correlated
    /// with `document.jobs.get` snapshots.
    pub fn task_id(&self) -> String {
        self.record
            .core
            .lock()
            .unwrap()
            .snapshot
            .get("taskId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancelled
    }

    pub fn check_cancelled(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("操作已取消".into())
        } else {
            Ok(())
        }
    }

    pub fn wait_if_paused(&self) -> Result<(), String> {
        while self.paused.load(Ordering::SeqCst) {
            self.check_cancelled()?;
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.check_cancelled()
    }

    /// 进度上报：stage + percent + message（可选 page / speed 用于长任务）
    pub fn update_progress(&self, stage: &str, percent: u32, message: &str, extra: Option<Value>) {
        self.record.update_progress(stage, percent, message, extra);
    }

    fn mark_running(&self) {
        let mut core = self.record.core.lock().unwrap();
        if core.settled {
            return;
        }
        if core.snapshot["status"] != "paused" {
            core.snapshot["status"] = json!("running");
        }
        core.snapshot["startedAt"] = json!(now_ms());
    }
}

pub type TaskExecutor = Box<dyn FnOnce(&TaskContext) -> Result<Value, String> + Send>;

struct TaskCore {
    snapshot: Value,
    settled: bool,
}

struct TaskRecord {
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    core: Mutex<TaskCore>,
}

impl TaskRecord {
    fn update_progress(&self, stage: &str, percent: u32, message: &str, extra: Option<Value>) {
        let mut core = self.core.lock().unwrap();
        if core.settled {
            return;
        }
        let status = core.snapshot["status"].as_str().unwrap_or("");
        if status != "queued" && status != "running" {
            return;
        }
        let mut progress = json!({
            "stage": stage,
            "percent": percent.clamp(0, 100),
            "message": message,
        });
        let percent_value = percent.clamp(0, 100);
        if let Some(started_at) = core.snapshot["startedAt"].as_u64() {
            let elapsed_ms = now_ms().saturating_sub(started_at);
            progress["elapsedMs"] = json!(elapsed_ms);
            if percent_value > 0 && elapsed_ms > 0 {
                let speed = percent_value as f64 / elapsed_ms as f64 * 1000.0;
                progress["speedPercentPerSecond"] = json!(speed);
                progress["etaMs"] =
                    json!(((100 - percent_value) as f64 / speed * 1000.0).round() as u64);
            }
        }
        if let Some(extra) = extra {
            if let Some(obj) = extra.as_object() {
                for (k, v) in obj {
                    progress[k] = v.clone();
                }
            }
        }
        core.snapshot["progress"] = progress;
    }

    fn complete_succeeded(&self, result: Value) {
        let mut core = self.core.lock().unwrap();
        if core.settled {
            return;
        }
        core.settled = true;
        core.snapshot["status"] = json!("succeeded");
        core.snapshot["completedAt"] = json!(now_ms());
        core.snapshot["result"] = result;
    }

    fn complete_failed(&self, message: String) {
        let mut core = self.core.lock().unwrap();
        if core.settled {
            return;
        }
        core.settled = true;
        core.snapshot["status"] = json!("failed");
        core.snapshot["completedAt"] = json!(now_ms());
        core.snapshot["error"] = json!({ "name": "TaskError", "message": message });
    }

    fn complete_cancelled(&self) {
        let mut core = self.core.lock().unwrap();
        if core.settled {
            return;
        }
        core.settled = true;
        core.snapshot["status"] = json!("cancelled");
        core.snapshot["completedAt"] = json!(now_ms());
        core.snapshot["error"] = json!({ "name": "AbortError", "message": "用户取消了任务" });
    }

    fn status_is_active(&self) -> bool {
        let core = self.core.lock().unwrap();
        matches!(
            core.snapshot["status"].as_str(),
            Some("queued") | Some("running")
        )
    }
}

#[derive(Default)]
pub struct TaskManager {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    tasks: HashMap<String, Arc<TaskRecord>>,
    active_resources: HashMap<String, String>,
    terminal_order: Vec<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[allow(dead_code)]
fn random_task_id() -> Result<String, String> {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).map_err(|e| format!("rng failure: {e}"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[allow(dead_code)]
impl TaskManager {
    /// 启动任务。resource_key 已有活跃任务时返回 Err(冲突消息)。
    pub fn start(
        self: &Arc<Self>,
        resource_key: &str,
        executor: TaskExecutor,
    ) -> Result<String, String> {
        let resource_key = resource_key.trim().to_string();
        if resource_key.is_empty() {
            return Err("resourceKey must not be empty".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if let Some(active_id) = inner.active_resources.get(&resource_key) {
            return Err(format!(
                "Resource \"{resource_key}\" is already owned by task \"{active_id}\""
            ));
        }
        let task_id = random_task_id()?;
        let record = Arc::new(TaskRecord {
            cancelled: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            core: Mutex::new(TaskCore {
                snapshot: json!({
                    "taskId": task_id.clone(),
                    "resourceKey": resource_key.clone(),
                    "status": "queued",
                    "createdAt": now_ms(),
                }),
                settled: false,
            }),
        });
        inner.tasks.insert(task_id.clone(), record.clone());
        inner
            .active_resources
            .insert(resource_key.clone(), task_id.clone());
        drop(inner);

        let ctx = TaskContext {
            cancelled: Arc::clone(&record.cancelled),
            paused: Arc::clone(&record.paused),
            record,
        };
        let manager = Arc::clone(self);
        let release_id = task_id.clone();
        std::thread::Builder::new()
            .name(format!("doceng-task-{}", &task_id[..8.min(task_id.len())]))
            .spawn(move || {
                ctx.mark_running();
                let result = executor(&ctx);
                match result {
                    Ok(value) => ctx.record.complete_succeeded(value),
                    Err(message) => ctx.record.complete_failed(message),
                }
                manager.release_resource(&release_id);
            })
            .map_err(|e| format!("spawn task thread failed: {e}"))?;

        Ok(task_id)
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let record = {
            let inner = self.inner.lock().unwrap();
            match inner.tasks.get(task_id) {
                Some(r) => Arc::clone(r),
                None => return false,
            }
        };
        if !record.status_is_active() {
            return false;
        }
        record.cancelled.store(true, Ordering::SeqCst);
        record.complete_cancelled();
        self.release_resource(task_id);
        true
    }

    pub fn pause(&self, task_id: &str) -> bool {
        let record = {
            let inner = self.inner.lock().unwrap();
            match inner.tasks.get(task_id) {
                Some(record) => Arc::clone(record),
                None => return false,
            }
        };
        let mut core = record.core.lock().unwrap();
        if core.settled
            || !matches!(
                core.snapshot["status"].as_str(),
                Some("queued") | Some("running")
            )
        {
            return false;
        }
        record.paused.store(true, Ordering::SeqCst);
        core.snapshot["status"] = json!("paused");
        true
    }

    pub fn resume(&self, task_id: &str) -> bool {
        let record = {
            let inner = self.inner.lock().unwrap();
            match inner.tasks.get(task_id) {
                Some(record) => Arc::clone(record),
                None => return false,
            }
        };
        let mut core = record.core.lock().unwrap();
        if core.settled || core.snapshot["status"] != "paused" {
            return false;
        }
        record.paused.store(false, Ordering::SeqCst);
        core.snapshot["status"] = json!("running");
        true
    }

    pub fn get(&self, task_id: &str) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        inner.tasks.get(task_id).map(|r| {
            let core = r.core.lock().unwrap();
            core.snapshot.clone()
        })
    }

    pub fn list(&self) -> Vec<Value> {
        let inner = self.inner.lock().unwrap();
        inner
            .tasks
            .values()
            .map(|r| {
                let core = r.core.lock().unwrap();
                core.snapshot.clone()
            })
            .collect()
    }

    pub fn active_task(&self, resource_key: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        inner.active_resources.get(resource_key.trim()).cloned()
    }

    pub fn cancel_all_active(&self) -> usize {
        let ids: Vec<String> = {
            let inner = self.inner.lock().unwrap();
            inner.active_resources.values().cloned().collect()
        };
        ids.iter().filter(|id| self.cancel(id)).count()
    }

    fn release_resource(&self, task_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        let resource = inner
            .active_resources
            .iter()
            .find(|(_, id)| id == &task_id)
            .map(|(k, _)| k.clone());
        if let Some(key) = resource {
            inner.active_resources.remove(&key);
        }
        inner.terminal_order.push(task_id.to_string());
        while inner.terminal_order.len() > MAX_RETAINED_TASKS {
            let oldest = inner.terminal_order.remove(0);
            inner.tasks.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn cancel_terminalizes() {
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
        assert_eq!(mgr.get(&id).unwrap()["status"], "paused");
        assert!(mgr.resume(&id));
        assert_eq!(mgr.get(&id).unwrap()["status"], "running");
        assert!(mgr.cancel(&id));
    }
}

// UniEnv 任务管理器（1.9.11 阶段 B）
// 对齐冻结线 plugin-system/trusted-services/unienv/task-manager.ts 的可观测语义：
// - 状态机 queued → running → succeeded|failed|cancelled；cancel 即刻终结任务
//   （settled 守卫保证 worker 迟到的结果不会覆盖 cancelled）
// - resourceKey 单飞（installation 全局一次只能跑一个安装/组合包任务）
// - 快照公共形状 { taskId, resourceKey, status, createdAt, startedAt?, completedAt?,
//   progress?, result?, error? }，error = { name, message }
// - 终态任务最多保留 100 条（FIFO 淘汰）

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const INSTALLATION_RESOURCE: &str = "installation";
const MAX_RETAINED_TASKS: usize = 100;

pub struct TaskContext {
    cancelled: Arc<AtomicBool>,
    record: Arc<TaskRecord>,
}

impl TaskContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// 供长任务原语（下载/进程）轮询的取消标志
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancelled
    }

    /// 取消时返回 Err（executor 应尽快退出；最终状态由 cancel 已写为 cancelled，
    /// worker 迟到的 Ok/Err 会被 settled 守卫丢弃）
    pub fn check_cancelled(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("操作已取消".into())
        } else {
            Ok(())
        }
    }

    pub fn update_progress(&self, stage: &str, percent: u32, message: &str) {
        self.record.update_progress(stage, percent, message);
    }

    fn mark_running(&self) {
        let mut core = self.record.core.lock().unwrap();
        if core.settled {
            return;
        }
        core.snapshot["status"] = json!("running");
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
    core: Mutex<TaskCore>,
}

impl TaskRecord {
    fn update_progress(&self, stage: &str, percent: u32, message: &str) {
        let mut core = self.core.lock().unwrap();
        if core.settled {
            return;
        }
        let status = core.snapshot["status"].as_str().unwrap_or("");
        if status != "queued" && status != "running" {
            return;
        }
        core.snapshot["progress"] = json!({
            "stage": stage,
            "percent": percent.clamp(0, 100),
            "message": message,
        });
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
        // 对齐 TS serializeCancellation(new Error('用户取消了任务'))
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

fn random_task_id() -> Result<String, String> {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).map_err(|e| format!("rng failure: {e}"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

impl TaskManager {
    /// 启动任务。resourceKey 已有活跃任务时返回 Err(冲突消息)（对等
    /// DuplicateResourceTaskError 文案，服务层映射 code='task-conflict'）。
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
            record,
        };
        let manager = Arc::clone(self);
        let release_id = task_id.clone();
        std::thread::Builder::new()
            .name(format!("unienv-task-{}", &task_id[..8.min(task_id.len())]))
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

    pub fn get(&self, task_id: &str) -> Option<Value> {
        let inner = self.inner.lock().unwrap();
        inner.tasks.get(task_id).map(|r| {
            let core = r.core.lock().unwrap();
            core.snapshot.clone()
        })
    }

    pub fn active_task(&self, resource_key: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        inner.active_resources.get(resource_key.trim()).cloned()
    }

    /// 停用插件时取消全部活跃任务（对等 deactivate 语义）
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
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    fn manager() -> Arc<TaskManager> {
        Arc::new(TaskManager::default())
    }

    #[test]
    fn lifecycle_success_releases_resource() {
        let mgr = manager();
        let ran = Arc::new(AtomicUsize::new(0));
        let ran2 = Arc::clone(&ran);
        let id = mgr
            .start(
                INSTALLATION_RESOURCE,
                Box::new(move |ctx| {
                    ran2.fetch_add(1, Ordering::SeqCst);
                    assert!(!ctx.is_cancelled());
                    ctx.update_progress("downloading", 50, "half");
                    Ok(json!({ "kind": "install" }))
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
        assert_eq!(snap["result"]["kind"], "install");
        assert_eq!(snap["progress"]["percent"], 50);
        assert!(snap["startedAt"].is_number());
        assert!(snap["completedAt"].is_number());
        assert_eq!(ran.load(Ordering::SeqCst), 1);
        assert!(mgr.active_task(INSTALLATION_RESOURCE).is_none());
    }

    #[test]
    fn conflict_when_resource_busy() {
        let mgr = manager();
        let gate = Arc::new(std::sync::Barrier::new(2));
        let gate2 = Arc::clone(&gate);
        let first = mgr
            .start(
                INSTALLATION_RESOURCE,
                Box::new(move |_| {
                    gate2.wait();
                    Ok(Value::Null)
                }),
            )
            .unwrap();
        // 活跃期间再启动必须冲突
        let err = mgr
            .start(INSTALLATION_RESOURCE, Box::new(|_| Ok(Value::Null)))
            .unwrap_err();
        assert!(
            err.contains(&first),
            "conflict message should name owner: {err}"
        );
        gate.wait();
        for _ in 0..100 {
            if mgr.active_task(INSTALLATION_RESOURCE).is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // 释放后可再次启动
        assert!(mgr
            .start(INSTALLATION_RESOURCE, Box::new(|_| Ok(Value::Null)))
            .is_ok());
    }

    #[test]
    fn cancel_terminalizes_and_worker_result_discarded() {
        let mgr = manager();
        let id = mgr
            .start(
                INSTALLATION_RESOURCE,
                Box::new(|ctx| {
                    // 轮询等待取消信号
                    for _ in 0..200 {
                        if ctx.is_cancelled() {
                            return Err("操作已取消".into());
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(json!({ "late": true }))
                }),
            )
            .unwrap();
        assert!(mgr.cancel(&id));
        assert!(!mgr.cancel(&id), "second cancel must report false");
        let snap = mgr.get(&id).unwrap();
        assert_eq!(snap["status"], "cancelled");
        assert_eq!(snap["error"]["name"], "AbortError");
        assert_eq!(snap["error"]["message"], "用户取消了任务");
        for _ in 0..100 {
            if mgr.active_task(INSTALLATION_RESOURCE).is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(mgr.active_task(INSTALLATION_RESOURCE).is_none());
    }

    #[test]
    fn unknown_task_get_returns_none() {
        let mgr = manager();
        assert!(mgr.get("nonexistent").is_none());
        assert!(!mgr.cancel("nonexistent"));
    }
}

//! Compatibility projection over the shared durable task authority.
use crate::task_runtime::{Context, TaskRuntime};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
type TaskObserver = Arc<dyn Fn(Value) + Send + Sync>;
pub struct TaskContext {
    inner: Context,
}
impl TaskContext {
    pub fn runtime_context(&self) -> &Context {
        &self.inner
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
    pub fn update_progress(&self, stage: &str, percent: u32, message: &str) {
        self.inner.update_progress(stage, percent, message, None);
    }
    pub fn publish_step(
        &self,
        publish: impl FnOnce() -> Result<Value, String>,
    ) -> Result<Value, String> {
        self.inner.publish_step(publish)
    }
    #[cfg(test)]
    pub fn commit_result(
        &self,
        publish: impl FnOnce() -> Result<Value, String>,
    ) -> Result<Value, String> {
        self.inner.commit_result(publish)
    }
}
pub type TaskExecutor = Box<dyn FnOnce(&TaskContext) -> Result<Value, String> + Send>;
pub struct TaskManager {
    owner: &'static str,
    runtime: Mutex<Arc<TaskRuntime>>,
}
#[cfg(test)]
impl Default for TaskManager {
    fn default() -> Self {
        Self::new("unienv")
    }
}
impl TaskManager {
    pub fn with_runtime(owner: &'static str, runtime: Arc<TaskRuntime>) -> Self {
        Self {
            owner,
            runtime: Mutex::new(runtime),
        }
    }
    #[cfg(test)]
    pub fn new(owner: &'static str) -> Self {
        Self {
            owner,
            runtime: Mutex::new(TaskRuntime::memory()),
        }
    }

    fn runtime(&self) -> Arc<TaskRuntime> {
        self.runtime.lock().unwrap().clone()
    }
    #[cfg(test)]
    pub fn set_runtime(&self, runtime: Arc<TaskRuntime>) -> Result<(), String> {
        let mut current = self
            .runtime
            .lock()
            .map_err(|_| "task manager unavailable")?;
        if current
            .list(self.owner)
            .iter()
            .any(|v| matches!(v["status"].as_str(), Some("queued" | "running" | "paused")))
        {
            return Err("cannot replace active task authority".into());
        }
        *current = runtime;
        Ok(())
    }
    pub fn set_observer(&self, observer: TaskObserver) {
        self.runtime().set_observer(self.owner, observer);
    }
    pub fn start(
        self: &Arc<Self>,
        resource: &str,
        executor: TaskExecutor,
    ) -> Result<String, String> {
        self.runtime().start(
            self.owner,
            resource,
            Box::new(move |context| {
                let context = TaskContext {
                    inner: context.clone(),
                };
                executor(&context)
            }),
        )
    }
    pub fn cancel(&self, id: &str) -> bool {
        self.runtime().cancel(self.owner, id)
    }
    pub fn get(&self, id: &str) -> Option<Value> {
        self.runtime().get(self.owner, id)
    }
    pub fn active_task(&self, resource: &str) -> Option<String> {
        self.runtime().active(self.owner, resource)
    }
    pub fn cancel_all_active(&self) -> usize {
        self.runtime()
            .list(self.owner)
            .iter()
            .filter_map(|v| v["taskId"].as_str())
            .filter(|id| self.cancel(id))
            .count()
    }
}
pub const INSTALLATION_RESOURCE: &str = "installation";

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
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
        assert_eq!(snap["progress"]["percent"], 100);
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
    fn cancel_waits_for_worker_before_terminalizing() {
        let mgr = manager();
        let (release, blocked) = std::sync::mpsc::channel();
        let (started, ready) = std::sync::mpsc::channel();
        let id = mgr
            .start(
                INSTALLATION_RESOURCE,
                Box::new(move |_| {
                    started.send(()).unwrap();
                    blocked.recv().unwrap();
                    Ok(json!({ "late": true }))
                }),
            )
            .unwrap();
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(mgr.cancel(&id));
        assert!(!mgr.cancel(&id), "second cancel must report false");
        assert_eq!(mgr.active_task(INSTALLATION_RESOURCE), Some(id.clone()));
        assert_ne!(mgr.get(&id).unwrap()["status"], "cancelled");
        release.send(()).unwrap();
        for _ in 0..100 {
            if mgr.get(&id).unwrap()["status"] == "cancelled" {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
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
    fn cancellation_after_output_commit_begins_does_not_hide_saved_result() {
        let mgr = manager();
        let (started, ready) = std::sync::mpsc::channel();
        let id = mgr
            .start(
                INSTALLATION_RESOURCE,
                Box::new(move |ctx| {
                    ctx.commit_result(|| {
                        started.send(()).unwrap();
                        std::thread::sleep(Duration::from_millis(100));
                        Ok(json!({ "outputPath": "saved.txt" }))
                    })
                }),
            )
            .unwrap();
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            !mgr.cancel(&id),
            "reserved publication cannot become cancelled"
        );
        for _ in 0..200 {
            if mgr.get(&id).unwrap()["status"] == "succeeded" {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let snapshot = mgr.get(&id).unwrap();
        assert_eq!(snapshot["status"], "succeeded");
        assert_eq!(snapshot["result"]["outputPath"], "saved.txt");
    }

    #[test]
    fn unknown_task_get_returns_none() {
        let mgr = manager();
        assert!(mgr.get("nonexistent").is_none());
        assert!(!mgr.cancel("nonexistent"));
    }
}

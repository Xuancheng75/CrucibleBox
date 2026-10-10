//! One durable task authority. Legacy adapters expose projections, not status owners.
mod executor;

use cruciblebox_task_core::{Action, Error, Limits, Observer, Runtime, Snapshot, Status};
use cruciblebox_task_store::SqliteRepository;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
type Event = Arc<dyn Fn(Value) + Send + Sync>;
struct QuietObserver;
impl Observer for QuietObserver {
    fn persisted(&self, _: &Snapshot) {}
}
struct Control {
    cancelled: AtomicBool,
    pause: AtomicBool,
}
struct View {
    payload: Value,
    control: Arc<Control>,
}
pub struct TaskRuntime {
    authority: Runtime<SqliteRepository, QuietObserver>,
    publications: cruciblebox_file_publication::Store,
    gate: Mutex<()>,
    views: Mutex<BTreeMap<String, View>>,
    observers: Mutex<BTreeMap<String, Event>>,
}
#[derive(Clone)]
pub struct Context {
    runtime: Arc<TaskRuntime>,
    id: String,
    owner: String,
    control: Arc<Control>,
}
pub type Executor = Box<dyn FnOnce(&Context) -> Result<Value, String> + Send>;
fn error(e: Error) -> String {
    format!("task authority: {e:?}")
}
fn time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}
impl TaskRuntime {
    pub fn open(path: &Path) -> Result<Arc<Self>, String> {
        let resources = [
            "document-engine.ocr",
            "document-engine.parse",
            "document-engine.chunk",
            "document-engine.split",
            "document-engine.convert",
            "document-engine.batch",
            "document-engine.models",
            "document-engine.panic-resource",
            "unienv.installation",
            "unienv.panic-resource",
            "unienv.archive-extraction",
            "unienv.process",
            "unienv.process-output-test",
            "archive-extractor.archive-extraction",
            "plugin-process.process",
            "marketplace.download",
            "plugin-install.installation",
        ]
        .into_iter()
        .map(|s| {
            (
                s.into(),
                if s == "unienv.process" || s == "plugin-process.process" {
                    2
                } else {
                    1
                },
            )
        })
        .collect();
        let authority = Runtime::open(
            SqliteRepository::open(path).map_err(error)?,
            QuietObserver,
            Limits {
                queued: 64,
                retained: 200,
                resources,
            },
        )
        .map_err(error)?;
        let publication_path = if path == Path::new(":memory:") {
            path.to_path_buf()
        } else {
            path.with_extension("publications.sqlite")
        };
        let publications = cruciblebox_file_publication::Store::open(&publication_path)?;
        let receipts = match publications.recover() {
            Ok(report) => {
                for (journal, diagnostic) in report.blocked {
                    eprintln!("[publication] {journal}: {diagnostic}");
                }
                report.verified
            }
            Err(diagnostic) => {
                eprintln!(
                    "[publication] recovery refused without modifying receipts: {diagnostic}"
                );
                Vec::new()
            }
        };
        // A stopped process cannot assert that its in-memory checkpoint is resumable.
        // Pending publication references stay on interrupted records for diagnosis.
        for record in authority.list_all().map_err(error)? {
            if !record.status.terminal() {
                authority
                    .apply(
                        &record.owner,
                        &record.id,
                        record.sequence,
                        if let Some(receipt) = receipts.iter().find(|receipt| {
                            receipt.owner == record.owner
                                && receipt.task_id == record.id
                                && record.publication.as_deref() == Some(receipt.journal.as_str())
                        }) {
                            let refs =
                                vec![receipt.result_reference().to_string_lossy().into_owned()];
                            if receipt.terminal {
                                Action::Recover {
                                    published: Some(refs),
                                }
                            } else {
                                Action::RecoverPartialPublication {
                                    journal: receipt.journal.clone(),
                                    refs,
                                }
                            }
                        } else {
                            Action::Recover { published: None }
                        },
                    )
                    .map_err(error)?;
            }
        }
        for receipt in receipts {
            if authority
                .list(&receipt.owner)
                .map_err(error)?
                .iter()
                .any(|record| {
                    record.id == receipt.task_id
                        && record
                            .result_refs
                            .contains(&receipt.result_reference().to_string_lossy().into_owned())
                })
            {
                if let Err(diagnostic) = publications.acknowledge(&receipt.journal) {
                    eprintln!("[publication] acknowledgement retained: {diagnostic}");
                }
            }
        }
        Ok(Arc::new(Self {
            authority,
            publications,
            gate: Mutex::new(()),
            views: Mutex::new(BTreeMap::new()),
            observers: Mutex::new(BTreeMap::new()),
        }))
    }
    pub fn memory() -> Arc<Self> {
        Self::open(Path::new(":memory:")).expect("in-memory task authority")
    }
    pub fn set_observer(&self, owner: &str, event: Event) {
        self.observers
            .lock()
            .unwrap()
            .insert(owner.into(), event.clone());
        if let Ok(records) = self.authority.list(owner) {
            for record in records {
                if let Some(snapshot) = self.get(owner, &record.id) {
                    event(snapshot);
                }
            }
        }
    }
    fn snapshot(&self, owner: &str, id: &str) -> Result<Snapshot, String> {
        self.authority
            .list(owner)
            .map_err(error)?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| "task not found".into())
    }
    fn apply(&self, owner: &str, id: &str, action: Action) -> Result<Snapshot, String> {
        let old = self.snapshot(owner, id)?;
        self.authority
            .apply(owner, id, old.sequence, action)
            .map_err(error)
    }
    pub fn get(&self, owner: &str, id: &str) -> Option<Value> {
        let _gate = self.gate.lock().ok()?;
        let snapshot = self.snapshot(owner, id).ok()?;
        let mut value=self.views.lock().ok()?.get(id).map(|v|v.payload.clone()).unwrap_or_else(||json!({"taskId":id,"resourceKey":snapshot.executor.split_once('.').map(|(_,r)|r).unwrap_or(&snapshot.executor),"createdAt":0}));
        value["status"] = serde_json::to_value(&snapshot.status).ok()?;
        value["sequence"] = json!(snapshot.sequence);
        value["cancelRequested"] = json!(snapshot.cancel_requested);
        value["resultRefs"] = json!(snapshot.result_refs);
        value["publication"] = json!(snapshot.publication);
        value["checkpoint"] = json!(snapshot.checkpoint);
        if snapshot.status == Status::Cancelled {
            value["error"] = json!({"name":"AbortError","message":"用户取消了任务"});
        }
        if snapshot.status == Status::Succeeded {
            if value.get("progress").is_none() {
                value["progress"] = json!({});
            }
            value["progress"]["percent"] = json!(100);
            value["progress"]["stage"] = json!("done");
        }
        if let Some(error) = snapshot.error {
            value["error"] = json!({"name":"TaskError","message":error});
        }
        if snapshot.status.terminal() && value.get("completedAt").is_none() {
            value["completedAt"] = json!(0);
        }
        Some(value)
    }
    pub fn list(&self, owner: &str) -> Vec<Value> {
        self.authority
            .list(owner)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| self.get(owner, &s.id))
            .collect()
    }
    fn emit(&self, owner: &str, id: &str) {
        let event = self
            .observers
            .lock()
            .ok()
            .and_then(|v| v.get(owner).cloned());
        if let (Some(event), Some(value)) = (event, self.get(owner, id)) {
            event(value);
        }
    }
    pub fn active(&self, owner: &str, resource: &str) -> Option<String> {
        self.authority
            .list(owner)
            .ok()?
            .into_iter()
            .find(|s| {
                (s.resource == format!("{owner}.{resource}")
                    || self
                        .views
                        .lock()
                        .ok()
                        .and_then(|v| {
                            v.get(&s.id)
                                .map(|v| v.payload["resourceKey"].as_str() == Some(resource))
                        })
                        .unwrap_or(false))
                    && !s.status.terminal()
            })
            .map(|s| s.id)
    }
    fn prepare(
        self: &Arc<Self>,
        owner: &str,
        resource: &str,
        supplied_id: Option<&str>,
    ) -> Result<Context, String> {
        let resource = resource.trim();
        if resource.is_empty() {
            return Err("resourceKey must not be empty".into());
        }
        let gate = self.gate.lock().map_err(|_| "task authority unavailable")?;
        // Preserve the existing trusted service single-flight contract.
        if let Some(id) = self.active(owner, resource) {
            return Err(format!(
                "Resource \"{resource}\" is already owned by task \"{id}\""
            ));
        }
        let id = match supplied_id {
            Some(id) => id.to_owned(),
            None => random_task_id()?,
        };
        let record = Snapshot::queued(
            id.clone(),
            owner.into(),
            format!("{owner}.{resource}"),
            if resource.starts_with("plugin-process:") {
                format!("{owner}.process")
            } else {
                format!("{owner}.{resource}")
            },
        )
        .map_err(error)?;
        self.authority.submit(record).map_err(error)?;
        let control = Arc::new(Control {
            cancelled: AtomicBool::new(false),
            pause: AtomicBool::new(false),
        });
        {
            let live = self
                .authority
                .list_all()
                .map_err(error)?
                .into_iter()
                .map(|s| s.id)
                .collect::<std::collections::BTreeSet<_>>();
            let mut views = self.views.lock().map_err(|_| "task views unavailable")?;
            views.retain(|key, _| live.contains(key));
            views.insert(
                id.clone(),
                View {
                    payload: json!({"taskId":id,"resourceKey":resource,"createdAt":time()}),
                    control: control.clone(),
                },
            );
        }
        drop(gate);
        self.emit(owner, &id);
        let ctx = Context {
            runtime: self.clone(),
            id: id.clone(),
            owner: owner.into(),
            control,
        };

        Ok(ctx)
    }
    pub fn run_sync(
        self: &Arc<Self>,
        owner: &str,
        resource: &str,
        supplied_id: Option<&str>,
        execute: impl FnOnce(&Context) -> Result<Value, String>,
    ) -> Result<Value, String> {
        let ctx = self.prepare(owner, resource, supplied_id)?;
        let result = executor::run(|| {
            ctx.start_running()?;
            execute(&ctx)
        });
        ctx.finish(result.clone())?;
        if self.snapshot(owner, &ctx.id)?.status == Status::Cancelled {
            return Err("CANCELLED".into());
        }
        result
    }
    pub fn report_progress(
        self: &Arc<Self>,
        owner: &str,
        id: &str,
        stage: &str,
        percent: u32,
        message: &str,
    ) {
        let control = self
            .views
            .lock()
            .ok()
            .and_then(|v| v.get(id).map(|v| v.control.clone()));
        if let Some(control) = control {
            Context {
                runtime: self.clone(),
                id: id.into(),
                owner: owner.into(),
                control,
            }
            .update_progress(stage, percent, message, None);
        }
    }
    pub fn start(
        self: &Arc<Self>,
        owner: &str,
        resource: &str,
        execute: Executor,
    ) -> Result<String, String> {
        let ctx = self.prepare(owner, resource, None)?;
        let id = ctx.id.clone();
        let runtime = self.clone();
        let failure_id = id.clone();
        let failure_owner = owner.to_owned();
        let spawn = std::thread::Builder::new()
            .name(format!("task-{}", &id[..8]))
            .spawn(move || {
                let result = executor::run(|| {
                    ctx.start_running()?;
                    execute(&ctx)
                });
                if let Err(error) = ctx.finish(result) {
                    eprintln!("[task-runtime] {} completion failed: {error}", ctx.id);
                }
            });
        if let Err(e) = spawn {
            let _gate = runtime
                .gate
                .lock()
                .map_err(|_| "task authority unavailable")?;
            runtime.apply(
                &failure_owner,
                &failure_id,
                Action::Fail(format!("spawn failed: {e}")),
            )?;
            drop(_gate);
            runtime.emit(&failure_owner, &failure_id);
            return Err(format!("spawn task thread failed: {e}"));
        }
        Ok(id)
    }
    pub fn cancel(&self, owner: &str, id: &str) -> bool {
        let Ok(gate) = self.gate.lock() else {
            return false;
        };
        if self.apply(owner, id, Action::RequestCancel).is_err() {
            return false;
        }
        let Some(control) = self
            .views
            .lock()
            .ok()
            .and_then(|v| v.get(id).map(|v| v.control.clone()))
        else {
            return false;
        };
        control.cancelled.store(true, Ordering::SeqCst);
        control.pause.store(false, Ordering::SeqCst);
        drop(gate);
        self.emit(owner, id);
        true
    }
    pub fn pause(&self, owner: &str, id: &str) -> bool {
        let Ok(_gate) = self.gate.lock() else {
            return false;
        };
        let Ok(record) = self.snapshot(owner, id) else {
            return false;
        };
        if record.status != Status::Running
            || record.cancel_requested
            || record.publication.is_some()
        {
            return false;
        }
        self.views
            .lock()
            .ok()
            .and_then(|v| {
                v.get(id)
                    .map(|v| v.control.pause.swap(true, Ordering::SeqCst))
            })
            .is_some_and(|previous| !previous)
    }
    pub fn resume(&self, owner: &str, id: &str) -> bool {
        let Ok(gate) = self.gate.lock() else {
            return false;
        };
        let Ok(record) = self.snapshot(owner, id) else {
            return false;
        };
        let Some(control) = self
            .views
            .lock()
            .ok()
            .and_then(|v| v.get(id).map(|v| v.control.clone()))
        else {
            return false;
        };
        if record.cancel_requested
            || record.status.terminal()
            || !control.pause.load(Ordering::SeqCst)
        {
            return false;
        }
        if record.status == Status::Paused
            && (self.apply(owner, id, Action::Resume).is_err()
                || self.apply(owner, id, Action::Start).is_err())
        {
            return false;
        }
        control.pause.store(false, Ordering::SeqCst);
        drop(gate);
        self.emit(owner, id);
        true
    }
}
impl Context {
    fn start_running(&self) -> Result<(), String> {
        loop {
            let gate = self
                .runtime
                .gate
                .lock()
                .map_err(|_| "task authority unavailable")?;
            if self.is_cancelled() {
                self.runtime
                    .apply(&self.owner, &self.id, Action::ConfirmStopped)?;
                drop(gate);
                self.runtime.emit(&self.owner, &self.id);
                return Err("操作已取消".into());
            }
            let old = self.runtime.snapshot(&self.owner, &self.id)?;
            match self
                .runtime
                .authority
                .apply(&self.owner, &self.id, old.sequence, Action::Start)
            {
                Ok(_) => {
                    self.runtime
                        .views
                        .lock()
                        .map_err(|_| "task views unavailable")?
                        .get_mut(&self.id)
                        .ok_or("task view missing")?
                        .payload["startedAt"] = json!(time());
                    drop(gate);
                    self.runtime.emit(&self.owner, &self.id);
                    return Ok(());
                }
                Err(Error::Capacity) => {
                    drop(gate);
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(e) => return Err(error(e)),
            }
        }
    }

    pub fn task_id(&self) -> String {
        self.id.clone()
    }
    pub fn is_cancelled(&self) -> bool {
        self.control.cancelled.load(Ordering::SeqCst)
    }
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.control.cancelled
    }
    pub fn check_cancelled(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("操作已取消".into())
        } else {
            Ok(())
        }
    }
    pub fn wait_if_paused(&self) -> Result<(), String> {
        while self.control.pause.load(Ordering::SeqCst) {
            self.check_cancelled()?;
            {
                let _gate = self
                    .runtime
                    .gate
                    .lock()
                    .map_err(|_| "task authority unavailable")?;
                let record = self.runtime.snapshot(&self.owner, &self.id)?;
                if record.status == Status::Running && self.control.pause.load(Ordering::SeqCst) {
                    self.runtime.apply(
                        &self.owner,
                        &self.id,
                        Action::ConfirmPaused(format!("in-memory:{}", self.id)),
                    )?;
                }
            }
            self.runtime.emit(&self.owner, &self.id);
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        self.check_cancelled()
    }
    pub fn progress_snapshot(&self) -> Value {
        self.runtime
            .get(&self.owner, &self.id)
            .and_then(|v| v.get("progress").cloned())
            .unwrap_or_else(|| json!({"stage":"queued","percent":0,"message":""}))
    }
    pub fn update_progress(&self, stage: &str, percent: u32, message: &str, extra: Option<Value>) {
        let result = (|| -> Result<(), String> {
            let _gate = self
                .runtime
                .gate
                .lock()
                .map_err(|_| "task authority unavailable")?;
            let record = self.runtime.apply(
                &self.owner,
                &self.id,
                Action::Progress(percent.min(100) as u8),
            )?;
            let mut views = self
                .runtime
                .views
                .lock()
                .map_err(|_| "task views unavailable")?;
            let view = views.get_mut(&self.id).ok_or("task view missing")?;
            let mut progress = json!({"stage":stage,"percent":record.progress,"message":message,"sequence":record.sequence});
            if let Some(extra) = extra.and_then(|v| v.as_object().cloned()) {
                for (key, value) in extra {
                    if !["stage", "percent", "message", "sequence"].contains(&key.as_str()) {
                        progress[&key] = value;
                    }
                }
            }
            view.payload["progress"] = progress;
            Ok(())
        })();
        match result {
            Ok(()) => self.runtime.emit(&self.owner, &self.id),
            Err(e) => eprintln!("[task-runtime] {} progress rejected: {e}", self.id),
        }
    }
    fn finish(&self, result: Result<Value, String>) -> Result<(), String> {
        let gate = self
            .runtime
            .gate
            .lock()
            .map_err(|_| "task authority unavailable")?;
        let snapshot = self.runtime.snapshot(&self.owner, &self.id)?;
        if snapshot.status.terminal() {
            if snapshot.status == Status::Succeeded {
                if let Ok(value) = &result {
                    if let Some(view) = self
                        .runtime
                        .views
                        .lock()
                        .map_err(|_| "task views unavailable")?
                        .get_mut(&self.id)
                    {
                        view.payload["result"] = value.clone();
                        view.payload["completedAt"] = json!(time());
                    }
                }
                drop(gate);
                self.runtime.emit(&self.owner, &self.id);
            }
            return Ok(());
        }
        if let Err(diagnostic) = &result {
            if diagnostic.starts_with(cruciblebox_file_publication::RECOVERY_REQUIRED) {
                return Err(diagnostic.clone());
            }
        }
        let action = if self.is_cancelled() {
            Action::ConfirmStopped
        } else {
            match &result {
                Ok(value) => Action::Succeed(result_refs(value)),
                Err(e) => Action::Fail(if e.is_empty() {
                    "Executor failed without a diagnostic".into()
                } else {
                    e.replace('\0', "\u{fffd}").chars().take(256).collect()
                }),
            }
        };
        let terminal = self.runtime.apply(&self.owner, &self.id, action)?;
        self.runtime
            .views
            .lock()
            .map_err(|_| "task views unavailable")?
            .get_mut(&self.id)
            .ok_or("task view missing")?
            .payload["completedAt"] = json!(time());
        if let Ok(value) = if terminal.status == Status::Succeeded {
            result
        } else {
            Err("cancelled".into())
        } {
            self.runtime
                .views
                .lock()
                .map_err(|_| "task views unavailable")?
                .get_mut(&self.id)
                .ok_or("task view missing")?
                .payload["result"] = value;
        }
        drop(gate);
        self.runtime.emit(&self.owner, &self.id);
        Ok(())
    }
    pub fn publication_pending(&self) -> bool {
        self.runtime
            .snapshot(&self.owner, &self.id)
            .is_ok_and(|snapshot| snapshot.publication.is_some())
    }
    pub fn publish_file(
        &self,
        stage: &Path,
        target: &Path,
        replace: bool,
        terminal: bool,
    ) -> Result<Value, String> {
        self.publish_object(stage, target, replace, terminal, None)
    }
    pub fn publish_object(
        &self,
        stage: &Path,
        target: &Path,
        replace: bool,
        terminal: bool,
        reference: Option<&Path>,
    ) -> Result<Value, String> {
        let reference = reference
            .map(Path::canonicalize)
            .transpose()
            .map_err(|e| e.to_string())?;
        let reference = reference.as_deref();
        let mut used_journal = None;
        let result = self.publish(
            || {
                let journal = self
                    .runtime
                    .snapshot(&self.owner, &self.id)?
                    .publication
                    .ok_or("publication reservation missing")?;
                used_journal = Some(journal.clone());
                let path = self.runtime.publications.publish_with_reference(
                    cruciblebox_file_publication::Publication {
                        owner: &self.owner,
                        task_id: &self.id,
                        journal: &journal,
                        stage,
                        target,
                        replace,
                        terminal,
                    },
                    reference,
                )?;
                Ok(json!({"path":reference.unwrap_or(&path).to_string_lossy()}))
            },
            terminal,
        )?;
        if let Some(journal) = used_journal {
            if let Err(diagnostic) = self.runtime.publications.acknowledge(&journal) {
                eprintln!("[publication] acknowledgement retained: {diagnostic}");
            }
        }
        Ok(result)
    }
    pub fn publish_link(
        &self,
        stage: &Path,
        target: &Path,
        replace: bool,
        terminal: bool,
        reference: Option<&Path>,
    ) -> Result<Value, String> {
        let reference = reference
            .map(Path::canonicalize)
            .transpose()
            .map_err(|e| e.to_string())?;
        let reference = reference.as_deref();
        let mut used_journal = None;
        let result = self.publish(
            || {
                let journal = self
                    .runtime
                    .snapshot(&self.owner, &self.id)?
                    .publication
                    .ok_or("publication reservation missing")?;
                used_journal = Some(journal.clone());
                let path = self.runtime.publications.publish_link(
                    cruciblebox_file_publication::Publication {
                        owner: &self.owner,
                        task_id: &self.id,
                        journal: &journal,
                        stage,
                        target,
                        replace,
                        terminal,
                    },
                    reference,
                )?;
                Ok(json!({"path":reference.unwrap_or(&path).to_string_lossy()}))
            },
            terminal,
        )?;
        if let Some(journal) = used_journal {
            if let Err(diagnostic) = self.runtime.publications.acknowledge(&journal) {
                eprintln!("[publication] acknowledgement retained: {diagnostic}");
            }
        }
        Ok(result)
    }
    pub fn commit_result(
        &self,
        publish: impl FnOnce() -> Result<Value, String>,
    ) -> Result<Value, String> {
        self.publish(publish, true)
    }
    /// Publish one durable batch result without terminalizing the whole task.
    pub fn publish_step(
        &self,
        publish: impl FnOnce() -> Result<Value, String>,
    ) -> Result<Value, String> {
        self.publish(publish, false)
    }
    fn publish(
        &self,
        publish: impl FnOnce() -> Result<Value, String>,
        terminal: bool,
    ) -> Result<Value, String> {
        let gate = self
            .runtime
            .gate
            .lock()
            .map_err(|_| "task authority unavailable")?;
        let journal = format!("publication:{}", random_task_id()?);
        self.runtime.apply(
            &self.owner,
            &self.id,
            Action::PreparePublication(journal.clone()),
        )?;
        drop(gate);
        // The durable reservation rejects cancellation; file I/O holds no task or DB lock.
        let result = executor::run(publish);
        let gate = self
            .runtime
            .gate
            .lock()
            .map_err(|_| "task authority unavailable")?;
        match result {
            Ok(value) => {
                self.runtime.apply(
                    &self.owner,
                    &self.id,
                    if terminal {
                        Action::Published {
                            journal: journal.clone(),
                            refs: result_refs(&value),
                        }
                    } else {
                        Action::PublicationCommitted {
                            journal: journal.clone(),
                            refs: result_refs(&value),
                        }
                    },
                )?;
                self.runtime
                    .views
                    .lock()
                    .map_err(|_| "task views unavailable")?
                    .get_mut(&self.id)
                    .ok_or("task view missing")?
                    .payload["result"] = value.clone();
                drop(gate);
                self.runtime.emit(&self.owner, &self.id);
                Ok(value)
            }
            Err(e) => {
                if e.starts_with(cruciblebox_file_publication::RECOVERY_REQUIRED) {
                    return Err(e);
                }
                self.runtime
                    .apply(&self.owner, &self.id, Action::PublicationAborted(journal))?;
                Err(e)
            }
        }
    }
}
fn result_refs(value: &Value) -> Vec<String> {
    ["outputPath", "path", "filePath", "destination"]
        .into_iter()
        .filter_map(|k| value.get(k).and_then(Value::as_str))
        .filter(|p| !p.is_empty() && p.len() <= 2048 && !p.contains('\0'))
        .map(str::to_owned)
        .collect()
}

fn random_task_id() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("rng failure: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn terminal_snapshot_waits_for_result_publication() {
        let runtime = TaskRuntime::memory();
        let ctx = runtime
            .prepare(
                "archive-extractor",
                "archive-extraction",
                Some("snapshot-race"),
            )
            .unwrap();
        ctx.start_running().unwrap();
        let gate = runtime.gate.lock().unwrap();
        runtime
            .apply(
                "archive-extractor",
                "snapshot-race",
                Action::Succeed(vec!["archive.zip".into()]),
            )
            .unwrap();
        let reader = runtime.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            tx.send(reader.get("archive-extractor", "snapshot-race").unwrap())
                .unwrap();
        });
        let premature = rx.recv_timeout(std::time::Duration::from_millis(100));
        runtime
            .views
            .lock()
            .unwrap()
            .get_mut("snapshot-race")
            .unwrap()
            .payload["result"] = json!({"destination":"archive.zip"});
        drop(gate);
        handle.join().unwrap();
        assert!(
            premature.is_err(),
            "terminal state escaped before its result"
        );
        let snapshot = rx.recv().unwrap();
        assert_eq!(snapshot["status"], "succeeded");
        assert_eq!(snapshot["result"]["destination"], "archive.zip");
    }

    #[test]
    fn physical_output_then_terminal_sql_failure_recovers_matching_receipt() {
        for terminal in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let journal = dir.path().join("tasks.sqlite");
            let stage = dir.path().join(".output.stage");
            let target = dir.path().join("output.txt");
            std::fs::write(&stage, b"verified output").unwrap();
            std::fs::write(&target, b"original").unwrap();
            let runtime = TaskRuntime::open(&journal).unwrap();
            let ctx = runtime
                .prepare("document-engine", "convert", Some("receipt-fault"))
                .unwrap();
            ctx.start_running().unwrap();
            let fault = rusqlite::Connection::open(&journal).unwrap();
            fault.execute_batch("CREATE TRIGGER fail_publication BEFORE UPDATE ON next_tasks WHEN json_extract(NEW.snapshot, '$.status') = 'succeeded' OR (json_extract(OLD.snapshot, '$.publication') IS NOT NULL AND json_extract(NEW.snapshot, '$.publication') IS NULL) BEGIN SELECT RAISE(ABORT, 'injected publication persistence failure'); END;").unwrap();
            assert!(ctx.publish_file(&stage, &target, true, terminal).is_err());
            assert_eq!(std::fs::read(&target).unwrap(), b"verified output");
            assert!(ctx.publication_pending());
            assert!(stage.exists());
            drop(ctx);
            drop(runtime);
            fault
                .execute_batch("DROP TRIGGER fail_publication")
                .unwrap();
            drop(fault);
            let reopened = TaskRuntime::open(&journal).unwrap();
            let snapshot = reopened.get("document-engine", "receipt-fault").unwrap();
            assert_eq!(
                snapshot["status"],
                if terminal { "succeeded" } else { "interrupted" }
            );
            assert_eq!(
                snapshot["resultRefs"],
                json!([target.canonicalize().unwrap().to_string_lossy()])
            );
            assert!(!stage.exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"verified output");
        }
    }

    #[test]
    fn startup_replays_authority_and_final_publication_keeps_prior_results() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("replay.sqlite");
        let runtime = TaskRuntime::open(&journal).unwrap();
        runtime
            .run_sync("document-engine", "convert", Some("multi-output"), |ctx| {
                ctx.publish_step(|| Ok(json!({"path":"first.pdf"})))?;
                ctx.commit_result(|| Ok(json!({"path":"second.pdf"})))
            })
            .unwrap();
        drop(runtime);
        let runtime = TaskRuntime::open(&journal).unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let events = received.clone();
        runtime.set_observer(
            "document-engine",
            Arc::new(move |snapshot| {
                events.lock().unwrap().push(snapshot);
            }),
        );
        let values = received.lock().unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["status"], "succeeded");
        assert_eq!(values[0]["resultRefs"], json!(["first.pdf", "second.pdf"]));
    }

    #[test]
    fn synchronous_cancellation_cannot_return_success() {
        let runtime = TaskRuntime::memory();
        let result = runtime.run_sync("marketplace", "download", Some("cancel-race"), |_| {
            assert!(runtime.cancel("marketplace", "cancel-race"));
            Ok(json!({"path":"not-published"}))
        });
        assert_eq!(result, Err("CANCELLED".into()));
        assert_eq!(
            runtime.get("marketplace", "cancel-race").unwrap()["status"],
            "cancelled"
        );
    }
    #[test]
    fn cancelling_batch_keeps_published_output_and_durable_reference() {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("tasks.sqlite");
        let output = dir.path().join("installed-runtime.txt");
        let path = output.to_str().unwrap().to_owned();
        let runtime = TaskRuntime::open(&journal).unwrap();
        assert!(runtime
            .run_sync("unienv", "installation", Some("partial-install"), |ctx| {
                ctx.publish_step(|| {
                    assert!(!ctx.runtime.cancel("unienv", "partial-install"));
                    std::fs::write(&output, b"verified runtime").map_err(|e| e.to_string())?;
                    Ok(json!({"path": path}))
                })?;
                assert_eq!(
                    ctx.runtime.get("unienv", "partial-install").unwrap()["status"],
                    "running"
                );
                assert!(ctx.runtime.cancel("unienv", "partial-install"));
                ctx.check_cancelled()?;
                Ok(Value::Null)
            })
            .is_err());
        let result = runtime.get("unienv", "partial-install").unwrap();
        assert_eq!(result["status"], "cancelled");
        assert_eq!(result["resultRefs"], json!([path]));
        drop(runtime);
        let reopened = TaskRuntime::open(&journal).unwrap();
        assert_eq!(
            reopened.get("unienv", "partial-install").unwrap()["resultRefs"],
            json!([path])
        );
        assert_eq!(std::fs::read(output).unwrap(), b"verified runtime");
    }
    #[test]
    fn empty_or_nul_diagnostics_terminalize_without_leaking_resource() {
        let runtime = TaskRuntime::memory();
        for (id, diagnostic) in [("empty-error", ""), ("nul-error", "bad\0error")] {
            assert!(runtime
                .run_sync("unienv", "installation", Some(id), |_| Err(
                    diagnostic.into()
                ))
                .is_err());
            let record = runtime.get("unienv", id).unwrap();
            assert_eq!(record["status"], "failed");
            assert!(!record["error"]["message"].as_str().unwrap().is_empty());
            assert!(!record["error"]["message"].as_str().unwrap().contains('\0'));
        }
        assert!(runtime
            .run_sync("unienv", "installation", Some("after-error"), |_| Ok(
                Value::Null
            ))
            .is_ok());
    }
    use super::*;
    fn wait(runtime: &TaskRuntime, owner: &str, id: &str, status: &str) -> Value {
        for _ in 0..300 {
            let value = runtime.get(owner, id).unwrap();
            if value["status"] == status {
                return value;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out: {:?}", runtime.get(owner, id));
    }
    #[test]
    fn three_executors_share_real_journal_and_restart_result_refs() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("tasks.sqlite");
        let runtime = TaskRuntime::open(&path).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        for owner in ["document-engine", "unienv", "marketplace"] {
            let events = seen.clone();
            runtime.set_observer(owner, Arc::new(move |v| events.lock().unwrap().push(v)));
        }
        let output = scratch.path().join("document.md");
        let document = runtime
            .start(
                "document-engine",
                "convert",
                Box::new(move |ctx| {
                    ctx.update_progress("convert", 30, "write", None);
                    ctx.commit_result(|| {
                        std::fs::write(&output, "实际文档输出").map_err(|e| e.to_string())?;
                        Ok(json!({"outputPath":output.to_string_lossy()}))
                    })
                }),
            )
            .unwrap();
        let installation = runtime
            .start(
                "unienv",
                "installation",
                Box::new(|ctx| {
                    ctx.update_progress("install", 80, "done", None);
                    Ok(json!({"kind":"install"}))
                }),
            )
            .unwrap();
        let download = runtime
            .run_sync("marketplace", "download", Some("download-test"), |ctx| {
                ctx.update_progress("download", 70, "received", None);
                ctx.commit_result(|| Ok(json!({"path":"C:/verified/plugin.zip"})))
            })
            .unwrap();
        assert_eq!(download["path"], "C:/verified/plugin.zip");
        assert_eq!(
            wait(&runtime, "document-engine", &document, "succeeded")["resultRefs"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        wait(&runtime, "unienv", &installation, "succeeded");
        assert!(runtime.get("unienv", &document).is_none());
        assert!(!runtime.cancel("unienv", &document));
        assert!(!runtime.cancel("marketplace", "download-test"));
        assert_eq!(
            std::fs::read_to_string(scratch.path().join("document.md")).unwrap(),
            "实际文档输出"
        );
        assert!(!seen.lock().unwrap().is_empty());
        drop(runtime);
        let reopened = TaskRuntime::open(&path).unwrap();
        assert_eq!(
            reopened.get("marketplace", "download-test").unwrap()["status"],
            "succeeded"
        );
        assert_eq!(
            reopened.get("marketplace", "download-test").unwrap()["resultRefs"][0],
            "C:/verified/plugin.zip"
        );
        assert_eq!(
            reopened.get("document-engine", &document).unwrap()["status"],
            "succeeded"
        );
    }
    #[test]
    fn publication_panic_is_diagnostic_and_does_not_poison_other_tasks() {
        let runtime = TaskRuntime::memory();
        assert!(runtime
            .run_sync("marketplace", "download", Some("publish-panic"), |ctx| ctx
                .commit_result(|| panic!("injected")))
            .is_err());
        assert_eq!(
            runtime.get("marketplace", "publish-panic").unwrap()["status"],
            "failed"
        );
        assert_eq!(
            runtime
                .run_sync("marketplace", "download", Some("next-download"), |_| Ok(
                    json!(1)
                ))
                .unwrap(),
            json!(1)
        );
    }
    #[test]
    fn stopped_process_marks_unfinished_record_interrupted_without_reusing_id() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("tasks.sqlite");
        {
            let runtime = TaskRuntime::open(&path).unwrap();
            let _ctx = runtime
                .prepare("marketplace", "download", Some("interrupted-download"))
                .unwrap();
        }
        let runtime = TaskRuntime::open(&path).unwrap();
        assert_eq!(
            runtime.get("marketplace", "interrupted-download").unwrap()["status"],
            "interrupted"
        );
        assert!(runtime
            .run_sync(
                "marketplace",
                "download",
                Some("interrupted-download"),
                |_| Ok(Value::Null)
            )
            .is_err());
    }
    #[test]
    fn queued_process_cancel_never_runs_executor_or_releases_running_leases_early() {
        let runtime = TaskRuntime::memory();
        let release = Arc::new(AtomicBool::new(false));
        let mut running = Vec::new();
        for i in 0..2 {
            let release = release.clone();
            let id = runtime
                .start(
                    "unienv",
                    &format!("plugin-process:{i}"),
                    Box::new(move |ctx| {
                        while !release.load(Ordering::SeqCst) {
                            ctx.check_cancelled()?;
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Ok(Value::Null)
                    }),
                )
                .unwrap();
            wait(&runtime, "unienv", &id, "running");
            running.push(id);
        }
        let ran = Arc::new(AtomicBool::new(false));
        let record = ran.clone();
        let queued = runtime
            .start(
                "unienv",
                "plugin-process:third",
                Box::new(move |_| {
                    record.store(true, Ordering::SeqCst);
                    Ok(Value::Null)
                }),
            )
            .unwrap();
        assert_eq!(runtime.get("unienv", &queued).unwrap()["status"], "queued");
        assert!(runtime.cancel("unienv", &queued));
        wait(&runtime, "unienv", &queued, "cancelled");
        assert!(!ran.load(Ordering::SeqCst));
        for id in &running {
            assert_eq!(runtime.get("unienv", id).unwrap()["status"], "running");
        }
        release.store(true, Ordering::SeqCst);
        for id in running {
            wait(&runtime, "unienv", &id, "succeeded");
        }
    }
}

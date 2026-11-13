//! Next 3.0 admission and generic backend routing.
mod capability_service;
mod download_service;
mod event_service;
mod file_mutation;
mod file_operations;
mod lifecycle;
mod native_publication;
mod network_service;
pub(crate) mod platform_service;
mod process_service;
mod stream_service;
mod websocket_service;
use crate::{db::Db, plugin_session::RendererSession};
use cruciblebox_next2_protocol::{
    self as protocol,
    gateway::{Installed, Registry, Route, Session},
    Request, RpcError,
};
use cruciblebox_next2_runtime::{
    package::ApprovedPlugin,
    worker::{CallLimits, InstanceKind, StopHandle, Worker},
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}
fn error(message: String) -> RpcError {
    let code = message
        .split_once(": ")
        .map_or(message.as_str(), |(code, _)| code);
    RpcError {
        code: if !code.is_empty()
            && code.len() <= 100
            && code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            code.into()
        } else {
            "PLATFORM_ERROR".into()
        },
        message: message.chars().take(2048).collect(),
    }
}

fn managed_child(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let directory = parent.join(name);
    if let Err(error) = std::fs::create_dir(&directory) {
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(format!("MANAGED_DIRECTORY_UNAVAILABLE: {error}"));
        }
    }
    crate::transaction::assert_plain_directory(&directory, "Managed plugin data")?;
    Ok(directory)
}
struct Slot {
    version: String,
    token: String,
    background_interval: Option<Duration>,
    worker: Mutex<Option<Worker>>,
    stop: StopHandle,
    admission: Mutex<Session>,
}
enum Instance {
    Starting,
    Ready(Arc<Slot>),
}
type Instances = HashMap<(String, InstanceKind), Instance>;
struct SessionEntry {
    plugin_id: String,
    session: Arc<Mutex<Session>>,
}
struct BackendCall<'a> {
    provider: &'a str,
    execution: Execution<'a>,
    sequence: &'a mut u64,
}
#[derive(Clone, Copy, Default)]
struct Execution<'a> {
    caller: Option<&'a str>,
    task: Option<(&'a crate::task_runtime::Context, &'a str)>,
    cancel: Option<&'a AtomicBool>,
    deadline: Option<Instant>,
}
impl Execution<'_> {
    fn check(self) -> Result<(), String> {
        if self.cancel.is_some_and(|flag| flag.load(Ordering::SeqCst))
            || self.task.is_some_and(|(context, _)| context.is_cancelled())
        {
            return Err("CANCELLED".into());
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(if self.task.is_some() {
                "TASK_TIMEOUT"
            } else {
                "RPC_TIMEOUT"
            }
            .into());
        }
        Ok(())
    }
}
pub struct Manager {
    runner: PathBuf,
    db: Arc<Mutex<Db>>,
    services: Arc<crate::host_services::Services>,
    maintenance: Arc<Mutex<HashSet<String>>>,
    workers: Mutex<Instances>,
    provider_failures: Mutex<HashMap<String, String>>,
    startup_finished: Condvar,
    sessions: Mutex<HashMap<String, SessionEntry>>,
    closing: AtomicBool,
    files: crate::next2_files::Files,
    file_operations: file_operations::MutableFiles,
    downloads: download_service::Downloads,
    websockets: websocket_service::WebSockets,
    events: event_service::Events,
    streams: stream_service::Streams,
    graph_changes: Mutex<()>,
}
impl Manager {
    pub fn new(
        db: Arc<Mutex<Db>>,
        services: Arc<crate::host_services::Services>,
        maintenance: Arc<Mutex<HashSet<String>>>,
        runner: PathBuf,
    ) -> Self {
        Self {
            runner,
            db,
            services,
            maintenance,
            workers: Mutex::new(HashMap::new()),
            provider_failures: Mutex::new(HashMap::new()),
            startup_finished: Condvar::new(),
            sessions: Mutex::new(HashMap::new()),
            closing: AtomicBool::new(false),
            files: crate::next2_files::Files::default(),
            file_operations: file_operations::MutableFiles::default(),
            downloads: download_service::Downloads::default(),
            websockets: websocket_service::WebSockets::default(),
            events: event_service::Events::default(),
            streams: stream_service::Streams::default(),
            graph_changes: Mutex::new(()),
        }
    }
    fn record(&self, id: &str) -> Result<crate::db::PluginRow, String> {
        self.db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_find_by_name(id)?
            .filter(|row| row.enabled)
            .ok_or("SESSION_DENIED".into())
    }
    fn check(&self, row: &crate::db::PluginRow) -> Result<(), String> {
        if self.closing.load(Ordering::SeqCst) {
            return Err("HOST_STOPPING".into());
        }
        if self
            .maintenance
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .contains(&row.id)
        {
            return Err("PLUGIN_IN_MAINTENANCE".into());
        }
        Ok(())
    }
    fn installed(&self, row: &crate::db::PluginRow) -> Result<ApprovedPlugin, String> {
        self.check(row)?;
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let plugin = crate::next2_install::load(&db, Path::new(&row.installed_path))?;
        Self::verify_record(plugin.manifest(), &row.name, &row.version, &row.permissions)?;
        Ok(plugin)
    }
    fn verify_record(
        manifest: &protocol::Manifest,
        id: &str,
        version: &str,
        permissions: &str,
    ) -> Result<(), String> {
        if manifest.id != id
            || manifest.version != version
            || serde_json::to_string(&manifest.permissions).map_err(|e| e.to_string())?
                != permissions
        {
            return Err("INSTALLATION_METADATA_MISMATCH".into());
        }
        Ok(())
    }
    fn registry(&self) -> Result<Registry, String> {
        self.registry_with(None)
    }
    fn registry_with(&self, replacement: Option<&protocol::Manifest>) -> Result<Registry, String> {
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let records = db
            .plugin_all_roots()?
            .into_iter()
            .map(|(id, _, _)| {
                db.plugin_find_by_id(&id)?
                    .ok_or_else(|| "PLUGIN_NOT_FOUND".to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut installed = Vec::new();
        for row in records {
            if replacement.is_some_and(|manifest| manifest.id == row.name) {
                continue;
            }
            // A row without approved metadata cannot contribute any capabilities.
            // Its own activation/request still reports the underlying installation error.
            let manifest = crate::next2_install::load_manifest(&db, Path::new(&row.installed_path));
            if let Ok(manifest) = manifest.and_then(|manifest| {
                Self::verify_record(&manifest, &row.name, &row.version, &row.permissions)?;
                Ok(manifest)
            }) {
                installed.push(Installed {
                    manifest,
                    enabled: row.enabled,
                });
            }
        }
        if let Some(manifest) = replacement {
            installed.push(Installed {
                manifest: manifest.clone(),
                enabled: true,
            });
        }
        let mut runtimes = HashMap::new();
        for plugin in &installed {
            if !plugin.enabled {
                continue;
            }
            let declarations = plugin
                .manifest
                .runtime_capabilities
                .as_deref()
                .unwrap_or_default();
            if declarations.is_empty() {
                continue;
            }
            let metadata = (|| {
                let row = db
                    .plugin_find_by_name(&plugin.manifest.id)?
                    .ok_or("PLUGIN_NOT_FOUND")?;
                let store = Self::data(&row)?
                    .parent()
                    .and_then(Path::parent)
                    .ok_or("INVALID_DATA_PATH")?
                    .join("plugin-resources");
                cruciblebox_next2_runtime::resource::resolve_metadata(
                    &store,
                    &plugin.manifest,
                    &crate::next2_install::keys(&db)?,
                    &crate::next2_install::resource_approvals(&db, &plugin.manifest.id)?,
                )
            })();
            for runtime in declarations {
                let value = match &metadata {
                    Err(failure) => Err(failure.clone()),
                    Ok(mounts) => plugin
                        .manifest
                        .tools
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .find(|tool| tool.id == runtime.tool_id)
                        .and_then(|tool| {
                            mounts
                                .iter()
                                .find(|(mount, manifest)| {
                                    mount.id == tool.resource_id
                                        && manifest.kind == "runtime"
                                        && manifest.contains_native_code
                                        && manifest.runtime_identity.as_ref().is_some_and(
                                            |identity| identity.engine == runtime.engine,
                                        )
                                        && manifest
                                            .files
                                            .iter()
                                            .any(|entry| entry.path == tool.entry)
                                })
                                .map(|(mount, manifest)| protocol::RuntimeEnvironment {
                                    engine: runtime.engine.clone(),
                                    version: manifest
                                        .runtime_identity
                                        .as_ref()
                                        .unwrap()
                                        .version
                                        .clone(),
                                    resource_version: mount.version.clone(),
                                    resource_id: tool.resource_id.clone(),
                                    tool_id: tool.id.clone(),
                                })
                        })
                        .ok_or_else(|| "PROCESS_RESOURCE_MISSING".to_string()),
                };
                runtimes.insert(
                    (plugin.manifest.id.clone(), runtime.capability_id.clone()),
                    value,
                );
            }
        }
        Registry::new(installed).map(|registry| registry.with_runtimes(runtimes))
    }
    pub fn dispose_renderer_session(&self, token: &str) -> Result<(), String> {
        let entry = self
            .sessions
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .remove(token);
        if let Some(entry) = entry {
            entry
                .session
                .lock()
                .map_err(|_| "INTERNAL_ERROR")?
                .cancel_all();
        }
        self.clear_session_resources(token)
    }
    fn clear_session_resources(&self, token: &str) -> Result<(), String> {
        self.events.clear_session(token)?;
        self.streams.clear_session(token)?;
        self.file_operations.clear_session(token)?;
        self.websockets.clear_session(token)?;
        Ok(())
    }
    pub fn dispose_plugin_renderers(&self, plugin_id: &str) -> Result<(), String> {
        let tokens: Vec<_> = self
            .sessions
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .iter()
            .filter(|(_, entry)| entry.plugin_id == plugin_id)
            .map(|(token, _)| token.clone())
            .collect();
        for token in tokens {
            self.dispose_renderer_session(&token)?;
        }
        Ok(())
    }
    pub fn graph_change(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.graph_changes
            .try_lock()
            .map_err(|_| "PLUGIN_GRAPH_BUSY".into())
    }
    pub fn assert_can_remove(&self, id: &str) -> Result<(), String> {
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let row = db.plugin_find_by_id(id)?.ok_or("PLUGIN_NOT_FOUND")?;
        if !row.enabled {
            return Ok(());
        }
        let dependents = self.registry()?.dependents(&row.name);
        if dependents.is_empty() {
            Ok(())
        } else {
            Err(format!("DEPENDENTS_ACTIVE: {}", dependents.join(", ")))
        }
    }
    pub fn assert_upgrade(&self, replacement: &protocol::Manifest) -> Result<(), String> {
        let enabled = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_find_by_name(&replacement.id)?
            .is_some_and(|row| row.enabled);
        if enabled {
            self.validate_graph_change(replacement)?;
        }
        Ok(())
    }
    fn validate_graph_change(&self, replacement: &protocol::Manifest) -> Result<(), String> {
        let current = self.registry()?;
        let next = self.registry_with(Some(replacement))?;
        next.validate_plugin(&replacement.id)?;
        let mut affected = current.dependents(&replacement.id);
        affected.extend(next.dependents(&replacement.id));
        affected.sort();
        affected.dedup();
        for dependent in affected {
            next.validate_plugin(&dependent)?;
        }
        Ok(())
    }
    pub fn activate(self: &Arc<Self>, owner: &str) -> Result<(), String> {
        let row = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_find_by_id(owner)?
            .filter(|row| row.enabled)
            .ok_or("SESSION_DENIED")?;
        self.installed(&row)?;
        let registry = self.registry()?;
        for manifest in registry.activation_order(&row.name)? {
            if manifest.backend.is_some() {
                self.slot(&manifest.id, InstanceKind::Service)?;
            }
        }
        Ok(())
    }
    /// Restore enabled background services after install recovery. Reuse each shared
    /// dependency's startup outcome for this pass, and leave enabled choices intact.
    pub fn restore_background(self: &Arc<Self>) -> Result<(), String> {
        let _graph = self.graph_changes.lock().map_err(|_| "INTERNAL_ERROR")?;
        let registry = self.registry()?;
        let mut started = HashMap::new();
        for id in registry.background_plugins() {
            if self.closing.load(Ordering::SeqCst) {
                break;
            }
            let result = registry.activation_order(id).and_then(|order| {
                for plugin in order
                    .into_iter()
                    .filter(|manifest| manifest.backend.is_some())
                {
                    started
                        .entry(plugin.id.as_str())
                        .or_insert_with(|| self.slot(&plugin.id, InstanceKind::Service).map(|_| ()))
                        .clone()
                        .map_err(|error| format!("{}: {error}", plugin.id))?;
                }
                Ok(())
            });
            if let Err(error) = result {
                let row = self.record(id)?;
                let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
                if let Err(write_error) = db.log_write(
                    &row.id,
                    "error",
                    &format!("Background activation failed: {error}"),
                ) {
                    eprintln!("[next2] background activation for {id}: {error}; log write failed: {write_error}");
                }
            }
        }
        Ok(())
    }
    /// Start under the lifecycle lock, then persist the user's enabled choice.
    pub fn enable(self: &Arc<Self>, owner: &str) -> Result<(), String> {
        let row = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_find_by_id(owner)?
            .ok_or("PLUGIN_NOT_FOUND")?;
        let plugin = self.installed(&row)?;
        self.validate_graph_change(plugin.manifest())?;
        let registry = self.registry_with(Some(plugin.manifest()))?;
        for dependency in registry.activation_order(&row.name)? {
            if dependency.id != row.name && dependency.backend.is_some() {
                self.slot(&dependency.id, InstanceKind::Service)?;
            }
        }
        if plugin.manifest().backend.is_some() {
            self.slot_for_record(&row, InstanceKind::Service)?;
        }
        let saved = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .set_plugin_enabled(owner, true)
            .map_err(|error| error.to_string());
        if let Err(error) = saved {
            self.stop(owner)
                .map_err(|stop| format!("{error}; stop failed: {stop}"))?;
            return Err(error);
        }
        Ok(())
    }
    fn slot(self: &Arc<Self>, provider: &str, kind: InstanceKind) -> Result<Arc<Slot>, String> {
        let row = self.record(provider)?;
        self.slot_for_record(&row, kind)
    }
    fn data(row: &crate::db::PluginRow) -> Result<PathBuf, String> {
        Ok(
            crate::next2_install::data_root(Path::new(&row.installed_path))?
                .join("plugin-data")
                .join(&row.name),
        )
    }

    fn writable_data(row: &crate::db::PluginRow) -> Result<PathBuf, String> {
        let root = crate::next2_install::data_root(Path::new(&row.installed_path))?;
        let parent = managed_child(root, "plugin-data")?;
        managed_child(&parent, &row.name)
    }
    fn mounts(
        &self,
        row: &crate::db::PluginRow,
        plugin: &ApprovedPlugin,
    ) -> Result<Vec<protocol::ResourceMount>, String> {
        self.verified_mounts(row, plugin)
            .map(|verified| verified.into_iter().map(|(mount, _)| mount).collect())
    }

    fn verified_mounts(
        &self,
        row: &crate::db::PluginRow,
        plugin: &ApprovedPlugin,
    ) -> Result<Vec<(protocol::ResourceMount, protocol::ResourceManifest)>, String> {
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let store = Self::data(row)?
            .parent()
            .and_then(Path::parent)
            .ok_or("INVALID_DATA_PATH")?
            .join("plugin-resources");
        cruciblebox_next2_runtime::resource::resolve_verified(
            &store,
            plugin,
            &crate::next2_install::keys(&db)?,
            &crate::next2_install::resource_approvals(&db, &plugin.manifest().id)?,
        )
    }
    pub fn stop(&self, owner: &str) -> Result<(), String> {
        crate::diagnostics::record("backend-stop-start", Some(owner));
        let slots: Vec<_> = {
            let workers = self.workers.lock().map_err(|_| "INTERNAL_ERROR")?;
            let (mut workers, startup) = self
                .startup_finished
                .wait_timeout_while(workers, Duration::from_secs(15), |workers| {
                    workers
                        .iter()
                        .any(|((id, _), slot)| id == owner && matches!(slot, Instance::Starting))
                })
                .map_err(|_| "INTERNAL_ERROR")?;
            if startup.timed_out() {
                return Err("BACKEND_START_STILL_PENDING".into());
            }
            [InstanceKind::Task, InstanceKind::Service]
                .into_iter()
                .filter_map(|kind| match workers.remove(&(owner.into(), kind)) {
                    Some(Instance::Ready(slot)) => Some(slot),
                    _ => None,
                })
                .collect()
        };
        let runtime = &self.services.runtime;
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let provider = db
            .plugin_find_by_id(owner)?
            .map(|row| row.name)
            .unwrap_or_else(|| owner.into());
        let mut affected = Vec::new();
        for (id, _, _) in db.plugin_all_roots()? {
            for task in runtime.list(&id) {
                let resource = task["resourceKey"].as_str().unwrap_or("");
                if (resource == format!("next2:{provider}")
                    || (id == owner
                        && (resource.starts_with("next2:") || resource.starts_with("platform:"))))
                    && !terminal(&task)
                {
                    let task_id = task["taskId"].as_str().ok_or("INVALID_TASK")?.to_string();
                    runtime.cancel(&id, &task_id);
                    affected.push((id.clone(), task_id));
                }
            }
        }
        crate::diagnostics::record("backend-stop-cancel-requested", Some(owner));
        let deadline = Instant::now() + Duration::from_secs(5);
        for slot in slots {
            let mut was_busy = false;
            let mut worker = loop {
                match slot.worker.try_lock() {
                    Ok(worker) => break worker,
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        return Err("BACKEND_LOCK_FAILED".into())
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {
                        was_busy = true;
                        if Instant::now() >= deadline {
                            slot.stop.terminate()?;
                            break slot.worker.lock().map_err(|_| "BACKEND_LOCK_FAILED")?;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            };
            if let Some(worker) = worker.take() {
                if was_busy {
                    drop(worker);
                } else {
                    worker.stop()?;
                }
            }
            slot.stop.wait_empty()?;
            self.clear_session_resources(&slot.token)?;
        }
        let deadline =
            Instant::now() + Duration::from_millis((protocol::BUDGET_STOP_TIMEOUT_MS * 2) as u64);
        while affected
            .iter()
            .any(|(owner, id)| runtime.get(owner, id).is_some_and(|task| !terminal(&task)))
        {
            if Instant::now() >= deadline {
                return Err("BACKEND_TASK_STOP_NOT_CONFIRMED".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.files.clear(&provider)?;
        self.events.clear_plugin(&provider)?;
        self.streams.clear_plugin(&provider)?;
        self.file_operations.clear_owner(&provider)?;
        self.websockets.clear_owner(&provider)?;
        crate::diagnostics::record("backend-stop-complete", Some(owner));
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), String> {
        self.closing.store(true, Ordering::SeqCst);
        let owners: HashSet<_> = self
            .workers
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .keys()
            .map(|(owner, _)| owner.clone())
            .collect();
        let mut failures = Vec::new();
        let order = self.shutdown_order(&owners).unwrap_or_else(|failure| {
            failures.push(format!("Lifecycle graph: {failure}"));
            let mut owners: Vec<_> = owners.into_iter().collect();
            owners.sort();
            owners
        });
        for owner in order {
            if let Err(error) = self.stop(&owner) {
                failures.push(format!("{owner}: {error}"));
            }
        }
        if let Err(error) = self.websockets.clear() {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }
    fn shutdown_order(&self, owners: &HashSet<String>) -> Result<Vec<String>, String> {
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let names: HashMap<_, _> = db
            .plugin_all_roots()?
            .into_iter()
            .map(|(id, name, _)| (name, id))
            .collect();
        let registry = self.registry()?;
        let mut all: Vec<_> = names.keys().collect();
        all.sort_unstable();
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        for name in all {
            if let Ok(dependencies) = registry.activation_order(name) {
                for plugin in dependencies {
                    if let Some(owner) = names.get(&plugin.id) {
                        if owners.contains(owner) && visited.insert(owner.clone()) {
                            order.push(owner.clone());
                        }
                    }
                }
            }
        }
        order.reverse();
        let mut unresolved: Vec<_> = owners.difference(&visited).cloned().collect();
        unresolved.sort_unstable();
        order.extend(unresolved);
        Ok(order)
    }
    #[cfg(test)]
    pub(crate) fn has_backend_slot(&self, owner: &str) -> bool {
        self.workers
            .lock()
            .unwrap()
            .get(&(owner.to_owned(), InstanceKind::Service))
            .is_some_and(|instance| matches!(instance, Instance::Ready(_)))
    }
    #[cfg(test)]
    pub(crate) fn grant_fixture_file(&self, owner: &str, path: &Path) -> protocol::FileReference {
        self.files.grant(owner, path).unwrap()
    }
    /// Host command palette invokes only the method declared by this installation.
    pub fn execute_command(
        self: &Arc<Self>,
        owner: &str,
        command_id: &str,
    ) -> Result<Value, String> {
        let row = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_find_by_id(owner)?
            .filter(|row| row.enabled)
            .ok_or("SESSION_DENIED")?;
        let owner = row.name.as_str();
        let plugin = self.installed(&row)?;
        let command = plugin
            .manifest()
            .contributes
            .commands
            .iter()
            .find(|command| command.id == command_id)
            .ok_or("COMMAND_NOT_DECLARED")?;
        let raw = json!({"wireVersion":protocol::WIRE_VERSION,"requestId":crate::rand_token::random_token_hex()?,"session":crate::rand_token::random_token_hex()?,"target":{"kind":"backend"},"method":command.method,"params":{}}).to_string();
        let request = protocol::validate_request(&raw)?;
        let route = self.registry()?.resolve(owner, &request, "host")?;
        self.execute(owner, &request, route, Execution::default())
            .map_err(|failure| format!("{}: {}", failure.code, failure.message))
    }
    pub fn renderer(
        self: &Arc<Self>,
        raw: &str,
        metadata: &RendererSession,
    ) -> Result<Value, String> {
        let request = protocol::validate_request(raw)?;
        let result = (|| -> Result<Value, RpcError> {
            if metadata.renderer_api_version != protocol::SDK_API_VERSION
                || metadata.state != "active"
                || metadata.owner_webview_label != "main"
                || metadata.token != request.session
            {
                return Err(error("SESSION_DENIED".into()));
            }
            let row = self
                .db
                .lock()
                .map_err(|_| error("STORAGE_UNAVAILABLE".into()))?
                .plugin_find_by_id(&metadata.plugin_id)
                .map_err(error)?
                .filter(|row| row.enabled)
                .ok_or_else(|| error("SESSION_DENIED".into()))?;
            let plugin = self.installed(&row).map_err(error)?;
            if row.installed_path != metadata.plugin_directory
                || plugin.manifest().permissions != metadata.permissions
            {
                return Err(error("SESSION_REPLACED".into()));
            }
            let registry = if !matches!(request.target, protocol::Target::ServiceTarget(_)) {
                Registry::new([Installed {
                    manifest: plugin.manifest().clone(),
                    enabled: true,
                }])
            } else {
                self.registry()
            }
            .map_err(error)?;
            let session = {
                let mut sessions = self
                    .sessions
                    .lock()
                    .map_err(|_| error("INTERNAL_ERROR".into()))?;
                if !sessions.contains_key(&metadata.token)
                    && sessions.len() >= crate::plugin_session::MAX_RENDERER_SESSIONS
                {
                    return Err(error("BUSY".into()));
                }
                sessions
                    .entry(metadata.token.clone())
                    .or_insert_with(|| SessionEntry {
                        plugin_id: metadata.plugin_id.clone(),
                        session: Arc::new(Mutex::new(Session::renderer(
                            plugin.manifest().clone(),
                            metadata.token.clone(),
                            "main".into(),
                            metadata.expires_at_ms,
                        ))),
                    })
                    .session
                    .clone()
            };
            let admitted = {
                let mut admission = session.lock().map_err(|_| error("INTERNAL_ERROR".into()))?;
                admission.renew(metadata.expires_at_ms);
                admission
                    .admit(&registry, raw, "main", now())
                    .map_err(error)?
            };
            let result = self.execute(
                &admitted.owner,
                &admitted.request,
                admitted.route,
                Execution {
                    cancel: admitted.cancel.as_deref(),
                    ..Execution::default()
                },
            );
            session
                .lock()
                .map_err(|_| error("INTERNAL_ERROR".into()))?
                .finish(&request.request_id);
            result
        })();
        let response = match result {
            Ok(result) => {
                json!({"wireVersion":protocol::WIRE_VERSION,"requestId":request.request_id,"ok":true,"result":result})
            }
            Err(error) => {
                json!({"wireVersion":protocol::WIRE_VERSION,"requestId":request.request_id,"ok":false,"error":error})
            }
        };
        match protocol::validate_response(&response.to_string(), &request.request_id) {
            Ok(_) => Ok(response),
            Err(_) => Ok(
                json!({"wireVersion":protocol::WIRE_VERSION,"requestId":request.request_id,"ok":false,"error":{"code":"BUDGET_EXCEEDED","message":"Response exceeds wire budget; use a task or file result"}}),
            ),
        }
    }
    fn backend_capability(
        self: &Arc<Self>,
        session: &mut Session,
        request: &Request,
        mut call: BackendCall<'_>,
    ) -> Result<Value, RpcError> {
        let row = self.record(call.provider).map_err(error)?;
        self.check(&row).map_err(error)?;
        let registry = if matches!(request.target, protocol::Target::ServiceTarget(_)) {
            self.registry()
        } else {
            let db = self
                .db
                .lock()
                .map_err(|_| error("STORAGE_UNAVAILABLE".into()))?
                .clone();
            let manifest = crate::next2_install::load_manifest(&db, Path::new(&row.installed_path))
                .map_err(error)?;
            Self::verify_record(&manifest, &row.name, &row.version, &row.permissions)
                .map_err(error)?;
            Registry::new([Installed {
                manifest,
                enabled: true,
            }])
        }
        .map_err(error)?;
        let admitted = session
            .admit(
                &registry,
                &serde_json::to_string(request).map_err(|e| error(e.to_string()))?,
                "native",
                now(),
            )
            .map_err(error)?;
        let request = &admitted.request;
        let result = if admitted.route == Route::Platform
            && matches!(
                request.method.as_str(),
                "tasks.beginCommit" | "tasks.progress"
            ) {
            task_notification(request, &mut call)
        } else {
            self.execute(
                &admitted.owner,
                request,
                admitted.route,
                Execution {
                    caller: Some(call.execution.caller.unwrap_or(call.provider)),
                    ..call.execution
                },
            )
        };
        session.finish(&request.request_id);
        result
    }

    fn execute(
        self: &Arc<Self>,
        owner: &str,
        request: &Request,
        route: Route,
        mut execution: Execution<'_>,
    ) -> Result<Value, RpcError> {
        execution.check().map_err(error)?;
        match route {
            Route::Platform => self
                .services
                .platform
                .dispatch(self, owner, request, execution),
            Route::Backend {
                initiator,
                provider,
                provider_version,
                service,
            } => {
                let row = self.record(&provider).map_err(error)?;
                if row.version != provider_version {
                    return Err(error("PROVIDER_CHANGED".into()));
                }
                let task = execution.task;
                let kind = if task.is_some() {
                    InstanceKind::Task
                } else {
                    InstanceKind::Service
                };
                let slot = self.slot_for_record(&row, kind).map_err(error)?;
                let mut locked = slot.worker.try_lock().map_err(|_| error("BUSY".into()))?;
                let worker = locked
                    .as_mut()
                    .ok_or_else(|| error("BACKEND_STOPPED".into()))?;
                let mut session = slot
                    .admission
                    .lock()
                    .map_err(|_| error("INTERNAL_ERROR".into()))?;
                let cancelled = AtomicBool::new(false);
                let mut sequence = 0;
                if execution.deadline.is_none() && task.is_none() {
                    execution.deadline = Some(
                        Instant::now()
                            + Duration::from_millis(protocol::BUDGET_REQUEST_TIMEOUT_MS as u64),
                    );
                }
                let result = worker.call_with_limits(
                    &initiator,
                    &request.method,
                    request.params.clone(),
                    CallLimits {
                        task_id: task.map(|(ctx, _)| ctx.task_id()).as_deref(),
                        cancel: task
                            .map(|(ctx, _)| ctx.cancel_flag())
                            .or(execution.cancel)
                            .unwrap_or(&cancelled),
                        deadline: execution.deadline,
                    },
                    |capability| {
                        self.backend_capability(
                            &mut session,
                            capability,
                            BackendCall {
                                provider: &provider,
                                execution: Execution {
                                    caller: Some(&initiator),
                                    ..execution
                                },
                                sequence: &mut sequence,
                            },
                        )
                    },
                );
                drop(session);
                drop(locked);
                let result = result.and_then(|value| {
                    if let Some(service) = &service {
                        service.validate_result(&value).map_err(error)?;
                    }
                    Ok(value)
                });
                if let Some((ctx, _)) = task {
                    let outcome = result.clone().map_err(|failure| {
                        if matches!(
                            failure.code.as_str(),
                            "NATIVE_TRANSPORT"
                                | "NATIVE_STOP_FAILED"
                                | "BACKEND_STOPPED"
                                | "CANCELLED"
                                | "TASK_TIMEOUT"
                                | "RPC_TIMEOUT"
                        ) {
                            format!("EXECUTION_OUTCOME_UNKNOWN: {}", failure.message)
                        } else {
                            failure.message.clone()
                        }
                    });
                    ctx.finish_external_commit(&outcome).map_err(error)?;
                }
                if result.as_ref().is_err_and(|error| {
                    matches!(
                        error.code.as_str(),
                        "CANCELLED"
                            | "NATIVE_TRANSPORT"
                            | "NATIVE_STOP_FAILED"
                            | "TASK_TIMEOUT"
                            | "RPC_TIMEOUT"
                    )
                }) {
                    self.retire(&row.id, kind, &slot).map_err(error)?;
                    if kind == InstanceKind::Service {
                        if let Err(failure) = &result {
                            self.lifecycle_failure(&row.id, &failure.code);
                        }
                    }
                }
                result
            }
        }
    }
    fn choose_files(
        &self,
        row: &crate::db::PluginRow,
        params: &Value,
        mut execution: Execution<'_>,
    ) -> Result<Value, String> {
        use tauri_plugin_dialog::DialogExt;
        let app = self
            .services
            .platform
            .app_handle
            .as_ref()
            .ok_or("APPLICATION_UNAVAILABLE")?;
        let mut dialog = app.dialog().file();
        let extensions: Vec<_> = params["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !extensions.is_empty() {
            dialog = dialog.add_filter("文件", &extensions);
        }
        let multiple = params["multiple"].as_bool() == Some(true);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        match (params["type"].as_str().unwrap(), multiple) {
            ("file", true) => dialog.pick_files(move |paths| {
                let _ = send.send(paths);
            }),
            ("folder", true) => dialog.pick_folders(move |paths| {
                let _ = send.send(paths);
            }),
            ("file", false) => dialog.pick_file(move |path| {
                let _ = send.send(path.map(|path| vec![path]));
            }),
            _ => dialog.pick_folder(move |path| {
                let _ = send.send(path.map(|path| vec![path]));
            }),
        }
        execution.deadline.get_or_insert_with(|| {
            Instant::now() + Duration::from_millis(protocol::BUDGET_INTERACTIVE_TIMEOUT_MS as u64)
        });
        let paths = loop {
            execution.check()?;
            self.check(row)?;
            match receive.recv_timeout(Duration::from_millis(100)) {
                Ok(paths) => break paths,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("FILE_DIALOG_DISCONNECTED".into())
                }
            }
        };
        execution.check()?;
        self.check(row)?;
        let Some(paths) = paths else {
            return Ok(Value::Null);
        };
        if paths.len() > 128 {
            return Err("FILE_SELECTION_LIMIT".into());
        }
        let references: Result<Vec<_>, _> = paths
            .into_iter()
            .map(|path| {
                let path = path.into_path().map_err(|e| e.to_string())?;
                self.files.grant(&row.name, &path)
            })
            .collect();
        serde_json::to_value(references?).map_err(|e| e.to_string())
    }
    fn resolve_file(
        &self,
        owner: &str,
        caller: Option<&str>,
        reference: &str,
    ) -> Result<PathBuf, String> {
        match self.files.resolve(owner, caller, reference) {
            Ok(path) => Ok(path),
            Err(error) if error == "FILE_REFERENCE_EXPIRED" => {
                let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
                let raw = db
                    .setting_get(&format!("next2.file.{reference}"))
                    .map_err(|e| e.to_string())?
                    .ok_or(error)?;
                let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                if value["owner"] != owner && caller.is_none_or(|caller| value["owner"] != caller) {
                    return Err("FILE_AUTHORITY_DENIED".into());
                }
                Ok(PathBuf::from(
                    value["path"].as_str().ok_or("INVALID_FILE_RECORD")?,
                ))
            }
            Err(error) => Err(error),
        }
    }
    fn publish_file(
        &self,
        owner: &str,
        params: &Value,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        use std::io::{Read, Write};
        let (ctx, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
        if provider != owner || params["taskId"] != ctx.task_id() {
            return Err("TASK_AUTHORITY_DENIED".into());
        }
        let row = self.record(owner)?;
        let source_root = Self::data(&row)?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let source = Path::new(params["path"].as_str().unwrap())
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !source.starts_with(&source_root) || !source.is_file() {
            return Err("RESULT_MUST_BE_IN_PLUGIN_DATA".into());
        }
        self.write_output(
            owner,
            params["name"].as_str().unwrap(),
            execution,
            |stage, _| {
                let mut input = crate::next2_files::open_authorized(&source)?;
                let mut output = std::fs::File::create(stage).map_err(|e| e.to_string())?;
                let size = input.metadata().map_err(|e| e.to_string())?.len();
                if size > 16 * 1024 * 1024 * 1024 {
                    return Err("FILE_BUDGET_EXCEEDED".into());
                }
                let mut bytes = 0;
                let mut buffer = [0u8; 65536];
                loop {
                    execution.check()?;
                    let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
                    if count == 0 {
                        break;
                    }
                    bytes += count as u64;
                    if bytes > size {
                        return Err("RESULT_CHANGED_DURING_COPY".into());
                    }
                    output
                        .write_all(&buffer[..count])
                        .map_err(|e| e.to_string())?;
                }
                if bytes != size {
                    return Err("RESULT_CHANGED_DURING_COPY".into());
                }
                output.sync_all().map_err(|e| e.to_string())?;
                Ok(())
            },
        )
    }

    fn write_output(
        &self,
        owner: &str,
        name: &str,
        execution: Execution<'_>,
        write: impl FnOnce(&Path, &crate::task_runtime::Context) -> Result<(), String>,
    ) -> Result<Value, String> {
        let (ctx, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
        if provider != owner {
            return Err("TASK_AUTHORITY_DENIED".into());
        }
        if name.ends_with(['.', ' ']) || name.contains(['/', '\\', ':']) {
            return Err("INVALID_FILE_NAME".into());
        }
        let recipient = execution.caller.unwrap_or(owner);
        let data = Self::writable_data(&self.record(recipient)?)?;
        let results = managed_child(&data, "results")?;
        let output = managed_child(&results, &ctx.task_id())?.join(name);
        let transaction = crate::output_transaction::OutputTransaction::new(&output, false)?;
        write(transaction.stage_path(), ctx)?;
        execution.check()?;
        let published = transaction.publish_durable(ctx, false, |_| Ok(()))?;
        let reference = self.files.grant(recipient, &published)?;
        self.persist_file_reference(recipient, &published, reference)
    }
    fn remember_file(&self, owner: &str, path: &Path) -> Result<Value, String> {
        self.persist_file_reference(owner, path, self.files.grant(owner, path)?)
    }
    fn persist_file_reference(
        &self,
        recipient: &str,
        published: &Path,
        reference: protocol::FileReference,
    ) -> Result<Value, String> {
        self.db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .setting_set(
                &format!("next2.file.{}", reference.r#ref),
                &json!({"owner":recipient,"path":published}).to_string(),
            )
            .map_err(|e| e.to_string())?;
        serde_json::to_value(reference).map_err(|e| e.to_string())
    }
    fn task(
        &self,
        initiator: &str,
        owner: &str,
        id: &str,
        require_result: bool,
    ) -> Result<Option<Value>, String> {
        let Some(mut value) = self.services.runtime.get(owner, id) else {
            return Ok(None);
        };
        if terminal(&value) {
            restore_task_outcome(
                &*self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?,
                owner,
                id,
                &mut value,
                require_result,
            )?;
        }
        if require_result {
            if let Some(message) = value["executionError"].as_str() {
                return Err(format!("任务输出已提交，但执行收尾失败：{message}"));
            }
        }
        let provider = value["resourceKey"]
            .as_str()
            .and_then(|value| {
                value
                    .strip_prefix("next2:")
                    .or_else(|| value.strip_prefix("platform:").map(|_| "cruciblebox-host"))
            })
            .ok_or("INVALID_TASK_RECORD")?
            .to_owned();
        value["initiatorId"] = json!(initiator);
        value["providerId"] = json!(provider);
        Ok(Some(value))
    }
}

fn task_notification(request: &Request, call: &mut BackendCall<'_>) -> Result<Value, RpcError> {
    let (ctx, expected_provider) = call
        .execution
        .task
        .ok_or_else(|| error("TASK_EXECUTION_REQUIRED".into()))?;
    if request.params["taskId"] != ctx.task_id() || expected_provider != call.provider {
        return Err(error("TASK_AUTHORITY_DENIED".into()));
    }
    if request.method == "tasks.beginCommit" {
        return ctx
            .begin_external_commit()
            .map(|()| json!(true))
            .map_err(error);
    }
    let sequence = request.params["sequence"].as_u64().unwrap();
    if sequence <= *call.sequence {
        return Err(error("TASK_AUTHORITY_DENIED".into()));
    }
    *call.sequence = sequence;
    ctx.update_progress(
        request.params["stage"].as_str().unwrap(),
        request.params["percent"].as_u64().unwrap() as u32,
        request.params["message"].as_str().unwrap(),
        None,
    );
    Ok(json!(true))
}
// Persist only executor outcomes, never the task request or credentials. Core task
// status remains authoritative; a saved outcome cannot promote an interrupted task.
fn save_task_outcome(
    db: &Db,
    owner: &str,
    id: &str,
    created: u64,
    result: &Result<Value, String>,
) -> Result<(), String> {
    let mut value = json!({"owner":owner,"createdAt":created,"completedAt":now()});
    match result {
        Ok(result) => value["result"] = result.clone(),
        Err(error) => value["error"] = json!(error),
    }
    cruciblebox_repository::task_projection::save_outcome(db, id, &value)
}
fn restore_task_outcome(
    db: &Db,
    owner: &str,
    id: &str,
    value: &mut Value,
    require_result: bool,
) -> Result<(), String> {
    let stored = db
        .setting_get(&format!("next2.task-result.{id}"))
        .map_err(|e| e.to_string())?;
    let Some(raw) = stored else {
        // Publication can precede the durable executor outcome. Keep the
        // existing pending-completion sentinel until that outcome is available.
        if value["completedAt"].as_u64().is_some_and(|time| time > 0) {
            if value["status"] == "succeeded" && value.get("executionError").is_none() {
                value["completedAt"] = json!(0);
            }
            return Ok(());
        }
        value["resultUnavailable"] = json!(true);
        if let Some(previous) = cruciblebox_repository::task_projection::get(db, id)? {
            if previous.owner != owner {
                return Err("TASK_AUTHORITY_DENIED".into());
            }
            value["createdAt"] = json!(previous.created_at);
            value["completedAt"] = json!(previous.updated_at);
        }
        if require_result
            && (value["status"] == "succeeded" || value["completedAt"].as_u64().unwrap_or(0) == 0)
        {
            return Err(
                "历史任务未保存完整结果，请在任务中心核对已有输出；任务没有重新执行".into(),
            );
        }
        return Ok(());
    };
    let saved: Value = serde_json::from_str(&raw).map_err(|_| "INVALID_TASK_RESULT_RECORD")?;
    if saved["owner"] != owner {
        return Err("TASK_AUTHORITY_DENIED".into());
    }
    if saved["createdAt"].as_u64().filter(|n| *n > 0).is_none()
        || saved["completedAt"].as_u64().filter(|n| *n > 0).is_none()
    {
        return Err("INVALID_TASK_RESULT_RECORD".into());
    }
    value["createdAt"] = saved["createdAt"].clone();
    value["completedAt"] = saved["completedAt"].clone();
    if value["status"] == "succeeded" {
        if let Some(result) = saved.get("result") {
            value["result"] = result.clone();
        } else {
            value["executionError"] = saved["error"].clone();
            if require_result {
                return Err("任务输出已提交，但执行收尾失败；请核对任务中心已有输出".into());
            }
        }
    }
    Ok(())
}
fn terminal(value: &Value) -> bool {
    matches!(
        value["status"].as_str(),
        Some("succeeded" | "failed" | "cancelled" | "interrupted")
    )
}

#[cfg(test)]
mod task_outcome_tests {
    use super::*;
    fn directory() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "cb-task-outcome-{}",
            crate::rand_token::random_token_hex().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        root
    }
    #[test]
    fn public_completion_time_is_identical_before_and_after_runtime_reopen() {
        let root = directory();
        let db = Arc::new(Mutex::new(Db::open(&root.join("data.db")).unwrap()));
        let path = root.join("tasks.sqlite");
        let runtime = crate::task_runtime::TaskRuntime::open(&path).unwrap();
        let task_runtime = runtime.clone();
        let task_db = db.clone();
        let (published, publication) = std::sync::mpsc::channel();
        let (resume, resumed) = std::sync::mpsc::channel();
        let id = runtime
            .start(
                "sample-alpha",
                "next2:sample-alpha",
                Box::new(move |ctx| {
                    let created = task_runtime.get("sample-alpha", &ctx.task_id()).unwrap()
                        ["createdAt"]
                        .as_u64()
                        .unwrap();
                    let result = Ok(json!({"answer":42}));
                    ctx.begin_external_commit()?;
                    ctx.finish_external_commit(&result)?;
                    published.send(()).map_err(|error| error.to_string())?;
                    resumed.recv().map_err(|error| error.to_string())?;
                    save_task_outcome(
                        &task_db.lock().unwrap(),
                        "sample-alpha",
                        &ctx.task_id(),
                        created,
                        &result,
                    )?;
                    // Executor cleanup occurs after its durable outcome is written.
                    std::thread::sleep(Duration::from_millis(30));
                    result
                }),
            )
            .unwrap();
        publication.recv_timeout(Duration::from_secs(10)).unwrap();
        let manager = Manager::new(
            db.clone(),
            Arc::new(crate::host_services::Services::new(runtime.clone())),
            Arc::default(),
            PathBuf::new(),
        );
        let published_time = runtime.get("sample-alpha", &id).unwrap()["completedAt"].clone();
        assert_eq!(
            manager
                .task("sample-alpha", "sample-alpha", &id, true)
                .unwrap()
                .unwrap()["completedAt"],
            0
        );
        resume.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while runtime.get("sample-alpha", &id).unwrap()["completedAt"] == published_time {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let before = manager
            .task("sample-alpha", "sample-alpha", &id, true)
            .unwrap()
            .unwrap();
        assert_eq!(before["result"], json!({"answer":42}));
        drop(manager);
        drop(runtime);
        let reopened = crate::task_runtime::TaskRuntime::open(&path).unwrap();
        let manager = Manager::new(
            db,
            Arc::new(crate::host_services::Services::new(reopened)),
            Arc::default(),
            PathBuf::new(),
        );
        let after = manager
            .task("sample-alpha", "sample-alpha", &id, true)
            .unwrap()
            .unwrap();
        assert_eq!(before["completedAt"], after["completedAt"]);
        assert_eq!(before["createdAt"], after["createdAt"]);
        assert_eq!(before["result"], after["result"]);
        drop(manager);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn full_outcome_survives_database_reopen_and_keeps_owner() {
        let root = directory();
        let path = root.join("data.db");
        let result = json!({"outputPath":"C:/synthetic/output.pdf","files":[{"path":"C:/synthetic/output.pdf"}],"quality":{"mode":"synthetic"}});
        {
            let db = Db::open(&path).unwrap();
            save_task_outcome(&db, "sample-alpha", "task-1", 123, &Ok(result.clone())).unwrap();
        }
        let db = Db::open(&path).unwrap();
        let mut recovered = json!({"status":"succeeded","completedAt":0});
        restore_task_outcome(&db, "sample-alpha", "task-1", &mut recovered, true).unwrap();
        assert_eq!(recovered["createdAt"], 123);
        assert!(recovered["completedAt"].as_u64().unwrap() > 0);
        assert_eq!(recovered["result"], result);
        assert_eq!(
            restore_task_outcome(
                &db,
                "sample-beta",
                "task-1",
                &mut json!({"status":"succeeded"}),
                true
            )
            .unwrap_err(),
            "TASK_AUTHORITY_DENIED"
        );
        let mut interrupted = json!({"status":"interrupted"});
        restore_task_outcome(&db, "sample-alpha", "task-1", &mut interrupted, true).unwrap();
        assert_eq!(interrupted["status"], "interrupted");
        assert!(interrupted.get("result").is_none());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_result_is_explicitly_unavailable_without_changing_status_or_outputs() {
        let root = directory();
        let db = Db::open(&root.join("data.db")).unwrap();
        let mut old = json!({"status":"succeeded","resultRefs":["preserved.pdf"]});
        assert!(restore_task_outcome(&db, "sample-alpha", "old-task", &mut old, true).is_err());
        assert_eq!(old["resultUnavailable"], true);
        assert_eq!(old["status"], "succeeded");
        assert_eq!(old["resultRefs"], json!(["preserved.pdf"]));
        restore_task_outcome(&db, "sample-alpha", "old-task", &mut old, false).unwrap();
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}

//! Manifest 6 admission and generic backend routing. Domain methods stay in plugins.
pub(crate) mod platform_service;
use crate::{db::Db, plugin_session::RendererSession};
use cruciblebox_next2_protocol::{
    self as protocol,
    gateway::{Installed, Registry, Route, Session},
    Backend, Request, RpcError,
};
use cruciblebox_next2_runtime::{
    package::ApprovedPlugin,
    worker::{StopHandle, Worker},
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}
fn error(message: String) -> RpcError {
    RpcError {
        code: if message.len() <= 100 && !message.contains(' ') {
            message.clone()
        } else {
            "PLATFORM_ERROR".into()
        },
        message: message.chars().take(2048).collect(),
    }
}
struct Slot {
    worker: Mutex<Option<Worker>>,
    stop: StopHandle,
    admission: Mutex<Session>,
}
struct SessionEntry {
    plugin_id: String,
    session: Arc<Mutex<Session>>,
}
pub struct Manager {
    runner: PathBuf,
    db: Arc<Mutex<Db>>,
    services: Arc<crate::host_services::Services>,
    maintenance: Arc<Mutex<HashSet<String>>>,
    workers: Mutex<HashMap<(String, bool), Arc<Slot>>>,
    sessions: Mutex<HashMap<String, SessionEntry>>,
    closing: AtomicBool,
    files: crate::next2_files::Files,
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
            sessions: Mutex::new(HashMap::new()),
            closing: AtomicBool::new(false),
            files: crate::next2_files::Files::default(),
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
            .enabled_plugin_backend_records()
            .map_err(|e| e.to_string())?;
        let mut installed = Vec::new();
        for (_, row) in records {
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
                    enabled: true,
                });
            }
        }
        if let Some(manifest) = replacement {
            installed.push(Installed {
                manifest: manifest.clone(),
                enabled: true,
            });
        }
        Registry::new(installed)
    }
    pub fn dispose_renderer_session(&self, token: &str) -> Result<(), String> {
        self.sessions
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .remove(token);
        Ok(())
    }
    pub fn dispose_plugin_renderers(&self, plugin_id: &str) -> Result<(), String> {
        self.sessions
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .retain(|_, entry| entry.plugin_id != plugin_id);
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
    pub fn activate(&self, owner: &str) -> Result<(), String> {
        let row = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_find_by_id(owner)?
            .filter(|row| row.enabled)
            .ok_or("SESSION_DENIED")?;
        let plugin = self.installed(&row)?;
        self.registry()?.validate_plugin(&row.name)?;
        if plugin.manifest().backend.is_some() {
            self.slot(&row.name, false)?;
        }
        Ok(())
    }
    /// Restore enabled background services after install recovery. Reuse each shared
    /// dependency's startup outcome for this pass, and leave enabled choices intact.
    pub fn restore_background(&self) -> Result<(), String> {
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
                        .or_insert_with(|| self.slot(&plugin.id, false).map(|_| ()))
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
    pub fn enable(&self, owner: &str) -> Result<(), String> {
        let row = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_find_by_id(owner)?
            .ok_or("PLUGIN_NOT_FOUND")?;
        let plugin = self.installed(&row)?;
        self.validate_graph_change(plugin.manifest())?;
        if plugin.manifest().backend.is_some() {
            self.slot_for_record(&row, false)?;
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
    fn slot(&self, provider: &str, task: bool) -> Result<Arc<Slot>, String> {
        let row = self.record(provider)?;
        self.slot_for_record(&row, task)
    }
    fn slot_for_record(&self, row: &crate::db::PluginRow, task: bool) -> Result<Arc<Slot>, String> {
        self.check(row)?;
        let mut workers = self.workers.lock().map_err(|_| "INTERNAL_ERROR")?;
        let key = (row.id.clone(), task);
        if let Some(slot) = workers.get(&key) {
            self.installed(row)?;
            return Ok(slot.clone());
        }
        if workers.len() >= 16 {
            return Err("BUSY".into());
        }
        let plugin = self.installed(row)?;
        let resources = self.mounts(row, &plugin)?;
        let worker = match plugin.manifest().backend {
            Some(Backend::NativeBackend(_)) => {
                Worker::start(&plugin, &Self::data(row)?, &resources)?
            }
            Some(Backend::JsBackend(_)) => {
                Worker::start_js(&plugin, &Self::data(row)?, &resources, &self.runner)?
            }
            None => return Err("BACKEND_UNAVAILABLE".into()),
        };
        let slot = Arc::new(Slot {
            stop: worker.stop_handle(),
            admission: Mutex::new(Session::new(
                plugin.manifest().clone(),
                worker.session().into(),
                "native".into(),
                "backend".into(),
                u64::MAX,
            )),
            worker: Mutex::new(Some(worker)),
        });
        // Maintenance marks the owner before taking this pool lock; a racing
        // startup must finish and be stopped before the installer can swap files.
        self.check(row)?;
        workers.insert(key, slot.clone());
        Ok(slot)
    }
    fn data(row: &crate::db::PluginRow) -> Result<PathBuf, String> {
        Ok(
            crate::next2_install::data_root(Path::new(&row.installed_path))?
                .join("plugin-data")
                .join(&row.name),
        )
    }
    fn mounts(
        &self,
        row: &crate::db::PluginRow,
        plugin: &ApprovedPlugin,
    ) -> Result<Vec<protocol::ResourceMount>, String> {
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let store = Self::data(row)?
            .parent()
            .and_then(Path::parent)
            .ok_or("INVALID_DATA_PATH")?
            .join("plugin-resources");
        cruciblebox_next2_runtime::resource::resolve(
            &store,
            plugin,
            &crate::next2_install::keys(&db)?,
            &crate::next2_install::resource_approvals(&db, &plugin.manifest().id)?,
        )
    }
    pub fn stop(&self, owner: &str) -> Result<(), String> {
        crate::diagnostics::record("backend-stop-start", Some(owner));
        let slots: Vec<_> = {
            let mut workers = self.workers.lock().map_err(|_| "INTERNAL_ERROR")?;
            [false, true]
                .into_iter()
                .filter_map(|task| workers.remove(&(owner.into(), task)))
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
                    || (id == owner && resource.starts_with("next2:")))
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
        for owner in owners {
            if let Err(error) = self.stop(&owner) {
                failures.push(format!("{owner}: {error}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }
    #[cfg(test)]
    pub(crate) fn has_backend_slot(&self, owner: &str) -> bool {
        self.workers
            .lock()
            .unwrap()
            .contains_key(&(owner.to_owned(), false))
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
        let raw = json!({"wireVersion":4,"requestId":crate::rand_token::random_token_hex()?,"session":crate::rand_token::random_token_hex()?,"target":{"kind":"backend"},"method":command.method,"params":{}}).to_string();
        let request = protocol::validate_request(&raw)?;
        let route = self
            .registry()?
            .resolve(owner, &request.target, &request.method)?;
        self.execute(owner, &request, route, None, None)
            .map_err(|failure| format!("{}: {}", failure.code, failure.message))
    }
    pub fn renderer(
        self: &Arc<Self>,
        raw: &str,
        metadata: &RendererSession,
    ) -> Result<Value, String> {
        let request = protocol::validate_request(raw)?;
        let result = (|| -> Result<Value, RpcError> {
            if metadata.renderer_api_version != 6
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
            let result = self.execute(&admitted.owner, &request, admitted.route, None, None);
            session
                .lock()
                .map_err(|_| error("INTERNAL_ERROR".into()))?
                .finish(&request.request_id);
            result
        })();
        let response = match result {
            Ok(result) => {
                json!({"wireVersion":4,"requestId":request.request_id,"ok":true,"result":result})
            }
            Err(error) => {
                json!({"wireVersion":4,"requestId":request.request_id,"ok":false,"error":error})
            }
        };
        match protocol::validate_response(&response.to_string(), &request.request_id) {
            Ok(_) => Ok(response),
            Err(_) => Ok(
                json!({"wireVersion":4,"requestId":request.request_id,"ok":false,"error":{"code":"BUDGET_EXCEEDED","message":"Response exceeds wire budget; use a task or file result"}}),
            ),
        }
    }
    fn execute(
        self: &Arc<Self>,
        owner: &str,
        request: &Request,
        route: Route,
        task: Option<(&crate::task_runtime::Context, &str)>,
        file_caller: Option<&str>,
    ) -> Result<Value, RpcError> {
        match route {
            Route::Platform => {
                self.services
                    .platform
                    .dispatch(self, owner, request, task, file_caller)
            }
            Route::Backend {
                initiator,
                provider,
                ..
            } => {
                let slot = self.slot(&provider, task.is_some()).map_err(error)?;
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
                let result = worker.call(
                    &initiator,
                    &request.method,
                    request.params.clone(),
                    task.map(|(ctx, _)| ctx.task_id()).as_deref(),
                    task.map_or(&cancelled, |(ctx, _)| ctx.cancel_flag()),
                    |capability| {
                        let registry =
                            if matches!(capability.target, protocol::Target::ServiceTarget(_)) {
                                self.registry()
                            } else {
                                let row = self.record(&provider).map_err(error)?;
                                self.check(&row).map_err(error)?;
                                let db = self
                                    .db
                                    .lock()
                                    .map_err(|_| error("STORAGE_UNAVAILABLE".into()))?
                                    .clone();
                                let manifest = crate::next2_install::load_manifest(
                                    &db,
                                    Path::new(&row.installed_path),
                                )
                                .map_err(error)?;
                                Self::verify_record(
                                    &manifest,
                                    &row.name,
                                    &row.version,
                                    &row.permissions,
                                )
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
                                &serde_json::to_string(capability)
                                    .map_err(|e| error(e.to_string()))?,
                                "native",
                                now(),
                            )
                            .map_err(error)?;
                        let result = if admitted.route == Route::Platform
                            && capability.method == "tasks.beginCommit"
                        {
                            let (ctx, expected_provider) =
                                task.ok_or_else(|| error("TASK_EXECUTION_REQUIRED".into()))?;
                            if capability.params["taskId"] != ctx.task_id()
                                || expected_provider != provider
                            {
                                return Err(error("TASK_AUTHORITY_DENIED".into()));
                            }
                            ctx.begin_external_commit()
                                .map(|()| json!(true))
                                .map_err(error)
                        } else if admitted.route == Route::Platform
                            && capability.method == "tasks.progress"
                        {
                            let (ctx, expected_provider) =
                                task.ok_or_else(|| error("TASK_EXECUTION_REQUIRED".into()))?;
                            let next = capability.params["sequence"].as_u64().unwrap();
                            if capability.params["taskId"] != ctx.task_id()
                                || next <= sequence
                                || expected_provider != provider
                            {
                                return Err(error("TASK_AUTHORITY_DENIED".into()));
                            }
                            sequence = next;
                            ctx.update_progress(
                                capability.params["stage"].as_str().unwrap(),
                                capability.params["percent"].as_u64().unwrap() as u32,
                                capability.params["message"].as_str().unwrap(),
                                None,
                            );
                            Ok(json!(true))
                        } else {
                            self.execute(
                                &admitted.owner,
                                capability,
                                admitted.route,
                                task,
                                Some(&initiator),
                            )
                        };
                        session.finish(&capability.request_id);
                        result
                    },
                );
                drop(session);
                drop(locked);
                if let Some((ctx, _)) = task {
                    let outcome = result.clone().map_err(|failure| {
                        if matches!(
                            failure.code.as_str(),
                            "NATIVE_TRANSPORT"
                                | "NATIVE_STOP_FAILED"
                                | "BACKEND_STOPPED"
                                | "CANCELLED"
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
                        "CANCELLED" | "NATIVE_TRANSPORT" | "NATIVE_STOP_FAILED"
                    )
                }) {
                    let row = self.record(&provider).map_err(error)?;
                    let mut workers = self
                        .workers
                        .lock()
                        .map_err(|_| error("INTERNAL_ERROR".into()))?;
                    let key = (row.id, task.is_some());
                    if workers
                        .get(&key)
                        .is_some_and(|current| Arc::ptr_eq(current, &slot))
                    {
                        workers.remove(&key);
                    }
                }
                result
            }
        }
    }
    fn choose_files(&self, owner: &str, params: &Value) -> Result<Value, String> {
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
        let paths = match (params["type"].as_str().unwrap(), multiple) {
            ("file", true) => dialog.blocking_pick_files(),
            ("folder", true) => dialog.blocking_pick_folders(),
            ("file", false) => dialog.blocking_pick_file().map(|path| vec![path]),
            _ => dialog.blocking_pick_folder().map(|path| vec![path]),
        };
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
                self.files.grant(owner, &path)
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
        caller: Option<&str>,
        params: &Value,
        task: Option<(&crate::task_runtime::Context, &str)>,
    ) -> Result<Value, String> {
        use std::io::{Read, Write};
        let (ctx, provider) = task.ok_or("TASK_EXECUTION_REQUIRED")?;
        if provider != owner || params["taskId"] != ctx.task_id() {
            return Err("TASK_AUTHORITY_DENIED".into());
        }
        let name = params["name"].as_str().unwrap();
        if name == "." || name == ".." || name.contains(['/', '\\', ':']) {
            return Err("INVALID_FILE_NAME".into());
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
        let recipient = caller.unwrap_or(owner);
        let output = Self::data(&self.record(recipient)?)?
            .join("results")
            .join(ctx.task_id())
            .join(name);
        let transaction = crate::output_transaction::OutputTransaction::new(&output, false)?;
        let mut input = std::fs::File::open(&source).map_err(|e| e.to_string())?;
        let mut output =
            std::fs::File::create(transaction.stage_path()).map_err(|e| e.to_string())?;
        let size = input.metadata().map_err(|e| e.to_string())?.len();
        if size > 16 * 1024 * 1024 * 1024 {
            return Err("FILE_BUDGET_EXCEEDED".into());
        }
        let mut bytes = 0;
        let mut buffer = [0u8; 65536];
        loop {
            ctx.check_cancelled()?;
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
        drop(output);
        let published = transaction.publish_durable(ctx, false, |_| Ok(()))?;
        let reference = self.files.grant(recipient, &published)?;
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
        if terminal(&value) && value["completedAt"].as_u64().unwrap_or(0) == 0 {
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
            .and_then(|value| value.strip_prefix("next2:"))
            .ok_or("INVALID_TASK_RECORD")?
            .to_owned();
        value["initiatorId"] = json!(initiator);
        value["providerId"] = json!(provider);
        Ok(Some(value))
    }
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

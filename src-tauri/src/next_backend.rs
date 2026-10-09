//! Next worker supervisor. Bounded per-worker frame queue, no long DB lock.
use crate::db::Db;
use cruciblebox_next_protocol::{
    self as protocol, gateway::Session, runtime::dispatch_response, transport,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |v| v.as_millis() as u64)
}
struct Worker {
    child: Arc<Mutex<Child>>,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Vec<u8>, String>>,
    token: String,
    session: Session,
    owner: String,
    permissions: String,
    sequence: u64,
    db: Arc<Mutex<Db>>,
    services: Arc<crate::host_services::Services>,
}
impl Worker {
    fn start(
        owner: &str,
        db: Arc<Mutex<Db>>,
        executable: &PathBuf,
        services: Arc<crate::host_services::Services>,
    ) -> Result<Self, String> {
        let record = db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_backend_record(owner)
            .map_err(|e| e.to_string())?
            .ok_or("SESSION_DENIED")?;
        if !record.enabled {
            return Err("SESSION_DENIED".into());
        }
        let directory = PathBuf::from(&record.installed_path);
        let manifest = crate::manifest::read_manifest(&directory)?;
        if manifest.manifest_version != Some(5)
            || manifest.backend != Some(true)
            || manifest.name != record.name
            || manifest.main != record.entry_main
        {
            return Err("SESSION_DENIED".into());
        }
        crate::manifest::validate_entrypoints(&directory, &manifest)?;
        let permissions: Vec<String> =
            serde_json::from_str(&record.permissions).map_err(|_| "SESSION_DENIED")?;
        if permissions != manifest.permissions {
            return Err("SESSION_DENIED".into());
        }
        let token = crate::rand_token::random_token_hex()?;
        let mut command = Command::new(executable);
        command
            .args([
                directory.to_string_lossy().as_ref(),
                &record.entry_main,
                "3",
                &token,
            ])
            .current_dir(&directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Next worker missing/start failed: {e}"))?;
        let input = child.stdin.take().ok_or("INTERNAL_ERROR")?;
        let mut output = child.stdout.take().ok_or("INTERNAL_ERROR")?;
        let (sender, receiver) = mpsc::sync_channel(protocol::BACKEND_QUEUE);
        std::thread::Builder::new()
            .name(format!("next-worker-{owner}"))
            .spawn(move || loop {
                let value = transport::read_frame(&mut output)
                    .map_err(|e| e.to_string())
                    .and_then(|v| v.ok_or("Next worker EOF".into()));
                let failed = value.is_err();
                if sender.send(value).is_err() || failed {
                    break;
                }
            })
            .map_err(|e| e.to_string())?;
        let session = Session::new(
            token.clone(),
            format!("backend:{token}"),
            owner.into(),
            now_ms() + protocol::LEASE_MS,
            permissions,
        );
        let mut worker = Self {
            child: Arc::new(Mutex::new(child)),
            input,
            output: receiver,
            token,
            session,
            owner: owner.into(),
            permissions: record.permissions,
            sequence: 0,
            db,
            services,
        };
        let handshake = worker.control("activate", json!({}))?;
        if handshake["contractSha256"] != protocol::CONTRACT_SHA256
            || handshake["sdkApiVersion"] != 5
            || handshake["wireVersion"] != 3
        {
            return Err("Next worker contract mismatch".into());
        }
        Ok(worker)
    }
    fn control(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let current = self
            .db
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .plugin_backend_record(&self.owner)
            .map_err(|e| e.to_string())?
            .ok_or("SESSION_DENIED")?;
        if !current.enabled || current.permissions != self.permissions {
            return Err("SESSION_DENIED".into());
        }
        self.sequence += 1;
        let id = format!("worker-{}", self.sequence);
        let frame = json!({"kind":"control","wireVersion":3,"token":self.token,"requestId":id,"method":method,"params":params});
        transport::validate_frame(&frame.to_string(), &self.token).map_err(str::to_owned)?;
        transport::write_frame(&mut self.input, &frame)?;
        let deadline = Instant::now() + Duration::from_millis(protocol::BACKEND_TIMEOUT_MS);
        loop {
            let bytes = self
                .output
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "TIMEOUT")??;
            let frame = transport::validate_frame(
                std::str::from_utf8(&bytes).map_err(|_| "INVALID_RESPONSE")?,
                &self.token,
            )
            .map_err(str::to_owned)?;
            match frame["kind"].as_str() {
                Some("capability") => {
                    let raw = frame["request"].to_string();
                    // Short storage operations only. This guard is dropped before waiting for worker output.
                    let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
                    let current = db
                        .plugin_backend_record(&self.owner)
                        .map_err(|e| e.to_string())?
                        .ok_or("SESSION_DENIED")?;
                    if !current.enabled || current.permissions != self.permissions {
                        return Err("SESSION_DENIED".into());
                    }
                    let adapter = Adapter {
                        db: self.db.clone(),
                        backend: None,
                        services: self.services.clone(),
                    };
                    let response = dispatch_response(
                        &mut self.session,
                        &adapter,
                        &raw,
                        &format!("backend:{}", self.token),
                        now_ms(),
                    )?;
                    drop(db);
                    transport::write_frame(
                        &mut self.input,
                        &json!({"kind":"capability-result","response":response}),
                    )?;
                }
                Some("result") => {
                    let response = protocol::validate_response(&frame["response"].to_string(), &id)
                        .map_err(str::to_owned)?;
                    return match response {
                        protocol::Response::Success(value) => Ok(value.result),
                        protocol::Response::Failure(error) => Err(error.error.code),
                    };
                }
                _ => return Err("INVALID_RESPONSE".into()),
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
struct Slot {
    worker: Mutex<Worker>,
    child: Arc<Mutex<Child>>,
}
impl Slot {
    fn new(worker: Worker) -> Self {
        Self {
            child: worker.child.clone(),
            worker: Mutex::new(worker),
        }
    }
    fn stop(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct Manager {
    db: Arc<Mutex<Db>>,
    services: Arc<crate::host_services::Services>,
    executable: PathBuf,
    workers: Mutex<HashMap<String, Arc<Slot>>>,
    maintenance: Mutex<HashSet<String>>,
}
impl Manager {
    #[cfg(test)]
    pub fn new(db: Arc<Mutex<Db>>) -> Self {
        Self::with_services(
            db,
            Arc::new(crate::host_services::Services::new(
                crate::task_runtime::TaskRuntime::memory(),
            )),
        )
    }
    pub fn with_services(
        db: Arc<Mutex<Db>>,
        services: Arc<crate::host_services::Services>,
    ) -> Self {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("cruciblebox-plugin-host.exe")));
        let root = std::env::current_dir().unwrap_or_default();
        let development = [
            Some(root.join(
                "src-tauri/cruciblebox-plugin-host/target/debug/cruciblebox-plugin-host.exe",
            )),
            Some(root.join("cruciblebox-plugin-host/target/debug/cruciblebox-plugin-host.exe")),
        ];
        let candidates = if cfg!(debug_assertions) {
            vec![
                development[0].clone(),
                development[1].clone(),
                Some(root.join("target/debug/cruciblebox-plugin-host.exe")),
                exe,
            ]
        } else {
            vec![exe, development[0].clone(), development[1].clone()]
        };
        let executable = candidates
            .into_iter()
            .flatten()
            .find(|p| p.is_file())
            .unwrap_or_else(|| root.join("cruciblebox-plugin-host.exe"));
        Self {
            db,
            services,
            executable,
            workers: Mutex::new(HashMap::new()),
            maintenance: Mutex::new(HashSet::new()),
        }
    }
    pub fn services(&self) -> Arc<crate::host_services::Services> {
        self.services.clone()
    }
    pub fn begin_maintenance(self: &Arc<Self>, owner: &str) -> Result<Maintenance, String> {
        if !self
            .maintenance
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .insert(owner.into())
        {
            return Err("BUSY".into());
        }
        self.stop(owner);
        Ok(Maintenance {
            manager: self.clone(),
            owner: owner.into(),
        })
    }
    fn check_maintenance(&self, owner: &str) -> Result<(), String> {
        if self
            .maintenance
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .contains(owner)
        {
            Err("SESSION_DENIED".into())
        } else {
            Ok(())
        }
    }
    pub fn call(&self, owner: &str, method: &str, args: &[Value]) -> Result<Value, String> {
        self.check_maintenance(owner)?;
        let worker = {
            let mut workers = self.workers.try_lock().map_err(|_| "BUSY")?;
            if let Some(worker) = workers.get(owner) {
                worker.clone()
            } else {
                if workers.len() >= protocol::MAX_BACKEND_WORKERS {
                    return Err("BUSY".into());
                }
                let worker = Arc::new(Slot::new(Worker::start(
                    owner,
                    self.db.clone(),
                    &self.executable,
                    self.services.clone(),
                )?));
                self.check_maintenance(owner)?;
                workers.insert(owner.into(), worker.clone());
                worker
            }
        };
        let result = worker
            .worker
            .try_lock()
            .map_err(|_| "BUSY")?
            .control("call", json!({"method":method,"args":args}));
        if result.is_err() {
            self.stop(owner);
        }
        result
    }
    pub fn activate(&self, owner: &str) -> Result<(), String> {
        self.check_maintenance(owner)?;
        let mut workers = self.workers.try_lock().map_err(|_| "BUSY")?;
        if workers.contains_key(owner) {
            return Ok(());
        }
        if workers.len() >= protocol::MAX_BACKEND_WORKERS {
            return Err("BUSY".into());
        }
        let worker = Worker::start(
            owner,
            self.db.clone(),
            &self.executable,
            self.services.clone(),
        )?;
        self.check_maintenance(owner)?;
        workers.insert(owner.into(), Arc::new(Slot::new(worker)));
        Ok(())
    }
    pub fn stop(&self, owner: &str) {
        let removed = self
            .workers
            .lock()
            .ok()
            .and_then(|mut workers| workers.remove(owner));
        if let Some(slot) = removed {
            slot.stop();
        }
    }
    pub fn shutdown(&self) {
        let workers = self
            .workers
            .lock()
            .map(|mut workers| workers.drain().map(|(_, slot)| slot).collect::<Vec<_>>())
            .unwrap_or_default();
        for worker in workers {
            worker.stop();
        }
    }
}

pub struct Adapter {
    pub db: Arc<Mutex<Db>>,
    pub backend: Option<Arc<Manager>>,
    pub services: Arc<crate::host_services::Services>,
}
fn public_task(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.remove("result");
        if let Some(progress) = object.get_mut("progress").and_then(Value::as_object_mut) {
            progress.remove("extra");
        }
    }
    value
}
impl Adapter {
    fn owned_task(&self, owner: &str, id: &str) -> Option<Value> {
        self.services.runtime.get(owner, id).or_else(|| {
            self.services
                .runtime
                .get("plugin-process", id)
                .filter(|value| value["resourceKey"] == format!("plugin-process:{owner}"))
        })
    }
}
impl protocol::runtime::Storage for Adapter {
    fn result_begin(
        &self,
        owner: &str,
        session: &str,
        method: &str,
        value: &Value,
    ) -> Result<Value, String> {
        self.services.results.open(owner, session, method, value)
    }
    fn result_chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        offset: u64,
    ) -> Result<Value, String> {
        let method = self.services.results.method(owner, session, id)?;
        let record = self
            .db
            .try_lock()
            .map_err(|_| "BUSY")?
            .plugin_backend_record(owner)
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .filter(|v| v.enabled)
            .ok_or("SESSION_DENIED")?;
        let permissions: Vec<String> =
            serde_json::from_str(&record.permissions).map_err(|_| "PERMISSION_DENIED")?;
        if protocol::required_capability(&method)
            .flatten()
            .is_some_and(|required| !permissions.contains(&required))
        {
            return Err("PERMISSION_DENIED".into());
        }
        let service = match method.as_str() {
            "document.call" => "document-engine",
            "environment.call" => "unienv",
            "archive.call" => "archive-extractor",
            _ => return Err("PERMISSION_DENIED".into()),
        };
        crate::next_trusted::verify(
            service,
            std::path::Path::new(&record.installed_path),
            &permissions,
        )?;
        self.services.results.chunk(owner, session, id, offset)
    }
    fn result_close(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        self.services.results.close(owner, session, id)
    }

    fn ui_call(&self, _owner: &str, method: &str, params: &Value) -> Result<Value, String> {
        if self.backend.is_none() {
            return Err("SESSION_DENIED".into());
        }
        Ok(json!({"operation":method,"params":params}))
    }

    fn tasks_get(&self, owner: &str, id: &str) -> Result<Value, String> {
        Ok(self
            .owned_task(owner, id)
            .map(public_task)
            .unwrap_or(Value::Null))
    }
    fn tasks_cancel(&self, owner: &str, id: &str) -> Result<Value, String> {
        let direct = self.services.runtime.get(owner, id).is_some();
        let owned = self.owned_task(owner, id).is_some();
        let accepted = owned
            && self
                .services
                .runtime
                .cancel(if direct { owner } else { "plugin-process" }, id);
        Ok(json!({"accepted":accepted,"task":self.owned_task(owner,id).map(public_task)}))
    }
    fn tasks_list(&self, owner: &str, limit: u32, after: Option<&str>) -> Result<Value, String> {
        let mut values = self.services.runtime.list(owner);
        values.extend(
            self.services
                .runtime
                .list("plugin-process")
                .into_iter()
                .filter(|value| value["resourceKey"] == format!("plugin-process:{owner}")),
        );
        values.sort_by(|a, b| a["taskId"].as_str().cmp(&b["taskId"].as_str()));
        let mut items = Vec::new();
        let mut next = None;
        for value in values.into_iter().filter(|value| {
            after.is_none_or(|after| value["taskId"].as_str().is_some_and(|id| id > after))
        }) {
            let value = public_task(value);
            let mut candidate = items.clone();
            candidate.push(value.clone());
            if items.len() >= limit as usize
                || serde_json::to_vec(&candidate)
                    .map_err(|_| "INVALID_RESPONSE")?
                    .len()
                    > 60 * 1024
            {
                next = items
                    .last()
                    .and_then(|v: &Value| v["taskId"].as_str().map(str::to_owned));
                if items.is_empty() {
                    return Err("BUDGET_EXCEEDED".into());
                }
                break;
            }
            items.push(value);
        }
        Ok(json!({"items":items,"nextCursor":next}))
    }
    fn config_get(&self, owner: &str) -> Result<Value, String> {
        let db = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        db.plugin_config_get(owner)
    }
    fn config_patch(&self, owner: &str, values: &Value) -> Result<Value, String> {
        let db = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        db.plugin_config_patch(owner, values)
    }
    fn service_call(&self, owner: &str, service: &str, payload: &Value) -> Result<Value, String> {
        if owner != service {
            return Err("PERMISSION_DENIED".into());
        }
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        let record = repository
            .plugin_backend_record(owner)
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .ok_or("SESSION_DENIED")?;
        if !record.enabled {
            return Err("SESSION_DENIED".into());
        }
        crate::permissions::PermissionGuard::from_json(&record.permissions)
            .assert_trusted_service(service)
            .map_err(|_| "PERMISSION_DENIED")?;
        if record.name != service {
            return Err("PERMISSION_DENIED".into());
        }
        let declared: Vec<String> =
            serde_json::from_str(&record.permissions).map_err(|_| "PERMISSION_DENIED")?;
        crate::next_trusted::verify(
            service,
            std::path::Path::new(&record.installed_path),
            &declared,
        )?;
        // Next details use the original live task payload. Legacy display compaction
        // remains limited to the legacy transport, never the new result stream.
        if service == "document-engine" && payload["type"] == "document.jobs.get" {
            if let Some(id) = payload["taskId"]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 128)
            {
                if let Some(value) = self.services.runtime.get(owner, id) {
                    return Ok(value);
                }
            }
        }
        crate::envelope_host::host_dispatch(
            &self.db,
            &self.services,
            owner,
            "trusted.invoke",
            &json!({"service":service,"operation":"message","payload":payload}),
            &|_, _| {},
        )
    }
    fn keys(
        &self,
        owner: &str,
        prefix: &str,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Value, String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::keys(&repository, owner, prefix, limit, after)
    }
    fn read_begin(&self, owner: &str, session: &str, key: &str) -> Result<Value, String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::read_begin(&repository, owner, session, key)
    }
    fn read_chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        offset: u64,
    ) -> Result<Value, String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::read_chunk(&repository, owner, session, id, offset)
    }
    fn read_close(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::read_close(&repository, owner, session, id)
    }
    fn write_begin(
        &self,
        owner: &str,
        session: &str,
        writes: &[Value],
        deletes: &[Value],
    ) -> Result<Value, String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::write_begin(&repository, owner, session, writes, deletes)
    }
    fn write_chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        key: &str,
        offset: u64,
        data: &str,
    ) -> Result<Value, String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::write_chunk(&repository, owner, session, id, key, offset, data)
    }
    fn write_commit(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::write_commit(&repository, owner, session, id)
    }
    fn write_abort(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::write_abort(&repository, owner, session, id)
    }

    fn get(&self, owner: &str, key: &str) -> Result<Option<Value>, String> {
        protocol::runtime::Storage::get(&*self.db.try_lock().map_err(|_| "BUSY")?, owner, key)
    }
    fn set(&self, owner: &str, key: &str, value: &Value) -> Result<(), String> {
        protocol::runtime::Storage::set(
            &*self.db.try_lock().map_err(|_| "BUSY")?,
            owner,
            key,
            value,
        )
    }
    fn delete(&self, owner: &str, key: &str) -> Result<(), String> {
        protocol::runtime::Storage::delete(&*self.db.try_lock().map_err(|_| "BUSY")?, owner, key)
    }
    fn batch(&self, owner: &str, operations: &[Value]) -> Result<(), String> {
        protocol::runtime::Storage::batch(
            &*self.db.try_lock().map_err(|_| "BUSY")?,
            owner,
            operations,
        )
    }
    fn list(
        &self,
        owner: &str,
        prefix: &str,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Value, String> {
        let repository = self.db.try_lock().map_err(|_| "BUSY")?.clone();
        protocol::runtime::Storage::list(&repository, owner, prefix, limit, after)
    }
    fn backend_call(&self, owner: &str, method: &str, args: &[Value]) -> Result<Value, String> {
        self.backend
            .as_ref()
            .ok_or("SESSION_DENIED")?
            .call(owner, method, args)
    }
}

pub struct Maintenance {
    manager: Arc<Manager>,
    owner: String,
}
impl Drop for Maintenance {
    fn drop(&mut self) {
        if let Ok(mut owners) = self.manager.maintenance.lock() {
            owners.remove(&self.owner);
        }
    }
}

#[cfg(test)]
mod tests {
    fn pin_fixture(db: &Arc<Mutex<Db>>, owner: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../plugins")
            .join(owner);
        let manifest = protocol::validate_manifest(
            &std::fs::read_to_string(path.join("plugin.json")).unwrap(),
        )
        .unwrap();
        db.lock()
            .unwrap()
            .conn()
            .lock()
            .unwrap()
            .execute(
                "UPDATE plugins SET installed_path=?1,permissions=?2 WHERE id=?3",
                rusqlite::params![
                    path.to_string_lossy(),
                    serde_json::to_string(&manifest.permissions).unwrap(),
                    owner
                ],
            )
            .unwrap();
    }

    #[test]
    fn next_services_use_shared_composition_and_recheck_permission_and_identity() {
        use protocol::runtime::Storage;
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(
            Db::open(&root.path().join("host.sqlite")).unwrap(),
        ));
        let services = Arc::new(crate::host_services::Services::new(
            crate::task_runtime::TaskRuntime::memory(),
        ));
        let adapter = Adapter {
            db: db.clone(),
            backend: None,
            services,
        };
        for owner in ["document-engine", "unienv", "archive-extractor"] {
            db.lock().unwrap().conn().lock().unwrap().execute(
                "INSERT INTO plugins(id,name,version,display_name,entry_main,entry_renderer,installed_path,enabled,permissions) VALUES (?1,?1,'1.0.0',?1,'dist/main.js','dist/renderer.js','test',1,?2)",
                rusqlite::params![owner, serde_json::to_string(&vec![format!("trusted:{owner}")]).unwrap()],
            ).unwrap();
        }
        for owner in ["document-engine", "unienv", "archive-extractor"] {
            pin_fixture(&db, owner);
        }
        assert_eq!(
            adapter
                .service_call(
                    "diary",
                    "document-engine",
                    &json!({"type":"document.jobs.get","taskId":"missing"})
                )
                .unwrap_err(),
            "PERMISSION_DENIED"
        );
        assert_eq!(
            adapter
                .service_call(
                    "document-engine",
                    "document-engine",
                    &json!({"type":"document.jobs.get","taskId":"missing"})
                )
                .unwrap()["code"],
            "task-not-found"
        );
        assert_eq!(
            adapter
                .service_call(
                    "unienv",
                    "unienv",
                    &json!({"type":"getTask","taskId":"missing"})
                )
                .unwrap()["code"],
            "task-not-found"
        );
        assert!(adapter
            .service_call(
                "archive-extractor",
                "archive-extractor",
                &json!({"type":"getTask","taskId":"missing"})
            )
            .is_ok());
        db.lock()
            .unwrap()
            .conn()
            .lock()
            .unwrap()
            .execute("UPDATE plugins SET permissions='[]' WHERE id='unienv'", [])
            .unwrap();
        assert_eq!(
            adapter
                .service_call(
                    "unienv",
                    "unienv",
                    &json!({"type":"getTask","taskId":"missing"})
                )
                .unwrap_err(),
            "PERMISSION_DENIED"
        );
        db.lock()
            .unwrap()
            .conn()
            .lock()
            .unwrap()
            .execute(
                "UPDATE plugins SET enabled=0 WHERE id='document-engine'",
                [],
            )
            .unwrap();
        assert_eq!(
            adapter
                .service_call(
                    "document-engine",
                    "document-engine",
                    &json!({"type":"document.jobs.get","taskId":"missing"})
                )
                .unwrap_err(),
            "SESSION_DENIED"
        );
    }

    #[test]
    fn task_access_is_owner_scoped_and_late_cancel_preserves_terminal_state() {
        use protocol::runtime::Storage;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::task_runtime::TaskRuntime::memory();
        runtime
            .run_sync("document-engine", "convert", Some("a"), |_| {
                Ok(json!({"private":"secret"}))
            })
            .unwrap();
        runtime
            .run_sync("document-engine", "convert", Some("b"), |_| Ok(Value::Null))
            .unwrap();
        runtime
            .run_sync("plugin-process", "plugin-process:diary", Some("c"), |_| {
                Ok(Value::Null)
            })
            .unwrap();
        let adapter = Adapter {
            db: Arc::new(Mutex::new(
                Db::open(&root.path().join("host.sqlite")).unwrap(),
            )),
            backend: None,
            services: Arc::new(crate::host_services::Services::new(runtime)),
        };
        assert!(adapter.tasks_get("diary", "a").unwrap().is_null());
        assert!(adapter.tasks_get("document-engine", "c").unwrap().is_null());
        assert_eq!(
            adapter.tasks_get("diary", "c").unwrap()["status"],
            "succeeded"
        );
        let before = adapter.tasks_get("document-engine", "a").unwrap();
        assert!(before.get("result").is_none());
        assert_eq!(
            adapter.tasks_cancel("diary", "a").unwrap(),
            json!({"accepted":false,"task":null})
        );
        assert_eq!(
            adapter.tasks_cancel("document-engine", "a").unwrap()["accepted"],
            false
        );
        assert_eq!(adapter.tasks_get("document-engine", "a").unwrap(), before);
        let first = adapter.tasks_list("document-engine", 1, None).unwrap();
        assert_eq!(first["items"][0]["taskId"], "a");
        assert_eq!(first["nextCursor"], "a");
        let second = adapter.tasks_list("document-engine", 1, Some("a")).unwrap();
        assert_eq!(second["items"][0]["taskId"], "b");
        assert!(second["nextCursor"].is_null());
        assert_eq!(
            adapter.tasks_list("diary", 20, None).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn actual_document_result_stream_is_correlated_and_revoked_permissions_stop_download() {
        let root = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(
            Db::open(&root.path().join("host.sqlite")).unwrap(),
        ));
        db.lock().unwrap().conn().lock().unwrap().execute("INSERT INTO plugins(id,name,version,display_name,entry_main,entry_renderer,installed_path,enabled,permissions) VALUES ('document-engine','document-engine','1.0.0','Document','dist/main.js','dist/renderer.js','test',1,'[\"trusted:document-engine\"]')",[]).unwrap();
        pin_fixture(&db, "document-engine");
        let runtime = crate::task_runtime::TaskRuntime::memory();
        let expected = json!({"text":"中文🙂".repeat(30000)});
        runtime
            .run_sync("document-engine", "convert", Some("large-document"), |_| {
                Ok(expected.clone())
            })
            .unwrap();
        let adapter = Adapter {
            db: db.clone(),
            backend: None,
            services: Arc::new(crate::host_services::Services::new(runtime)),
        };
        let token = "a".repeat(64);
        let mut session = Session::new(
            token.clone(),
            "main".into(),
            "document-engine".into(),
            100000,
            ["trusted:document-engine".into()],
        );
        let call = |method: &str, params: Value, id: &str| {
            json!({"wireVersion":3,"requestId":id,"session":token,"method":method,"params":params})
                .to_string()
        };
        let reply = protocol::runtime::dispatch_response(
            &mut session,
            &adapter,
            &call(
                "document.call",
                json!({"payload":{"type":"document.jobs.get","taskId":"large-document"}}),
                "r1",
            ),
            "main",
            1,
        )
        .unwrap();
        let reply = serde_json::to_value(reply).unwrap();
        assert_eq!(reply["ok"], true);
        let id = reply["result"]["$nextResult"]["readId"].as_str().unwrap();
        let page = protocol::runtime::dispatch_response(
            &mut session,
            &adapter,
            &call("result.read.chunk", json!({"readId":id,"offset":0}), "r2"),
            "main",
            2,
        )
        .unwrap();
        assert_eq!(serde_json::to_value(page).unwrap()["ok"], true);
        db.lock()
            .unwrap()
            .conn()
            .lock()
            .unwrap()
            .execute(
                "UPDATE plugins SET permissions='[]' WHERE id='document-engine'",
                [],
            )
            .unwrap();
        let denied = protocol::runtime::dispatch_response(
            &mut session,
            &adapter,
            &call(
                "result.read.chunk",
                json!({"readId":id,"offset":24576}),
                "r3",
            ),
            "main",
            3,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(denied).unwrap()["error"]["code"],
            "PERMISSION_DENIED"
        );
    }

    use super::*;
    #[test]
    fn real_next_worker_boot_capability_restart_and_crash_isolation() {
        let directory = std::env::temp_dir().join(format!(
            "cb-next-worker-{}",
            crate::rand_token::random_token_hex().unwrap()
        ));
        std::fs::create_dir_all(directory.join("dist")).unwrap();
        std::fs::write(directory.join("plugin.json"), json!({"manifestVersion":5,"sdkApiVersion":5,"wireVersion":3,"dataSchemaVersion":1,"id":"next-worker-test","displayName":"Next worker test","version":"1.0.0","renderer":"dist/renderer.js","backend":"dist/main.js","permissions":["storage:read","storage:write"]}).to_string()).unwrap();
        std::fs::write(
            directory.join("dist/renderer.js"),
            "export default function() {}",
        )
        .unwrap();
        std::fs::write(directory.join("dist/main.js"), r#"exports.activate=async function(ctx){let seq=0;async function request(method,params){const r=await ctx.exchange({wireVersion:3,requestId:'test-'+(++seq),session:ctx.session,method,params});if(!r.ok)throw Error(r.error.code);return r.result;}return {save:async value=>request('storage.set',{key:'note.v1',value}),load:async()=>request('storage.get',{key:'note.v1'})};};"#).unwrap();
        // Exercise the actual SDK in QuickJS without browser globals.
        let generated = include_str!("../../packages/cruciblebox-next-api/src/generated.mjs")
            .replace("export const ", "const ");
        let sdk = include_str!("../../packages/cruciblebox-next-api/src/index.mjs")
            .replace("import { contract } from './generated.mjs'", "")
            .replace(
                "export { contract, contractSha256 } from './generated.mjs'",
                "",
            )
            .replace("export function ", "function ");
        let backend = format!(
            "{generated}\n{sdk}\n{}",
            r#"
exports.activate = async function(ctx) {
  const api = createClient(ctx);
  return {
    save: async value => api.storage.set('note.v1',value),
    load: async () => api.storage.get('note.v1'),
    saveLarge: async () => {
      await api.storage.transact([{type:'set',key:'large.v1',value:'日记🌏'.repeat(131072)}]);
      return true;
    },
    loadLarge: async () => (await api.storage.get('large.v1')).length,
    saveMax: async () => {
      await api.storage.set('maximum.v1','x'.repeat(4194302));
      return true;
    },
    loadMax: async () => (await api.storage.get('maximum.v1')).length
  };
};"#
        );
        std::fs::write(directory.join("dist/main.js"), backend).unwrap();
        let db = Arc::new(Mutex::new(Db::open(&directory.join("test.db")).unwrap()));
        db.lock().unwrap().conn().lock().unwrap().execute("INSERT INTO plugins(id,name,version,display_name,entry_main,installed_path,enabled,permissions) VALUES(?1,?1,'1.0.0','Next worker test','dist/main.js',?2,1,?3)", rusqlite::params!["next-worker-test", directory.to_string_lossy(), "[\"storage:read\",\"storage:write\"]"]).unwrap();
        let manager = Arc::new(Manager::new(db.clone()));
        assert!(
            manager.executable.is_file(),
            "Next sidecar must be built before host tests"
        );
        manager.activate("next-worker-test").unwrap();
        let note = json!({"schema":1,"text":"中文 worker 实测"});
        manager
            .call("next-worker-test", "save", std::slice::from_ref(&note))
            .unwrap();
        assert_eq!(manager.call("next-worker-test", "load", &[]).unwrap(), note);
        assert_eq!(
            manager.call("next-worker-test", "saveLarge", &[]).unwrap(),
            json!(true)
        );
        assert_eq!(
            manager.call("next-worker-test", "loadLarge", &[]).unwrap(),
            json!(524288)
        );
        let maximum_started = Instant::now();
        assert_eq!(
            manager.call("next-worker-test", "saveMax", &[]).unwrap(),
            json!(true)
        );
        assert_eq!(
            manager.call("next-worker-test", "loadMax", &[]).unwrap(),
            json!(4194302)
        );
        eprintln!(
            "[next-storage] actual QuickJS maximum 4MiB roundtrip: {:?}",
            maximum_started.elapsed()
        );
        let stored = db
            .lock()
            .unwrap()
            .storage_get("next-worker-test", "large.v1")
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<String>(&stored).unwrap(),
            "日记🌏".repeat(131072)
        );
        manager.stop("next-worker-test");
        assert_eq!(manager.call("next-worker-test", "load", &[]).unwrap(), note);
        {
            let slot = manager
                .workers
                .lock()
                .unwrap()
                .get("next-worker-test")
                .unwrap()
                .clone();
            slot.child.lock().unwrap().kill().unwrap();
        }
        assert!(manager.call("next-worker-test", "load", &[]).is_err());
        assert_eq!(manager.call("next-worker-test", "load", &[]).unwrap(), note);
        let maintenance = manager.begin_maintenance("next-worker-test").unwrap();
        assert_eq!(
            manager.call("next-worker-test", "load", &[]).unwrap_err(),
            "SESSION_DENIED"
        );
        drop(maintenance);
        db.lock()
            .unwrap()
            .conn()
            .lock()
            .unwrap()
            .execute(
                "UPDATE plugins SET enabled=0 WHERE id='next-worker-test'",
                [],
            )
            .unwrap();
        assert_eq!(
            manager.call("next-worker-test", "load", &[]).unwrap_err(),
            "SESSION_DENIED"
        );
        assert_eq!(
            db.lock()
                .unwrap()
                .storage_get("next-worker-test", "note.v1")
                .unwrap(),
            Some(note.to_string())
        );
        manager.shutdown();
        drop(manager);
        drop(db);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}

use super::{BackendCall, Instance, InstanceKind, Manager, Session, Slot, Worker};
use crate::db::PluginRow;
use cruciblebox_next2_protocol::BACKGROUND_METHOD;
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, TryLockError,
    },
    time::{Duration, Instant},
};

impl Manager {
    pub(super) fn slot_for_record(
        self: &Arc<Self>,
        row: &PluginRow,
        kind: InstanceKind,
    ) -> Result<Arc<Slot>, String> {
        let key = (row.id.clone(), kind);
        {
            let mut workers = self.workers.lock().map_err(|_| "INTERNAL_ERROR")?;
            self.check(row)?;
            match workers.get(&key) {
                Some(Instance::Ready(slot)) => {
                    let slot = slot.clone();
                    drop(workers);
                    if slot.version != row.version {
                        return Err("PROVIDER_CHANGED".into());
                    }
                    self.installed(row)?;
                    return Ok(slot);
                }
                Some(Instance::Starting) => return Err("BACKEND_STARTING".into()),
                None if workers.len() >= 16 => return Err("BUSY".into()),
                None => {
                    workers.insert(key.clone(), Instance::Starting);
                }
            }
        }

        let result = self.launch_slot(row, kind).and_then(|slot| {
            let mut workers = self.workers.lock().map_err(|_| "INTERNAL_ERROR")?;
            // Maintenance can begin during package verification or handshake.
            self.check(row)?;
            self.supervise(row, kind, &slot)?;
            workers.insert(key.clone(), Instance::Ready(slot.clone()));
            Ok(slot)
        });
        if result.is_err() {
            self.workers
                .lock()
                .map_err(|_| "INTERNAL_ERROR")?
                .remove(&key);
        }
        self.startup_finished.notify_all();
        result
    }

    fn launch_slot(&self, row: &PluginRow, kind: InstanceKind) -> Result<Arc<Slot>, String> {
        let plugin = self.installed(row)?;
        let resources = self.mounts(row, &plugin)?;
        let worker = Worker::launch(&plugin, &Self::data(row)?, &resources, &self.runner)?;
        let manifest = plugin.manifest();
        Ok(Arc::new(Slot {
            version: row.version.clone(),
            background_interval: if kind == InstanceKind::Service {
                manifest.background_interval_ms.map(Duration::from_millis)
            } else {
                None
            },
            stop: worker.stop_handle(),
            admission: Mutex::new(Session::new(
                manifest.clone(),
                worker.session().into(),
                "native".into(),
                "backend".into(),
                u64::MAX,
            )),
            worker: Mutex::new(Some(worker)),
        }))
    }

    fn supervise(
        self: &Arc<Self>,
        row: &PluginRow,
        kind: InstanceKind,
        slot: &Arc<Slot>,
    ) -> Result<(), String> {
        let manager = Arc::downgrade(self);
        let slot = Arc::downgrade(slot);
        let owner = row.id.clone();
        let provider = row.name.clone();
        std::thread::Builder::new()
            .name("next2-worker-supervisor".into())
            .spawn(move || {
                let mut last_tick = Instant::now();
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    let (Some(manager), Some(slot)) = (manager.upgrade(), slot.upgrade()) else {
                        break;
                    };
                    if manager.closing.load(Ordering::SeqCst)
                        || !manager.is_current(&owner, kind, &slot)
                    {
                        break;
                    }
                    if let Err(failure) = manager.poll_worker(&slot, &provider, &mut last_tick) {
                        if matches!(manager.retire(&owner, kind, &slot), Ok(true)) {
                            manager.lifecycle_failure(&owner, &failure);
                        }
                        break;
                    }
                }
            })
            .map_err(|failure| failure.to_string())?;
        Ok(())
    }

    fn poll_worker(
        self: &Arc<Self>,
        slot: &Slot,
        provider: &str,
        last_tick: &mut Instant,
    ) -> Result<(), String> {
        let mut guard = match slot.worker.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return Ok(()),
            Err(TryLockError::Poisoned(_)) => return Err("BACKEND_LOCK_FAILED".into()),
        };
        let Some(worker) = guard.as_mut() else {
            return Ok(());
        };
        let mut session = slot.admission.lock().map_err(|_| "INTERNAL_ERROR")?;
        let mut sequence = 0;
        let mut callback = |request: &cruciblebox_next2_protocol::Request| {
            self.backend_capability(
                &mut session,
                request,
                BackendCall {
                    provider,
                    caller: None,
                    task: None,
                    sequence: &mut sequence,
                },
            )
        };
        let result = worker.poll_idle(&mut callback).and_then(|()| {
            if slot
                .background_interval
                .is_some_and(|interval| last_tick.elapsed() >= interval)
            {
                worker
                    .call(
                        provider,
                        BACKGROUND_METHOD,
                        json!({}),
                        None,
                        &AtomicBool::new(false),
                        &mut callback,
                    )
                    .map_err(|failure| format!("BACKGROUND_HOOK_FAILED: {}", failure.code))?;
                *last_tick = Instant::now();
            }
            Ok(())
        });
        if result.is_err() {
            drop(guard.take());
        }
        result
    }

    fn is_current(&self, owner: &str, kind: InstanceKind, slot: &Arc<Slot>) -> bool {
        self.workers.lock().is_ok_and(|workers| matches!(workers.get(&(owner.into(), kind)), Some(Instance::Ready(current)) if Arc::ptr_eq(current, slot)))
    }

    pub(super) fn retire(
        &self,
        owner: &str,
        kind: InstanceKind,
        slot: &Arc<Slot>,
    ) -> Result<bool, String> {
        let mut workers = self.workers.lock().map_err(|_| "INTERNAL_ERROR")?;
        let key = (owner.into(), kind);
        if matches!(workers.get(&key), Some(Instance::Ready(current)) if Arc::ptr_eq(current, slot))
        {
            workers.remove(&key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(super) fn lifecycle_failure(&self, owner: &str, failure: &str) {
        if let Ok(db) = self.db.lock() {
            if let Err(error) = db.log_write(owner, "error", &format!("Backend stopped: {failure}"))
            {
                eprintln!("[next2] backend {owner} failed: {failure}; log write failed: {error}");
            }
        }
    }
}

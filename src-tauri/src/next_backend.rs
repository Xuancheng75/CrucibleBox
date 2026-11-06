//! Next 3.0 backend lifecycle coordinator.
use crate::db::Db;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
pub struct Manager {
    services: Arc<crate::host_services::Services>,
    maintenance: Arc<Mutex<HashSet<String>>>,
    pub next2: Arc<crate::next2_host::Manager>,
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
        let mut directory = std::env::current_exe().expect("host executable location");
        directory.pop();
        let name = "cruciblebox-plugin-host.exe";
        let adjacent = directory.join(name);
        let mut candidates = vec![adjacent.clone(), directory.join("resources").join(name)];
        if cfg!(debug_assertions) {
            if let Some(target) = std::env::var_os("CARGO_TARGET_DIR") {
                // An explicit development target selects its own runner.
                candidates = vec![std::path::PathBuf::from(target).join("debug").join(name)];
            } else {
                let root = std::env::current_dir().unwrap_or_default();
                candidates.extend([
                    root.join("src-tauri/cruciblebox-plugin-host/target/debug")
                        .join(name),
                    root.join("cruciblebox-plugin-host/target/debug").join(name),
                    root.join("target/debug").join(name),
                ]);
            }
        }
        let executable = candidates
            .iter()
            .find(|path| path.is_file())
            .unwrap_or(&candidates[0])
            .clone();
        let maintenance = Arc::new(Mutex::new(HashSet::new()));
        let next2 = Arc::new(crate::next2_host::Manager::new(
            db.clone(),
            services.clone(),
            maintenance.clone(),
            executable.clone(),
        ));
        Self {
            services,
            maintenance,
            next2,
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
        if let Err(error) = self.next2.stop(owner) {
            self.maintenance
                .lock()
                .map_err(|_| "INTERNAL_ERROR")?
                .remove(owner);
            return Err(error);
        }
        Ok(Maintenance {
            manager: self.clone(),
            owner: owner.into(),
        })
    }
    pub fn activate(&self, owner: &str) -> Result<(), String> {
        self.next2.activate(owner)
    }
    pub fn shutdown(&self) -> Result<(), String> {
        self.next2.shutdown()
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

//! Next lifecycle coordination. Legacy execution is absent from production builds.
use crate::db::{Db, PluginBackendRecord};
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
type Emitter = Arc<dyn Fn(&str, Value) + Send + Sync>;
pub struct BackendProcessManager {
    db: Arc<Mutex<Db>>,
    emitter: Mutex<Option<Emitter>>,
    maintenance: Mutex<HashSet<String>>,
    lifecycle: Mutex<HashSet<String>>,
}
fn lock<T>(value: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|e| e.into_inner())
}
impl BackendProcessManager {
    #[cfg(test)]
    pub fn new(db: Arc<Mutex<Db>>) -> Arc<Self> {
        Self::with_services(
            db,
            Arc::new(crate::host_services::Services::new(
                crate::task_runtime::TaskRuntime::memory(),
            )),
        )
    }

    pub fn with_services(
        db: Arc<Mutex<Db>>,
        _services: Arc<crate::host_services::Services>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            emitter: Mutex::new(None),
            maintenance: Mutex::new(HashSet::new()),
            lifecycle: Mutex::new(HashSet::new()),
        })
    }
    pub fn set_emitter(&self, emitter: Emitter) {
        *lock(&self.emitter) = Some(emitter);
    }
    pub fn emit(&self, event: &str, payload: Value) {
        if let Some(emitter) = lock(&self.emitter).clone() {
            emitter(event, payload);
        }
    }
    pub fn broadcast_host_event(&self, event: &str, data: Value) {
        self.emit(event, data);
    }
    pub fn activate_enabled_with_permission(&self, _permission: &str) {
        let records = match lock(&self.db).enabled_plugin_backend_records() {
            Ok(records) => records,
            Err(error) => {
                eprintln!("[next-preflight] {error}");
                return;
            }
        };
        for (id, record) in records {
            let next = crate::manifest::read_manifest(std::path::Path::new(&record.installed_path))
                .is_ok_and(|manifest| manifest.manifest_version == Some(5));
            if !next {
                if let Err(error) = lock(&self.db).set_plugin_enabled(&id, false) {
                    eprintln!("[next-preflight] disable {id}: {error}");
                } else {
                    eprintln!("[next-preflight] retained legacy plugin package and data: {id}");
                }
            }
        }
    }
    pub fn ensure_activated(&self, _id: &str, _record: PluginBackendRecord) -> Result<(), String> {
        Err("legacy activation refused: LEGACY_RUNTIME_RETIRED".into())
    }
    pub fn deactivate(&self, _id: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn kill_all(&self) {}
    pub fn begin_maintenance(&self, plugin_id: &str) -> Result<(), String> {
        let mut maintenance = self.maintenance.lock().unwrap();
        if !maintenance.insert(plugin_id.to_string()) {
            return Err("plugin maintenance already in progress".into());
        }
        drop(maintenance);
        Ok(())
    }

    pub fn end_maintenance(&self, plugin_id: &str) {
        self.maintenance.lock().unwrap().remove(plugin_id);
    }

    /// 进入维护窗口并返回自动释放的 guard。
    pub fn enter_maintenance(&self, plugin_id: &str) -> Result<PluginMaintenanceGuard<'_>, String> {
        self.begin_maintenance(plugin_id)?;
        Ok(PluginMaintenanceGuard {
            manager: self,
            plugin_id: plugin_id.to_string(),
        })
    }

    /// 开始一个插件生命周期操作。返回的 guard 在所有返回路径上自动释放，
    /// 避免快速导入/卸载失败后留下永久 busy 状态。
    pub fn begin_lifecycle_operation(
        &self,
        plugin_ref: &str,
    ) -> Result<PluginLifecycleGuard<'_>, String> {
        // 旧版数据库允许 id 与 manifest name 不一致。生命周期调用方有时拿
        // id、有时只能拿 name；同时登记两个别名，避免升级/卸载与启停互相穿透。
        let keys = self.lifecycle_identity_keys(plugin_ref);
        let mut lifecycle = self.lifecycle.lock().unwrap();
        if keys.iter().any(|key| lifecycle.contains(key)) {
            return Err(format!(
                "{plugin_ref}: plugin lifecycle operation already in progress"
            ));
        }
        lifecycle.extend(keys.iter().cloned());
        Ok(PluginLifecycleGuard {
            manager: self,
            keys,
        })
    }

    fn lifecycle_identity_keys(&self, plugin_ref: &str) -> Vec<String> {
        let mut keys = vec![plugin_ref.to_string()];
        // Keep one Db guard for both lookups. Chaining two `lock(&self.db)` calls
        // in `or_else` can extend the first temporary guard until the whole
        // expression ends and deadlock on a name-based lookup.
        let db = lock(&self.db);
        let row = db
            .plugin_find_by_id(plugin_ref)
            .ok()
            .flatten()
            .or_else(|| db.plugin_find_by_name(plugin_ref).ok().flatten());
        drop(db);
        if let Some(row) = row {
            if !keys.iter().any(|key| key == &row.id) {
                keys.push(row.id);
            }
            if !keys.iter().any(|key| key == &row.name) {
                keys.push(row.name);
            }
        }
        keys
    }
}
pub struct PluginLifecycleGuard<'a> {
    manager: &'a BackendProcessManager,
    keys: Vec<String>,
}

impl Drop for PluginLifecycleGuard<'_> {
    fn drop(&mut self) {
        let mut lifecycle = self.manager.lifecycle.lock().unwrap();
        for key in &self.keys {
            lifecycle.remove(key);
        }
    }
}

/// RAII 维护窗口 guard。升级、卸载和停用遇到错误时也会释放维护标记。
pub struct PluginMaintenanceGuard<'a> {
    manager: &'a BackendProcessManager,
    plugin_id: String,
}

impl Drop for PluginMaintenanceGuard<'_> {
    fn drop(&mut self) {
        self.manager.end_maintenance(&self.plugin_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_disables_legacy_execution_preserving_raw_data_and_package() {
        let root = tempfile::tempdir().unwrap();
        let plugin = root.path().join("old");
        std::fs::create_dir(&plugin).unwrap();
        let manifest = serde_json::json!({"name":"old","version":"1.0.0","displayName":"Old","main":"dist/main.js","renderer":"dist/renderer.js","manifestVersion":2,"rendererApiVersion":2,"backend":false,"permissions":[]});
        std::fs::write(plugin.join("plugin.json"), manifest.to_string()).unwrap();
        let db = Arc::new(Mutex::new(
            Db::open(&root.path().join("data.sqlite")).unwrap(),
        ));
        lock(&db).conn().lock().unwrap().execute("INSERT INTO plugins(id,name,version,display_name,entry_main,entry_renderer,installed_path,enabled,config_data) VALUES ('old-id','old','1.0.0','Old','dist/main.js','dist/renderer.js',?1,1,'{\"raw\": 7}')",[plugin.to_string_lossy().as_ref()]).unwrap();
        lock(&db)
            .storage_set("old-id", "notes", "unmodified notes")
            .unwrap();
        let manager = BackendProcessManager::new(db.clone());
        manager.activate_enabled_with_permission("clipboard");
        let row = lock(&db).plugin_find_by_id("old-id").unwrap().unwrap();
        assert!(!row.enabled);
        assert_eq!(row.config_data, "{\"raw\": 7}");
        assert_eq!(
            lock(&db).storage_get("old-id", "notes").unwrap().as_deref(),
            Some("unmodified notes")
        );
        assert!(plugin.join("plugin.json").is_file());
        assert!(manager
            .ensure_activated(
                "old-id",
                lock(&db).plugin_backend_record("old-id").unwrap().unwrap()
            )
            .is_err());
        let guard = manager.begin_lifecycle_operation("old-id").unwrap();
        assert!(manager.begin_lifecycle_operation("old").is_err());
        drop(guard);
        assert!(manager.begin_lifecycle_operation("old").is_ok());
    }
}

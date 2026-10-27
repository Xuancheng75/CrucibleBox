//! Reviewed, offline resource imports. Source paths use the existing install path grant.
use crate::{
    db::Db,
    install::{InstallManager, InstallSource},
    next2_install,
};
use cruciblebox_next2_runtime::{
    package::{Approval, ApprovedPlugin, Provenance},
    resource::{self, InspectedResource},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}
struct TemporaryDirectory(PathBuf);
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[resource-import] retained {}: {error}", self.0.display());
            }
        }
    }
}
struct Prepared {
    plugin_id: String,
    plugin_digest: [u8; 32],
    expires: u64,
    source: InspectedResource,
    _temporary: Option<TemporaryDirectory>,
}
pub struct Manager {
    plugins: PathBuf,
    store: PathBuf,
    db: Arc<Mutex<Db>>,
    backend: Arc<crate::backend_process::BackendProcessManager>,
    next: Arc<crate::next_backend::Manager>,
    prepared: Mutex<HashMap<String, Prepared>>,
}
impl Manager {
    pub fn new(
        plugins: PathBuf,
        db: Arc<Mutex<Db>>,
        backend: Arc<crate::backend_process::BackendProcessManager>,
        next: Arc<crate::next_backend::Manager>,
    ) -> Self {
        let store = plugins
            .parent()
            .unwrap_or(&plugins)
            .join("plugin-resources");
        Self {
            plugins,
            store,
            db,
            backend,
            next,
            prepared: Mutex::new(HashMap::new()),
        }
    }
    fn plugin(&self, id: &str) -> Result<ApprovedPlugin, String> {
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let row = db.plugin_find_by_id(id)?.ok_or("PLUGIN_NOT_FOUND")?;
        next2_install::load(&db, Path::new(&row.installed_path))
    }
    pub fn preview(
        &self,
        install: &InstallManager,
        id: &str,
        source: InstallSource,
    ) -> Result<Value, String> {
        self.prepared
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .retain(|_, entry| entry.expires > now());
        if self.prepared.lock().map_err(|_| "INTERNAL_ERROR")?.len() >= 32 {
            return Err("BUSY".into());
        }
        let path = PathBuf::from(source.path);
        if !install.is_trusted_path(&path) {
            return Err("path not trusted".into());
        }
        let plugin = self.plugin(id)?;
        let temporary = if source.source_type == "zip" {
            let target = self.plugins.join(format!(
                ".resource-source-{}",
                crate::rand_token::random_token_hex()?
            ));
            std::fs::create_dir(&target).map_err(|e| e.to_string())?;
            Some(TemporaryDirectory(target))
        } else if source.source_type == "directory" {
            None
        } else {
            return Err("INVALID_RESOURCE_SOURCE".into());
        };
        let root = if let Some(temp) = &temporary {
            crate::archive::extract_resource_archive(&path, &temp.0)?;
            &temp.0
        } else {
            &path
        };
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let inspected = resource::inspect_resource(root, &next2_install::keys(&db)?)?;
        resource::check_plugin(&inspected, &plugin)?;
        let token = crate::rand_token::random_token_hex()?;
        let result = json!({"token":token,"manifest":inspected.manifest(),"manifestSha256":inspected.manifest_sha256(),"unsignedLocal":inspected.provenance() == &Provenance::LocalUnsigned});
        self.prepared.lock().map_err(|_| "INTERNAL_ERROR")?.insert(
            token,
            Prepared {
                plugin_id: id.into(),
                plugin_digest: plugin.manifest_sha256(),
                expires: now() + 15 * 60 * 1000,
                source: inspected,
                _temporary: temporary,
            },
        );
        Ok(result)
    }
    pub fn commit(&self, token: &str, approval: Approval) -> Result<Value, String> {
        let prepared = self
            .prepared
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .remove(token)
            .ok_or("RESOURCE_PREVIEW_EXPIRED")?;
        if prepared.expires <= now() {
            return Err("RESOURCE_PREVIEW_EXPIRED".into());
        }
        let _lifecycle = self
            .backend
            .begin_lifecycle_operation(&prepared.plugin_id)?;
        let _maintenance = self.next.begin_maintenance(&prepared.plugin_id)?;
        let plugin = self.plugin(&prepared.plugin_id)?;
        if plugin.manifest_sha256() != prepared.plugin_digest {
            return Err("PLUGIN_CHANGED_AFTER_RESOURCE_PREVIEW".into());
        }
        prepared.source.check_approval(&approval)?;
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        next2_install::save_resource_approval(&db, &plugin.manifest().id, &approval)?;
        let mount = resource::import_directory(
            prepared.source,
            &self.store,
            &plugin,
            approval,
            &next2_install::keys(&db)?,
        )?;
        serde_json::to_value(mount).map_err(|e| e.to_string())
    }
    pub fn discard(&self, token: &str) -> Result<(), String> {
        self.prepared
            .lock()
            .map_err(|_| "INTERNAL_ERROR")?
            .remove(token);
        Ok(())
    }
    pub fn list(&self, id: &str) -> Result<Value, String> {
        let db = self.db.lock().map_err(|_| "STORAGE_UNAVAILABLE")?.clone();
        let row = db.plugin_find_by_id(id)?.ok_or("PLUGIN_NOT_FOUND")?;
        if crate::manifest::read_manifest(Path::new(&row.installed_path))?.manifest_version
            != Some(6)
        {
            return Ok(json!([]));
        }
        let plugin = self.plugin(id)?;
        let mut result = Vec::new();
        for dependency in &plugin.manifest().resources {
            let directory = self.store.join(&plugin.manifest().id).join(&dependency.id);
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => Some(entries),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            let mut installed = Vec::new();
            if let Some(entries) = entries {
                for entry in entries {
                    let entry = entry.map_err(|e| e.to_string())?;
                    if entry.file_name().to_string_lossy().starts_with(".stage-") {
                        continue;
                    }
                    let manifest = resource::metadata(&entry.path())?;
                    if manifest.id != dependency.id || manifest.plugin_id != plugin.manifest().id {
                        return Err("RESOURCE_IDENTITY_MISMATCH".into());
                    }
                    installed.push(manifest);
                }
            }
            let available: Vec<_> = self
                .catalog(&plugin, &dependency.id)?
                .into_iter()
                .map(|release| {
                    let digest: [u8; 32] = Sha256::digest(
                        serde_json::to_vec(&release.manifest)
                            .expect("resource manifest serialization"),
                    )
                    .into();
                    json!({"release":release,"manifestSha256":digest,"unsignedLocal":true})
                })
                .collect();
            result.push(json!({"id":dependency.id,"version":dependency.version,"optional":dependency.optional == Some(true),"installed":installed,"available":available}));
        }
        Ok(json!(result))
    }
    pub fn remove(&self, plugin_id: &str, id: &str, version: &str) -> Result<(), String> {
        let _lifecycle = self.backend.begin_lifecycle_operation(plugin_id)?;
        let _maintenance = self.next.begin_maintenance(plugin_id)?;
        resource::remove(&self.store, &self.plugin(plugin_id)?, id, version)
    }
    fn catalog(
        &self,
        plugin: &ApprovedPlugin,
        resource_id: &str,
    ) -> Result<Vec<cruciblebox_next2_protocol::ResourceRelease>, String> {
        let dependency = plugin
            .manifest()
            .resources
            .iter()
            .find(|dependency| dependency.id == resource_id)
            .ok_or("RESOURCE_UNDECLARED")?;
        let Some(path) = &dependency.catalog else {
            return Ok(Vec::new());
        };
        let mut raw = Vec::new();
        std::fs::File::open(plugin.root().join(path))
            .map_err(|e| e.to_string())?
            .take(cruciblebox_next2_protocol::BUDGET_RESOURCE_MANIFEST_BYTES as u64 + 1)
            .read_to_end(&mut raw)
            .map_err(|e| e.to_string())?;
        let catalog = cruciblebox_next2_protocol::validate_resource_catalog(
            std::str::from_utf8(&raw).map_err(|e| e.to_string())?,
        )?;
        for release in &catalog.releases {
            for source in &release.downloads {
                let url = url::Url::parse(&source.url).map_err(|e| e.to_string())?;
                if url.scheme() != "https"
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err("RESOURCE_SOURCE_REQUIRES_HTTPS".into());
                }
            }
            if release.manifest.id != resource_id
                || release.manifest.plugin_id != plugin.manifest().id
                || !cruciblebox_next2_protocol::compatible(
                    &release.manifest.version,
                    &dependency.version,
                )?
                || !cruciblebox_next2_protocol::compatible(
                    &plugin.manifest().version,
                    &release.manifest.plugin_version,
                )?
            {
                return Err("RESOURCE_CATALOG_INCOMPATIBLE".into());
            }
        }
        Ok(catalog.releases)
    }
    pub fn start_download(
        install: Arc<InstallManager>,
        plugin_id: String,
        resource_id: String,
        release_id: String,
        approval: Approval,
    ) -> Result<String, String> {
        let plugin = install.resources.plugin(&plugin_id)?;
        let release = install
            .resources
            .catalog(&plugin, &resource_id)?
            .into_iter()
            .find(|release| release.id == release_id)
            .ok_or("RESOURCE_RELEASE_NOT_FOUND")?;
        let raw = serde_json::to_vec(&release.manifest).map_err(|e| e.to_string())?;
        if <[u8; 32]>::from(Sha256::digest(&raw)) != approval.manifest_sha256
            || !approval.accept_permissions
            || !approval.accept_unsigned_local
        {
            return Err("RESOURCE_APPROVAL_MISMATCH".into());
        }
        if release.manifest.contains_native_code && !approval.accept_native_code {
            return Err("NATIVE_TRUST_REQUIRED".into());
        }
        let resource = format!("resource-download:{}", plugin.manifest().id);
        let digest = plugin.manifest_sha256();
        let runtime = install.resources.next.services().runtime.clone();
        runtime.start(
            "plugin-install",
            &resource,
            Box::new(move |ctx| {
                let manager = &install.resources;
                let _lifecycle = manager.backend.begin_lifecycle_operation(&plugin_id)?;
                let plugin = manager.plugin(&plugin_id)?;
                if plugin.manifest_sha256() != digest {
                    return Err("PLUGIN_CHANGED_AFTER_RESOURCE_SELECTION".into());
                }
                let target = manager.plugins.join(format!(
                    ".resource-source-{}",
                    crate::rand_token::random_token_hex()?
                ));
                std::fs::create_dir(&target).map_err(|e| e.to_string())?;
                let temporary = TemporaryDirectory(target);
                let total: u64 = release.manifest.files.iter().map(|file| file.bytes).sum();
                let mut received = 0u64;
                let mut percent = 0;
                ctx.update_progress("download", 0, &format!("正在下载 {}", release.title), None);
                for file in &release.manifest.files {
                    ctx.check_cancelled()?;
                    let source = release
                        .downloads
                        .iter()
                        .find(|source| source.path == file.path)
                        .ok_or("RESOURCE_DOWNLOAD_INVENTORY_MISMATCH")?;
                    let url = url::Url::parse(&source.url).map_err(|e| e.to_string())?;
                    if url.scheme() != "https"
                        || !url.username().is_empty()
                        || url.password().is_some()
                    {
                        return Err("RESOURCE_SOURCE_REQUIRES_HTTPS".into());
                    }
                    let (agent, _) =
                        crate::network_policy::current().agent_for_url(url.as_str(), 10, 10)?;
                    let response = agent
                        .get(url.as_str())
                        .call()
                        .map_err(|e| format!("RESOURCE_DOWNLOAD_FAILED ({}): {e}", file.path))?;
                    if response.status() != 200 {
                        return Err(format!("RESOURCE_DOWNLOAD_STATUS: {}", response.status()));
                    }
                    let mut input = response.into_reader().take(file.bytes + 1);
                    let output = temporary.0.join(&file.path);
                    std::fs::create_dir_all(output.parent().ok_or("INVALID_RESOURCE_PATH")?)
                        .map_err(|e| e.to_string())?;
                    let mut output = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(output)
                        .map_err(|e| e.to_string())?;
                    let mut hash = Sha256::new();
                    let mut bytes = 0u64;
                    let mut buffer = [0u8; 65536];
                    loop {
                        ctx.check_cancelled()?;
                        let size = input.read(&mut buffer).map_err(|e| {
                            format!("RESOURCE_DOWNLOAD_READ_FAILED ({}): {e}", file.path)
                        })?;
                        if size == 0 {
                            break;
                        }
                        bytes += size as u64;
                        if bytes > file.bytes {
                            return Err(format!("RESOURCE_SIZE_MISMATCH: {}", file.path));
                        }
                        output
                            .write_all(&buffer[..size])
                            .map_err(|e| e.to_string())?;
                        hash.update(&buffer[..size]);
                        received += size as u64;
                        let next = ((received as u128 * 85) / u128::from(total.max(1))) as u32;
                        if next != percent {
                            percent = next;
                            ctx.update_progress(
                                "download",
                                percent,
                                &format!("{} · {} / {} bytes", release.title, received, total),
                                None,
                            );
                        }
                    }
                    output.sync_all().map_err(|e| e.to_string())?;
                    if bytes != file.bytes || format!("{:x}", hash.finalize()) != file.sha256 {
                        return Err(format!("RESOURCE_INTEGRITY_MISMATCH: {}", file.path));
                    }
                }
                ctx.check_cancelled()?;
                std::fs::write(temporary.0.join("resource.json"), raw)
                    .map_err(|e| e.to_string())?;
                let db = manager
                    .db
                    .lock()
                    .map_err(|_| "STORAGE_UNAVAILABLE")?
                    .clone();
                let keys = next2_install::keys(&db)?;
                ctx.update_progress("verify", 90, "校验资源并准备安装", None);
                let source = resource::inspect_resource(&temporary.0, &keys)?;
                source.check_approval(&approval)?;
                let _maintenance = manager.next.begin_maintenance(&plugin_id)?;
                ctx.commit_result(|| {
                    next2_install::save_resource_approval(&db, &plugin.manifest().id, &approval)?;
                    let mount = resource::import_directory(
                        source,
                        &manager.store,
                        &plugin,
                        approval,
                        &keys,
                    )?;
                    serde_json::to_value(mount).map_err(|e| e.to_string())
                })
            }),
        )
    }
}

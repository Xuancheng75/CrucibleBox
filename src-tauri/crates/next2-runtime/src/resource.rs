//! Offline resource versions are immutable, atomically published directories, separate from data.
use crate::package::{
    plain, read_limited, verify_files, verify_provenance, Approval, ApprovedPlugin, Provenance,
    PublisherKey,
};
use cruciblebox_next2_protocol::{
    compatible, validate_resource_manifest, ResourceManifest, ResourceMount,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct InspectedResource {
    root: PathBuf,
    manifest: ResourceManifest,
    provenance: Provenance,
    manifest_sha256: [u8; 32],
}
impl InspectedResource {
    pub fn manifest(&self) -> &ResourceManifest {
        &self.manifest
    }
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
    pub fn manifest_sha256(&self) -> [u8; 32] {
        self.manifest_sha256
    }
    pub fn check_approval(&self, decision: &Approval) -> Result<(), String> {
        if decision.manifest_sha256 != self.manifest_sha256 || !decision.accept_permissions {
            return Err("RESOURCE_APPROVAL_MISMATCH".into());
        }
        if self.manifest.contains_native_code && !decision.accept_native_code {
            return Err("NATIVE_TRUST_REQUIRED".into());
        }
        if self.provenance == Provenance::LocalUnsigned && !decision.accept_unsigned_local {
            return Err("LOCAL_TRUST_REQUIRED".into());
        }
        Ok(())
    }
}
pub fn inspect_resource(root: &Path, keys: &[PublisherKey]) -> Result<InspectedResource, String> {
    let meta = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !plain(&meta) || !meta.is_dir() {
        return Err("INVALID_RESOURCE_ROOT".into());
    }
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let raw = read_limited(
        &root.join("resource.json"),
        cruciblebox_next2_protocol::BUDGET_RESOURCE_MANIFEST_BYTES,
    )?;
    let manifest =
        validate_resource_manifest(std::str::from_utf8(&raw).map_err(|e| e.to_string())?)?;
    let provenance = verify_provenance(&root, &raw, &manifest.publisher.id, keys)?;
    verify_files(
        &root,
        &manifest.files,
        "resource.json",
        manifest.contains_native_code,
        40000,
    )?;
    Ok(InspectedResource {
        root,
        manifest,
        provenance,
        manifest_sha256: Sha256::digest(&raw).into(),
    })
}
pub fn metadata(root: &Path) -> Result<ResourceManifest, String> {
    let meta = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !plain(&meta) || !meta.is_dir() {
        return Err("INVALID_RESOURCE_ROOT".into());
    }
    let raw = read_limited(
        &root.join("resource.json"),
        cruciblebox_next2_protocol::BUDGET_RESOURCE_MANIFEST_BYTES,
    )?;
    validate_resource_manifest(std::str::from_utf8(&raw).map_err(|e| e.to_string())?)
}
pub fn check_plugin(source: &InspectedResource, plugin: &ApprovedPlugin) -> Result<(), String> {
    let manifest = plugin.manifest();
    if source.manifest.plugin_id != manifest.id
        || !compatible(&manifest.version, &source.manifest.plugin_version)?
    {
        return Err("RESOURCE_PLUGIN_INCOMPATIBLE".into());
    }
    let dependency = manifest
        .resources
        .iter()
        .find(|dependency| dependency.id == source.manifest.id)
        .ok_or("RESOURCE_UNDECLARED")?;
    if !compatible(&source.manifest.version, &dependency.version)? {
        return Err("RESOURCE_VERSION_INCOMPATIBLE".into());
    }
    Ok(())
}
fn directory(path: &Path) -> Result<(), String> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.to_string()),
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !plain(&meta) || !meta.is_dir() {
        return Err("NON_REGULAR_RESOURCE_DIRECTORY".into());
    }
    Ok(())
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "[next2-resource] retained staging {}: {error}",
                    self.0.display()
                );
            }
        }
    }
}
pub fn import_directory(
    source: InspectedResource,
    store: &Path,
    plugin: &ApprovedPlugin,
    decision: Approval,
    keys: &[PublisherKey],
) -> Result<ResourceMount, String> {
    check_plugin(&source, plugin)?;
    let plugin = plugin.manifest();
    source.check_approval(&decision)?;
    directory(store)?;
    let owner = store.join(&plugin.id);
    directory(&owner)?;
    let versions = owner.join(&source.manifest.id);
    directory(&versions)?;
    let versions = fs::canonicalize(versions).map_err(|e| e.to_string())?;
    let target = versions.join(&source.manifest.version);
    match fs::symlink_metadata(&target) {
        Ok(_) => return Err("RESOURCE_VERSION_EXISTS".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let mut token = [0u8; 16];
    getrandom::getrandom(&mut token).map_err(|e| e.to_string())?;
    let stage = Stage(versions.join(format!(
        ".stage-{}",
        token.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )));
    fs::create_dir(&stage.0).map_err(|e| e.to_string())?;
    for entry in &source.manifest.files {
        let output = stage.0.join(&entry.path);
        fs::create_dir_all(output.parent().ok_or("INVALID_RESOURCE_PATH")?)
            .map_err(|e| e.to_string())?;
        let input = fs::File::open(source.root.join(&entry.path)).map_err(|e| e.to_string())?;
        let mut output = fs::File::create(output).map_err(|e| e.to_string())?;
        let bytes = std::io::copy(&mut input.take(entry.bytes + 1), &mut output)
            .map_err(|e| format!("RESOURCE_COPY_FAILED: {e}"))?;
        if bytes != entry.bytes {
            return Err("RESOURCE_CHANGED_DURING_COPY".into());
        }
        output.sync_all().map_err(|e| e.to_string())?;
    }
    copy_metadata(
        &source.root,
        &stage.0,
        "resource.json",
        cruciblebox_next2_protocol::BUDGET_RESOURCE_MANIFEST_BYTES,
    )?;
    if matches!(source.provenance, Provenance::Verified { .. }) {
        copy_metadata(&source.root, &stage.0, "signature.json", 4096)?;
    }
    let staged = inspect_resource(&stage.0, keys)?;
    if staged.manifest_sha256 != decision.manifest_sha256 || staged.provenance != source.provenance
    {
        return Err("RESOURCE_CHANGED_DURING_STAGE".into());
    }
    fs::rename(&stage.0, &target).map_err(|e| {
        format!(
            "RESOURCE_PUBLISH_FAILED: {} -> {}: {e}",
            stage.0.display(),
            target.display()
        )
    })?;
    // A crash before rename leaves an inactive .stage directory; after rename the complete verified version is visible.
    Ok(ResourceMount {
        id: staged.manifest.id,
        version: staged.manifest.version,
        directory: target.to_string_lossy().into_owned(),
    })
}
fn copy_metadata(source: &Path, target: &Path, name: &str, limit: usize) -> Result<(), String> {
    let data = read_limited(&source.join(name), limit)?;
    let mut output = fs::File::create(target.join(name)).map_err(|e| e.to_string())?;
    output
        .write_all(&data)
        .and_then(|_| output.sync_all())
        .map_err(|e| e.to_string())
}

/// Select the newest declared compatible installed version. A corrupt or
/// unapproved selected version fails explicitly; it never silently falls back.
pub fn resolve(
    store: &Path,
    plugin: &ApprovedPlugin,
    keys: &[PublisherKey],
    approvals: &[Approval],
) -> Result<Vec<ResourceMount>, String> {
    let manifest = plugin.manifest();
    let mut mounts = Vec::new();
    'dependency: for dependency in &manifest.resources {
        let versions = store.join(&manifest.id).join(&dependency.id);
        for path in [
            store.to_path_buf(),
            store.join(&manifest.id),
            versions.clone(),
        ] {
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && dependency.optional == Some(true) =>
                {
                    continue 'dependency
                }
                Err(error) => return Err(format!("RESOURCE_MISSING: {}: {error}", dependency.id)),
            };
            if !plain(&meta) || !meta.is_dir() {
                return Err("INVALID_RESOURCE_DIRECTORY".into());
            }
        }
        let mut candidates = Vec::new();
        for entry in fs::read_dir(&versions).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .file_name()
                .to_str()
                .ok_or("INVALID_RESOURCE_VERSION")?
                .to_owned();
            if name.starts_with(".stage-") {
                continue;
            }
            let version = semver::Version::parse(&name).map_err(|_| "INVALID_RESOURCE_VERSION")?;
            if compatible(&name, &dependency.version)? {
                candidates.push((version, entry.path()));
            }
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0));
        let Some((version, directory)) = candidates.into_iter().next() else {
            if dependency.optional == Some(true) {
                continue;
            }
            return Err(format!("RESOURCE_VERSION_UNAVAILABLE: {}", dependency.id));
        };
        let resource = inspect_resource(&directory, keys)?;
        if resource.manifest.id != dependency.id
            || resource.manifest.plugin_id != manifest.id
            || resource.manifest.version != version.to_string()
            || !compatible(&manifest.version, &resource.manifest.plugin_version)?
        {
            return Err("RESOURCE_IDENTITY_MISMATCH".into());
        }
        let approval = approvals
            .iter()
            .find(|approval| approval.manifest_sha256 == resource.manifest_sha256)
            .ok_or("RESOURCE_APPROVAL_REQUIRED")?;
        resource.check_approval(approval)?;
        mounts.push(ResourceMount {
            id: dependency.id.clone(),
            version: resource.manifest.version,
            directory: fs::canonicalize(directory)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into(),
        });
    }
    Ok(mounts)
}

/// The host must stop the owner's backends/tasks before removing a mounted version.
pub fn remove(
    store: &Path,
    plugin: &ApprovedPlugin,
    id: &str,
    version: &str,
) -> Result<(), String> {
    if !plugin
        .manifest()
        .resources
        .iter()
        .any(|dependency| dependency.id == id)
        || semver::Version::parse(version)
            .map_err(|e| e.to_string())?
            .to_string()
            != version
    {
        return Err("INVALID_RESOURCE_VERSION".into());
    }
    let owner = store.join(&plugin.manifest().id);
    let versions = owner.join(id);
    let target = versions.join(version);
    for path in [store, &owner, &versions, &target] {
        let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !plain(&meta) || !meta.is_dir() {
            return Err("INVALID_RESOURCE_DIRECTORY".into());
        }
    }
    let parent = fs::canonicalize(versions).map_err(|e| e.to_string())?;
    let target = fs::canonicalize(target).map_err(|e| e.to_string())?;
    if target.parent() != Some(parent.as_path()) {
        return Err("RESOURCE_PATH_ESCAPE".into());
    }
    fs::remove_dir_all(target).map_err(|e| format!("RESOURCE_REMOVE_FAILED: {e}"))
}

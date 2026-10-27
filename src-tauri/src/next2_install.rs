//! Main-window consent is stored by immutable manifest digest, never supplied by a plugin.
use crate::db::Db;
use cruciblebox_next2_runtime::package::{
    self, Approval, ApprovedPlugin, InspectedPlugin, PublisherKey,
};
use std::path::{Path, PathBuf};

/// The outer directory remains the existing journal transaction root.
/// The active, fully verified code has its own version directory.
pub fn code_directory(root: &Path, manifest: &crate::manifest::Manifest) -> PathBuf {
    if manifest.manifest_version == Some(6) {
        root.join(cruciblebox_next2_protocol::CODE_DIRECTORY)
            .join(&manifest.version)
    } else {
        root.into()
    }
}
pub fn stage_layout(root: &Path, manifest: &crate::manifest::Manifest) -> Result<(), String> {
    let entries = std::fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let code = code_directory(root, manifest);
    std::fs::create_dir_all(&code).map_err(|e| e.to_string())?;
    for entry in entries {
        std::fs::rename(entry.path(), code.join(entry.file_name())).map_err(|e| e.to_string())?;
    }
    // Recovery reads only this small manifest; payload verification uses the code directory.
    std::fs::copy(code.join("plugin.json"), root.join("plugin.json")).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn data_root(installed: &Path) -> Result<&Path, String> {
    let parent = installed.parent().ok_or("INVALID_INSTALLATION_PATH")?;
    let root = if parent
        .file_name()
        .is_some_and(|name| name == cruciblebox_next2_protocol::CODE_DIRECTORY)
    {
        parent
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
    } else {
        parent.parent()
    };
    root.ok_or("INVALID_INSTALLATION_PATH".into())
}

pub fn keys(db: &Db) -> Result<Vec<PublisherKey>, String> {
    match db
        .setting_get("next2.publisherKeys")
        .map_err(|e| e.to_string())?
    {
        Some(raw) => serde_json::from_str(&raw).map_err(|e| format!("INVALID_PUBLISHER_KEYS: {e}")),
        None => Ok(Vec::new()),
    }
}
pub fn inspect(db: &Db, root: &Path) -> Result<InspectedPlugin, String> {
    package::inspect_plugin(root, &keys(db)?, env!("CARGO_PKG_VERSION"))
}
fn consent_key(plugin: &InspectedPlugin) -> String {
    let digest: String = plugin
        .manifest_sha256()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("next2.approval.{}.{digest}", plugin.manifest().id)
}
pub fn save(db: &Db, root: &Path, approval: Approval) -> Result<(), String> {
    let inspected = inspect(db, root)?;
    let key = consent_key(&inspected);
    inspected.approve(approval.clone())?;
    // Retain earlier digests so a journal rollback can use exactly the earlier consent.
    // New versions, changed permissions or changed payload digests require new consent.
    db.setting_set(
        &key,
        &serde_json::to_string(&approval).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub fn load(db: &Db, root: &Path) -> Result<ApprovedPlugin, String> {
    let inspected = inspect(db, root)?;
    let raw = db
        .setting_get(&consent_key(&inspected))
        .map_err(|e| e.to_string())?
        .ok_or("INSTALL_APPROVAL_REQUIRED")?;
    let approval =
        serde_json::from_str(&raw).map_err(|e| format!("INVALID_INSTALL_APPROVAL: {e}"))?;
    inspected.approve(approval)
}
pub fn resource_approvals(db: &Db, id: &str) -> Result<Vec<Approval>, String> {
    let prefix = format!("next2.resource-approval.{id}.");
    db.settings_all()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|(key, _)| key.starts_with(&prefix))
        .map(|(_, raw)| {
            serde_json::from_str(&raw).map_err(|e| format!("INVALID_RESOURCE_APPROVAL: {e}"))
        })
        .collect()
}
pub fn save_resource_approval(db: &Db, id: &str, approval: &Approval) -> Result<(), String> {
    let digest: String = approval
        .manifest_sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    db.setting_set(
        &format!("next2.resource-approval.{id}.{digest}"),
        &serde_json::to_string(approval).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

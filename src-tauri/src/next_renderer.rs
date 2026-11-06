//! Next 3.0 native renderer gate.
use crate::db::Db;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::{State, WebviewWindow};
pub(crate) fn trusted_origin(label: &str, main_url: &url::Url, origin: Option<&str>) -> bool {
    label == "main"
        && origin
            .and_then(|value| url::Url::parse(value).ok())
            .is_some_and(|value| {
                !matches!(value.origin(), url::Origin::Opaque(_))
                    && value.origin() == main_url.origin()
            })
}
#[tauri::command(async)]
pub fn create_next_renderer_session(
    window: WebviewWindow,
    caller: tauri::ipc::Request<'_>,
    db: State<'_, Arc<Mutex<Db>>>,
    protocol: State<'_, Arc<crate::plugin_protocol::ProtocolContext>>,
    id: String,
    lease_ms: Option<u64>,
) -> Result<Value, String> {
    let main_url = window.url().map_err(|_| "SESSION_DENIED")?;
    if !trusted_origin(
        window.label(),
        &main_url,
        caller.headers().get("Origin").and_then(|v| v.to_str().ok()),
    ) {
        return Err("SESSION_DENIED".into());
    }
    let (name, entry, permissions, directory): (String, String, String, String) = {
        let db = db.try_lock().map_err(|_| "BUSY")?;
        let record = db
            .plugin_find_by_id(&id)
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .filter(|record| record.enabled)
            .ok_or("SESSION_DENIED")?;
        (
            record.name,
            record.entry_renderer,
            record.permissions,
            record.installed_path,
        )
    };
    let root = std::path::Path::new(&directory);
    let manifest = crate::manifest::read_manifest(root)?;
    if manifest.manifest_version != Some(cruciblebox_next2_protocol::MANIFEST_VERSION)
        || manifest.name != name
        || entry.is_empty()
        || manifest.renderer != entry
    {
        return Err("SESSION_DENIED".into());
    }
    crate::manifest::validate_entrypoints(root, &manifest)?;
    crate::next2_install::load(&*db.try_lock().map_err(|_| "BUSY")?, root)?;
    let permissions: Vec<String> =
        serde_json::from_str(&permissions).map_err(|_| "SESSION_DENIED")?;
    let raw = std::fs::read_to_string(root.join("plugin.json")).map_err(|_| "INVALID_MANIFEST")?;
    let session = protocol
        .registry
        .lock()
        .map_err(|_| "INTERNAL_ERROR")?
        .create_next_with_lease(
            crate::plugin_session::CreateSessionInput {
                initial_background: "#0a0c10".into(),
                color_scheme: "dark".into(),
                plugin_id: id,
                plugin_name: name,
                plugin_directory: directory,
                renderer_entry: entry,
                runtime_path: String::new(),
                renderer_api_version: manifest.renderer_api_version.ok_or("SESSION_DENIED")?,
                permissions,
                owner_webview_label: "main".into(),
            },
            &raw,
            lease_ms.unwrap_or(crate::plugin_session::LEASE_MS),
        )?;
    Ok(crate::plugin_protocol::session_dto(&session))
}

#[tauri::command(async)]
pub fn renew_next_renderer_session(
    window: WebviewWindow,
    caller: tauri::ipc::Request<'_>,
    db: State<'_, Arc<Mutex<Db>>>,
    protocol: State<'_, Arc<crate::plugin_protocol::ProtocolContext>>,
    token: String,
) -> Result<Value, String> {
    let main_url = window.url().map_err(|_| "SESSION_DENIED")?;
    if !trusted_origin(
        window.label(),
        &main_url,
        caller.headers().get("Origin").and_then(|v| v.to_str().ok()),
    ) {
        return Err("SESSION_DENIED".into());
    }
    let candidate = protocol
        .registry
        .lock()
        .map_err(|_| "INTERNAL_ERROR")?
        .renewal_candidate(&token, "main")
        .session
        .ok_or("SESSION_DENIED")?;
    let row = db
        .lock()
        .map_err(|_| "STORAGE_UNAVAILABLE")?
        .plugin_find_by_id(&candidate.plugin_id)?
        .ok_or("SESSION_DENIED")?;
    if !renewal_matches_installation(&candidate, &row) {
        return Err("SESSION_REPLACED".into());
    }
    let renewed = protocol
        .registry
        .lock()
        .map_err(|_| "INTERNAL_ERROR")?
        .renew_active(&token, "main")?;
    Ok(serde_json::json!({"expiresAt":renewed.expires_at_ms}))
}

fn renewal_matches_installation(
    session: &crate::plugin_session::RendererSession,
    row: &crate::db::PluginRow,
) -> bool {
    row.enabled
        && row.name == session.plugin_name
        && row.version == session.plugin_version
        && row.installed_path == session.plugin_directory
        && row.entry_renderer == session.renderer_entry
        && serde_json::from_str::<Vec<String>>(&row.permissions)
            .is_ok_and(|permissions| permissions == session.permissions)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renewal_rejects_disabled_upgraded_or_changed_installations_and_keeps_record_identity() {
        let mut registry =
            crate::plugin_session::RendererSessionRegistry::new(crate::plugin_session::DEFAULT_TTL);
        let mut session = registry
            .create(crate::plugin_session::CreateSessionInput {
                initial_background: "#000000".into(),
                color_scheme: "dark".into(),
                plugin_id: "retained-guid".into(),
                plugin_name: "sample-view".into(),
                plugin_directory: "E:/approved/code/1.0.0".into(),
                renderer_entry: "dist/renderer.js".into(),
                runtime_path: String::new(),
                renderer_api_version: cruciblebox_next2_protocol::SDK_API_VERSION,
                permissions: vec![],
                owner_webview_label: "main".into(),
            })
            .unwrap();
        session.plugin_version = "1.0.0".into();
        let mut row = crate::db::PluginRow {
            id: "retained-guid".into(),
            name: "sample-view".into(),
            version: "1.0.0".into(),
            display_name: String::new(),
            description: String::new(),
            author: String::new(),
            icon: String::new(),
            entry_main: String::new(),
            entry_renderer: "dist/renderer.js".into(),
            permissions: "[]".into(),
            config_schema: "{}".into(),
            config_data: "{}".into(),
            enabled: true,
            installed_path: "E:/approved/code/1.0.0".into(),
            installed_at: String::new(),
            updated_at: String::new(),
            sort_order: 0,
        };
        assert_ne!(row.id, row.name);
        assert!(renewal_matches_installation(&session, &row));
        row.enabled = false;
        assert!(!renewal_matches_installation(&session, &row));
        row.enabled = true;
        row.version = "1.1.0".into();
        assert!(!renewal_matches_installation(&session, &row));
        row.version = "1.0.0".into();
        row.installed_path = "E:/approved/code/replaced".into();
        assert!(!renewal_matches_installation(&session, &row));
        row.installed_path = session.plugin_directory.clone();
        row.permissions = "[\"files:read\"]".into();
        assert!(!renewal_matches_installation(&session, &row));
        row.permissions = "invalid".into();
        assert!(!renewal_matches_installation(&session, &row));
    }
}

#[tauri::command(async)]
pub async fn next_renderer_request(
    window: WebviewWindow,
    caller: tauri::ipc::Request<'_>,
    protocol: State<'_, Arc<crate::plugin_protocol::ProtocolContext>>,
    backend: State<'_, Arc<crate::next_backend::Manager>>,
    raw: String,
) -> Result<Value, String> {
    let main_url = window.url().map_err(|_| "SESSION_DENIED")?;
    if !trusted_origin(
        window.label(),
        &main_url,
        caller
            .headers()
            .get("Origin")
            .and_then(|value| value.to_str().ok()),
    ) {
        return Err("SESSION_DENIED".into());
    }
    if raw.len() > cruciblebox_next2_protocol::MAX_FRAME_BYTES {
        return Err("BUDGET_EXCEEDED".into());
    }
    let request = cruciblebox_next2_protocol::validate_request(&raw)?;
    let access = protocol
        .registry
        .lock()
        .map_err(|_| "INTERNAL_ERROR")?
        .get_active(&request.session, "main");
    let metadata = access
        .session
        .filter(|_| access.ok)
        .ok_or("SESSION_DENIED")?;
    let manager = backend.next2.clone();
    tauri::async_runtime::spawn_blocking(move || manager.renderer(&raw, &metadata))
        .await
        .map_err(|error| format!("IPC_EXECUTION_UNAVAILABLE: {error}"))?
}

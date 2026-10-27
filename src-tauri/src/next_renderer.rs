//! API 6 native renderer gate.
use crate::db::Db;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::{State, WebviewWindow};
fn trusted_origin(label: &str, main_url: &url::Url, origin: Option<&str>) -> bool {
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
    if manifest.manifest_version != Some(6) || manifest.name != name || manifest.renderer != entry {
        return Err("SESSION_DENIED".into());
    }
    crate::manifest::validate_entrypoints(root, &manifest)?;
    if manifest.manifest_version == Some(6) {
        crate::next2_install::load(&*db.try_lock().map_err(|_| "BUSY")?, root)?;
    }
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
pub fn next_renderer_request(
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
    backend.next2.renderer(&raw, &metadata)
}

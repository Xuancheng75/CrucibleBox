//! Next-only native IPC gate. Legacy renderer tokens are never admitted here.
use crate::{db::Db, plugin_session::RendererSession};
use cruciblebox_next_protocol::{gateway::Session, runtime::dispatch_response, Request};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{State, WebviewWindow};

type SharedSession = Arc<Mutex<Session>>;
type SessionEntry = (u64, Option<SharedSession>);

#[derive(Default)]
pub struct Gateway {
    sessions: HashMap<String, SessionEntry>,
}

fn denial(request: &Request, code: &str) -> Result<Value, String> {
    let value = json!({"wireVersion":request.wire_version,"requestId":request.request_id,
        "ok":false,"error":{"code":code,"message":code}});
    cruciblebox_next_protocol::validate_response(&value.to_string(), &request.request_id)
        .map_err(str::to_owned)?;
    Ok(value)
}
fn trusted_origin(label: &str, main_url: &url::Url, origin: Option<&str>) -> bool {
    label == "main"
        && origin
            .and_then(|value| url::Url::parse(value).ok())
            .is_some_and(|value| {
                !matches!(value.origin(), url::Origin::Opaque(_))
                    && value.origin() == main_url.origin()
            })
}
impl Gateway {
    /// The authoritative protocol token must already be disposed.
    pub fn forget_session(&mut self, token: &str) {
        self.sessions.remove(token);
    }
    #[cfg(test)]
    fn handle(
        &mut self,
        request: &Request,
        raw: &str,
        metadata: &RendererSession,
        db: &Db,
        now_ms: u64,
    ) -> Result<Value, String> {
        let record = db
            .plugin_backend_record(&metadata.plugin_id)
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .map(|row| (row.enabled, row.permissions));
        self.handle_with_storage(request, raw, metadata, record, db, now_ms)
    }
    #[cfg(test)]
    fn handle_with_storage(
        &mut self,
        request: &Request,
        raw: &str,
        metadata: &RendererSession,
        record: Option<(bool, String)>,
        storage: &impl cruciblebox_next_protocol::runtime::Storage,
        now_ms: u64,
    ) -> Result<Value, String> {
        let session = match self.session_for(request, metadata, record, now_ms) {
            Ok(session) => session,
            Err(code) => return denial(request, &code),
        };
        let Ok(mut session) = session.try_lock() else {
            return denial(request, "BUSY");
        };
        let response = dispatch_response(&mut session, storage, raw, "main", now_ms)?;
        serde_json::to_value(response).map_err(|_| "INTERNAL_ERROR".into())
    }
    fn session_for(
        &mut self,
        request: &Request,
        metadata: &RendererSession,
        record: Option<(bool, String)>,
        now_ms: u64,
    ) -> Result<Arc<Mutex<Session>>, String> {
        self.sessions.retain(|_, (expires, _)| *expires > now_ms);
        if metadata.renderer_api_version != 5
            || metadata.state != "active"
            || metadata.token != request.session
            || metadata.owner_webview_label != "main"
        {
            return Err("SESSION_DENIED".into());
        }
        if now_ms >= metadata.expires_at_ms {
            return Err("SESSION_EXPIRED".into());
        }
        let valid = record.is_some_and(|(enabled, permissions)| {
            enabled
                && serde_json::from_str::<Vec<String>>(&permissions)
                    .is_ok_and(|permissions| permissions == metadata.permissions)
        });
        if !valid {
            if let Some(entry) = self.sessions.get_mut(&metadata.token) {
                entry.1 = None;
            } else if self.sessions.len() < 32 {
                self.sessions
                    .insert(metadata.token.clone(), (metadata.expires_at_ms, None));
            }
            return Err("SESSION_DENIED".into());
        }
        if !self.sessions.contains_key(&metadata.token) {
            if self.sessions.len() >= 32 {
                return Err("BUSY".into());
            }
            self.sessions.insert(
                metadata.token.clone(),
                (
                    metadata.expires_at_ms,
                    Some(Arc::new(Mutex::new(Session::new(
                        metadata.token.clone(),
                        "main".into(),
                        metadata.plugin_id.clone(),
                        metadata.expires_at_ms,
                        metadata.permissions.clone(),
                    )))),
                ),
            );
        }
        self.sessions
            .get(&metadata.token)
            .and_then(|entry| entry.1.clone())
            .ok_or_else(|| "SESSION_DENIED".into())
    }
}

/// Separate Next issuer; legacy renderer creation keeps its existing protocol.
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
    if manifest.manifest_version != Some(5) || manifest.name != name || manifest.renderer != entry {
        return Err("SESSION_DENIED".into());
    }
    crate::manifest::validate_entrypoints(root, &manifest)?;
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
                renderer_api_version: 5,
                permissions,
                owner_webview_label: "main".into(),
            },
            &raw,
            lease_ms.unwrap_or(cruciblebox_next_protocol::LEASE_MS),
        )?;
    Ok(crate::plugin_protocol::session_dto(&session))
}

#[tauri::command(async)]
pub fn next_renderer_request(
    window: WebviewWindow,
    caller: tauri::ipc::Request<'_>,
    db: State<'_, Arc<Mutex<Db>>>,
    protocol: State<'_, Arc<crate::plugin_protocol::ProtocolContext>>,
    gateway: State<'_, Mutex<Gateway>>,
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
    let request = cruciblebox_next_protocol::validate_typed_request(&raw).map_err(str::to_owned)?;
    let access = protocol
        .registry
        .lock()
        .map_err(|_| "INTERNAL_ERROR")?
        .get_active(&request.session, "main");
    let Some(metadata) = access.session.filter(|_| access.ok) else {
        return denial(
            &request,
            if access.reason == Some(crate::plugin_session::DenialReason::Expired) {
                "SESSION_EXPIRED"
            } else {
                "SESSION_DENIED"
            },
        );
    };
    let Ok(mut gateway) = gateway.try_lock() else {
        return denial(&request, "BUSY");
    };
    let db_state = db.inner().clone();
    let Ok(db) = db.try_lock() else {
        return denial(&request, "BUSY");
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "INTERNAL_ERROR")?
        .as_millis() as u64;
    let record = db
        .plugin_backend_record(&metadata.plugin_id)
        .map_err(|_| "STORAGE_UNAVAILABLE")?
        .map(|row| (row.enabled, row.permissions));
    drop(db);
    let storage = crate::next_backend::Adapter {
        db: db_state,
        backend: Some(backend.inner().clone()),
        services: backend.services(),
    };
    let session = match gateway.session_for(&request, &metadata, record, now) {
        Ok(session) => session,
        Err(code) => return denial(&request, &code),
    };
    drop(gateway);
    // Per-session execution never holds the global renderer gateway across worker I/O.
    let Ok(mut session) = session.try_lock() else {
        return denial(&request, "BUSY");
    };
    let response = dispatch_response(&mut session, &storage, &raw, "main", now)?;
    serde_json::to_value(response).map_err(|_| "INTERNAL_ERROR".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_main_frame_origin_is_admitted() {
        let main = url::Url::parse("http://tauri.localhost/index.html").unwrap();
        assert!(trusted_origin(
            "main",
            &main,
            Some("http://tauri.localhost")
        ));
        for origin in [
            None,
            Some("null"),
            Some("http://cruciblebox-plugin.localhost"),
            Some("http://evil.invalid"),
        ] {
            assert!(!trusted_origin("main", &main, origin));
        }
        assert!(!trusted_origin(
            "plugin",
            &main,
            Some("http://tauri.localhost")
        ));
    }
    #[test]
    fn legacy_tokens_are_denied_before_storage_and_revoked_grants_stay_denied() {
        let dir = std::env::temp_dir().join(format!(
            "next-gate-{}",
            crate::rand_token::random_token_alnum(12).unwrap()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("db.sqlite");
        {
            let db = Db::open(&path).unwrap();
            db.conn().lock().unwrap().execute_batch(r#"INSERT INTO plugins(id,name,version,display_name,entry_main,installed_path,enabled,permissions)
                VALUES('a','a','1.0.0','A','','unused',1,'["storage:read","storage:write"]');"#).unwrap();
            let mut registry = crate::plugin_session::RendererSessionRegistry::new(
                crate::plugin_session::DEFAULT_TTL,
            );
            let mut meta = registry
                .create(crate::plugin_session::CreateSessionInput {
                    initial_background: "#000".into(),
                    color_scheme: "dark".into(),
                    plugin_id: "a".into(),
                    plugin_name: "a".into(),
                    plugin_directory: "unused".into(),
                    renderer_entry: "dist/renderer.js".into(),
                    runtime_path: "unused".into(),
                    renderer_api_version: 2,
                    permissions: vec!["storage:read".into(), "storage:write".into()],
                    owner_webview_label: "main".into(),
                })
                .unwrap();
            meta.state = "active";
            let raw=json!({"wireVersion":3,"requestId":"r-1","session":meta.token,"method":"storage.set","params":{"key":"note.v1","value":"saved"}}).to_string();
            let request = cruciblebox_next_protocol::validate_typed_request(&raw).unwrap();
            let mut gateway = Gateway::default();
            assert_eq!(
                gateway.handle(&request, &raw, &meta, &db, 1).unwrap()["error"]["code"],
                "SESSION_DENIED"
            );
            assert!(db.storage_get("a", "note.v1").unwrap().is_none());
            // Trusted test metadata simulates a future verified Next issuer, not a real installation.
            meta.renderer_api_version = 5;
            assert_eq!(
                gateway.handle(&request, &raw, &meta, &db, 1).unwrap()["ok"],
                true
            );
            let first = gateway
                .session_for(
                    &request,
                    &meta,
                    Some((true, serde_json::to_string(&meta.permissions).unwrap())),
                    1,
                )
                .unwrap();
            let held = first.lock().unwrap();
            let mut other = meta.clone();
            other.token = "b".repeat(64);
            let ping = json!({"wireVersion":3,"requestId":"r-other","session":other.token,"method":"runtime.ping","params":{}}).to_string();
            let ping_request = cruciblebox_next_protocol::validate_typed_request(&ping).unwrap();
            assert_eq!(
                gateway
                    .handle(&ping_request, &ping, &other, &db, 1)
                    .unwrap()["result"],
                "pong"
            );
            assert_eq!(
                gateway.handle(&request, &raw, &meta, &db, 1).unwrap()["error"]["code"],
                "BUSY"
            );
            drop(held);
            db.conn()
                .lock()
                .unwrap()
                .execute("UPDATE plugins SET enabled=0 WHERE id='a'", [])
                .unwrap();
            assert_eq!(
                gateway.handle(&request, &raw, &meta, &db, 1).unwrap()["error"]["code"],
                "SESSION_DENIED"
            );
            db.conn()
                .lock()
                .unwrap()
                .execute("UPDATE plugins SET enabled=1 WHERE id='a'", [])
                .unwrap();
            assert_eq!(
                gateway.handle(&request, &raw, &meta, &db, 1).unwrap()["error"]["code"],
                "SESSION_DENIED"
            );
            assert_eq!(
                db.storage_get("a", "note.v1").unwrap().unwrap(),
                "\"saved\""
            );
        }
        std::fs::remove_file(&path).unwrap();
        for name in ["db.sqlite-wal", "db.sqlite-shm"] {
            let p = dir.join(name);
            if p.is_file() {
                std::fs::remove_file(p).unwrap();
            }
        }
        std::fs::remove_dir(dir).unwrap();
    }
}

// 插件 renderer session registry（1.8.3，对等 plugin-system/PluginRendererSessionRegistry.ts）
// 语义对齐 TS 版：token/handshakeToken 独立签发（64 hex）、owner 绑定、TTL 过期、
// 一次性 index 消费（issued → active）、expiry 清理。origin 按 Tauri Windows 形式：
// http://cruciblebox-plugin.localhost/<token>/index.html（path 型，见 PoC 结论）。

use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PLUGIN_RENDERER_SCHEME: &str = "cruciblebox-plugin";
pub const DEFAULT_TTL: Duration = Duration::from_secs(30 * 60);
pub const MAX_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Issued,
    Active,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererSession {
    pub token: String,
    pub handshake_token: String,
    pub origin: String,
    pub index_url: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_directory: String,
    pub renderer_entry: String,
    pub renderer_path: String,
    pub runtime_path: String,
    pub renderer_api_version: u8,
    pub permissions: Vec<String>,
    pub owner_webview_label: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub state: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DenialReason {
    InvalidToken,
    NotFound,
    Expired,
    OwnerMismatch,
    #[allow(dead_code)]
    NotActive,
    AlreadyConsumed,
}

pub struct SessionAccess {
    pub ok: bool,
    pub session: Option<RendererSession>,
    pub reason: Option<DenialReason>,
}

pub struct CreateSessionInput {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_directory: String,
    pub renderer_entry: String,
    pub runtime_path: String,
    pub renderer_api_version: u8,
    pub permissions: Vec<String>,
    pub owner_webview_label: String,
}

pub struct RendererSessionRegistry {
    sessions: HashMap<String, SessionInternal>,
    ttl: Duration,
}

struct SessionInternal {
    handshake_token: String,
    origin: String,
    index_url: String,
    plugin_id: String,
    plugin_name: String,
    plugin_directory: String,
    renderer_entry: String,
    renderer_path: String,
    runtime_path: String,
    renderer_api_version: u8,
    permissions: Vec<String>,
    owner_webview_label: String,
    created_at_ms: u64,
    expires_at_ms: u64,
    state: SessionState,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 64 位小写 hex 随机 token（对等 randomBytes(32).toString('hex')）
fn random_token() -> Result<String, String> {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).map_err(|e| format!("rng failure: {e}"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit())
}

impl RendererSessionRegistry {
    pub fn new(ttl: Duration) -> Self {
        let ttl = if ttl.is_zero() || ttl > MAX_TTL {
            DEFAULT_TTL
        } else {
            ttl
        };
        RendererSessionRegistry {
            sessions: HashMap::new(),
            ttl,
        }
    }

    pub fn create(&mut self, input: CreateSessionInput) -> Result<RendererSession, String> {
        if input.plugin_id.is_empty() || input.plugin_id != input.plugin_id.trim() {
            return Err("pluginId must be a non-empty trimmed string".into());
        }
        if input.renderer_api_version != 1 && input.renderer_api_version != 2 {
            return Err("rendererApiVersion must be 1 or 2".into());
        }
        if !input.renderer_entry.ends_with(".js")
            || input.renderer_entry.contains('\\')
            || input.renderer_entry.starts_with('/')
            || input
                .renderer_entry
                .split('/')
                .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        {
            return Err("rendererEntry must be a normalized relative JavaScript path".into());
        }

        let token = random_token()?;
        let handshake_token = random_token()?;
        // Tauri Windows 自定义协议形式：http://<scheme>.localhost/<token>/index.html
        // （PoC 结论：scheme:// 形式不被支持；path 型携带 session token）
        let origin = format!("http://{}.localhost/{}", PLUGIN_RENDERER_SCHEME, token);
        let index_url = format!("{}/index.html", origin);
        let created_at = now_ms();
        let expires_at = created_at + self.ttl.as_millis() as u64;

        let renderer_path = format!(
            "{}/{}",
            input.plugin_directory.trim_end_matches(['/', '\\']),
            input.renderer_entry
        );
        let session = SessionInternal {
            handshake_token,
            origin,
            index_url,
            plugin_id: input.plugin_id.clone(),
            plugin_name: input.plugin_name,
            plugin_directory: input.plugin_directory,
            renderer_entry: input.renderer_entry,
            renderer_path,
            runtime_path: input.runtime_path,
            renderer_api_version: input.renderer_api_version,
            permissions: input.permissions,
            owner_webview_label: input.owner_webview_label,
            created_at_ms: created_at,
            expires_at_ms: expires_at,
            state: SessionState::Issued,
        };
        self.sessions.insert(token.clone(), session);
        self.snapshot(&token)
            .session
            .ok_or_else(|| "session creation failed".into())
    }

    /// 一次性消费 index（issued → active）。对等 consumeIndex。
    pub fn consume_index(&mut self, token: &str, owner_webview_label: &str) -> SessionAccess {
        let access = self.access(token, owner_webview_label);
        if !access.ok {
            return access;
        }
        let session = match self.sessions.get_mut(token) {
            Some(s) => s,
            None => return Self::deny(DenialReason::NotFound),
        };
        if session.state == SessionState::Active {
            return Self::deny(DenialReason::AlreadyConsumed);
        }
        session.state = SessionState::Active;
        self.snapshot(token)
    }

    /// 供 1.8.4 宿主集成使用（当前协议层仅用 get/consume/dispose）
    #[allow(dead_code)]
    pub fn get_active(&self, token: &str, owner_webview_label: &str) -> SessionAccess {
        let access = self.access(token, owner_webview_label);
        if !access.ok {
            return access;
        }
        if let Some(s) = self.sessions.get(token) {
            if s.state != SessionState::Active {
                return Self::deny(DenialReason::NotActive);
            }
        }
        access
    }

    pub fn get(&self, token: &str, owner_webview_label: &str) -> SessionAccess {
        self.access(token, owner_webview_label)
    }

    pub fn dispose(&mut self, token: &str) -> bool {
        if !valid_token(token) {
            return false;
        }
        self.sessions.remove(token).is_some()
    }

    #[allow(dead_code)]
    pub fn dispose_owner(&mut self, owner_webview_label: &str) -> usize {
        let keys: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.owner_webview_label == owner_webview_label)
            .map(|(k, _)| k.clone())
            .collect();
        let n = keys.len();
        for k in keys {
            self.sessions.remove(&k);
        }
        n
    }

    #[allow(dead_code)]
    pub fn cleanup_expired(&mut self) -> usize {
        let now = now_ms();
        let keys: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| now >= s.expires_at_ms)
            .map(|(k, _)| k.clone())
            .collect();
        let n = keys.len();
        for k in keys {
            self.sessions.remove(&k);
        }
        n
    }

    fn access(&self, token: &str, owner_webview_label: &str) -> SessionAccess {
        if !valid_token(token) {
            return Self::deny(DenialReason::InvalidToken);
        }
        let session = match self.sessions.get(token) {
            Some(s) => s,
            None => return Self::deny(DenialReason::NotFound),
        };
        if now_ms() >= session.expires_at_ms {
            return Self::deny(DenialReason::Expired);
        }
        if session.owner_webview_label != owner_webview_label {
            return Self::deny(DenialReason::OwnerMismatch);
        }
        self.snapshot(token)
    }

    fn snapshot(&self, token: &str) -> SessionAccess {
        match self.sessions.get(token) {
            Some(s) => SessionAccess {
                ok: true,
                session: Some(RendererSession {
                    token: token.to_string(),
                    handshake_token: s.handshake_token.clone(),
                    origin: s.origin.clone(),
                    index_url: s.index_url.clone(),
                    plugin_id: s.plugin_id.clone(),
                    plugin_name: s.plugin_name.clone(),
                    plugin_directory: s.plugin_directory.clone(),
                    renderer_entry: s.renderer_entry.clone(),
                    renderer_path: s.renderer_path.clone(),
                    runtime_path: s.runtime_path.clone(),
                    renderer_api_version: s.renderer_api_version,
                    permissions: s.permissions.clone(),
                    owner_webview_label: s.owner_webview_label.clone(),
                    created_at_ms: s.created_at_ms,
                    expires_at_ms: s.expires_at_ms,
                    state: if s.state == SessionState::Active {
                        "active"
                    } else {
                        "issued"
                    },
                }),
                reason: None,
            },
            None => Self::deny(DenialReason::NotFound),
        }
    }

    fn deny(reason: DenialReason) -> SessionAccess {
        SessionAccess {
            ok: false,
            session: None,
            reason: Some(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(plugin_id: &str) -> CreateSessionInput {
        CreateSessionInput {
            plugin_id: plugin_id.to_string(),
            plugin_name: plugin_id.to_string(),
            plugin_directory: "C:/plugins/x".into(),
            renderer_entry: "dist/renderer.js".into(),
            runtime_path: "C:/app/out/plugin-frame/runtime.js".into(),
            renderer_api_version: 2,
            permissions: vec!["theme:read".into()],
            owner_webview_label: "main".into(),
        }
    }

    #[test]
    fn create_and_consume_index() {
        let mut reg = RendererSessionRegistry::new(DEFAULT_TTL);
        let session = reg.create(make_input("diary")).unwrap();
        assert_eq!(session.token.len(), 64);
        assert_eq!(session.handshake_token.len(), 64);
        assert!(session
            .index_url
            .contains("http://cruciblebox-plugin.localhost"));
        assert!(session.index_url.ends_with("/index.html"));
        assert_eq!(session.state, "issued");

        // 一次性消费
        let access = reg.consume_index(&session.token, "main");
        assert!(access.ok);
        assert_eq!(access.session.unwrap().state, "active");
        // 二次消费拒绝
        let again = reg.consume_index(&session.token, "main");
        assert!(!again.ok);
        assert_eq!(again.reason, Some(DenialReason::AlreadyConsumed));
    }

    #[test]
    fn owner_mismatch_rejected() {
        let mut reg = RendererSessionRegistry::new(DEFAULT_TTL);
        let session = reg.create(make_input("diary")).unwrap();
        let access = reg.get(&session.token, "other-window");
        assert!(!access.ok);
        assert_eq!(access.reason, Some(DenialReason::OwnerMismatch));
    }

    #[test]
    fn invalid_and_unknown_tokens() {
        let reg = RendererSessionRegistry::new(DEFAULT_TTL);
        assert_eq!(
            reg.get("short", "main").reason,
            Some(DenialReason::InvalidToken)
        );
        assert_eq!(
            reg.get(&"a".repeat(64), "main").reason,
            Some(DenialReason::NotFound)
        );
    }

    #[test]
    fn dispose_and_cleanup() {
        let mut reg = RendererSessionRegistry::new(DEFAULT_TTL);
        let session = reg.create(make_input("diary")).unwrap();
        assert!(reg.dispose(&session.token));
        assert!(!reg.dispose(&session.token));
        assert_eq!(reg.sessions.len(), 0);
    }

    #[test]
    fn expiry_enforced() {
        let mut reg = RendererSessionRegistry::new(Duration::from_millis(1));
        let session = reg.create(make_input("diary")).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let access = reg.get(&session.token, "main");
        assert!(!access.ok);
        assert_eq!(access.reason, Some(DenialReason::Expired));
    }
}

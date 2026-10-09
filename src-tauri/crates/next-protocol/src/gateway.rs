//! Host-owned admission state. Plugin requests never choose a storage owner.
use crate::{required_capability, validate_typed_request, Request};
use std::collections::HashSet;

pub struct Session {
    token: String,
    owner: String,
    plugin_id: String,
    expires_at_ms: u64,
    capabilities: HashSet<String>,
    seen: HashSet<String>,
    inflight: HashSet<String>,
}

pub struct Admitted {
    pub plugin_id: String,
    pub request: Request,
}

impl Session {
    /// All arguments come from verified installation/session records, never RPC params.
    pub fn new(
        token: String,
        owner: String,
        plugin_id: String,
        expires_at_ms: u64,
        capabilities: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            token,
            owner,
            plugin_id,
            expires_at_ms,
            capabilities: capabilities.into_iter().collect(),
            seen: HashSet::new(),
            inflight: HashSet::new(),
        }
    }

    pub fn admit(
        &mut self,
        raw: &str,
        caller: &str,
        now_ms: u64,
    ) -> Result<Admitted, &'static str> {
        let request = validate_typed_request(raw)?;
        if caller != self.owner || request.session != self.token {
            return Err("SESSION_DENIED");
        }
        if now_ms >= self.expires_at_ms {
            return Err("SESSION_EXPIRED");
        }
        let method = serde_json::to_value(&request).map_err(|_| "INVALID_REQUEST")?;
        let capability = required_capability(method["method"].as_str().ok_or("INVALID_REQUEST")?)
            .ok_or("INVALID_REQUEST")?;
        if capability.is_some_and(|cap| !self.capabilities.contains(&cap)) {
            return Err("PERMISSION_DENIED");
        }
        if self.seen.contains(&request.request_id) {
            return Err("REPLAY_DENIED");
        }
        if self.inflight.len() >= crate::generated::MAX_INFLIGHT {
            return Err("BUSY");
        }
        // Never evict replay IDs while a token remains valid. Rotate the session instead.
        if self.seen.len() >= 4096 {
            return Err("SESSION_EXHAUSTED");
        }
        self.seen.insert(request.request_id.clone());
        self.inflight.insert(request.request_id.clone());
        Ok(Admitted {
            plugin_id: self.plugin_id.clone(),
            request,
        })
    }

    /// Called on success, failure, cancellation or timeout by the host dispatcher.
    pub fn finish(&mut self, request_id: &str) -> bool {
        self.inflight.remove(request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session(caps: &[&str]) -> Session {
        Session::new(
            "a".repeat(64),
            "main".into(),
            "note".into(),
            100,
            caps.iter().map(|s| s.to_string()),
        )
    }
    fn raw(id: usize, method: &str, params: serde_json::Value) -> String {
        serde_json::json!({"wireVersion":3,"requestId":format!("r-{id}"),
            "session":"a".repeat(64),"method":method,"params":params})
        .to_string()
    }
    #[test]
    fn host_owner_expiry_permissions_and_namespace() {
        let mut s = session(&["storage:write"]);
        let request = raw(
            1,
            "storage.set",
            serde_json::json!({"key":"note.v1","value":null}),
        );
        assert_eq!(s.admit(&request, "other", 1).err(), Some("SESSION_DENIED"));
        assert_eq!(
            s.admit(&request, "main", 100).err(),
            Some("SESSION_EXPIRED")
        );
        let admitted = s.admit(&request, "main", 1).unwrap();
        assert_eq!(admitted.plugin_id, "note");
        let get = raw(2, "storage.get", serde_json::json!({"key":"note.v1"}));
        assert_eq!(s.admit(&get, "main", 1).err(), Some("PERMISSION_DENIED"));
        assert_eq!(s.admit(&request, "main", 1).err(), Some("REPLAY_DENIED"));
        assert!(s.finish("r-1"));
        assert!(!s.finish("r-1"));
        assert_eq!(s.admit(&request, "main", 1).err(), Some("REPLAY_DENIED"));
    }
    #[test]
    fn bounded_admission_releases_failure_slot_without_replay_eviction() {
        let mut s = session(&[]);
        for id in 0..32 {
            s.admit(&raw(id, "runtime.ping", serde_json::json!({})), "main", 1)
                .unwrap();
        }
        let pending = raw(32, "runtime.ping", serde_json::json!({}));
        assert_eq!(s.admit(&pending, "main", 1).err(), Some("BUSY"));
        assert!(s.finish("r-0"));
        s.admit(&pending, "main", 1).unwrap();
        assert_eq!(
            s.admit(&raw(0, "runtime.ping", serde_json::json!({})), "main", 1)
                .err(),
            Some("REPLAY_DENIED")
        );
    }
}

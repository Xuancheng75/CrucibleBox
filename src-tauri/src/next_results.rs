//! Bounded immutable trusted-service results, scoped to owner and exact session.
use base64::Engine;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
struct Snapshot {
    owner: String,
    session: String,
    method: String,
    bytes: Vec<u8>,
    expires: Instant,
}
#[derive(Default)]
pub struct Results {
    values: Mutex<HashMap<String, Snapshot>>,
}
impl Results {
    pub fn open(
        &self,
        owner: &str,
        session: &str,
        method: &str,
        value: &Value,
    ) -> Result<Value, String> {
        let raw = serde_json::to_string(value).map_err(|_| "INVALID_RESPONSE")?;
        cruciblebox_next_protocol::validate_storage_value(&raw).map_err(str::to_owned)?;
        let mut values = self.values.try_lock().map_err(|_| "BUSY")?;
        values.retain(|_, value| value.expires > Instant::now());
        if values.len() >= 32
            || values.values().filter(|v| v.owner == owner).count() >= 4
            || values.values().map(|v| v.bytes.len()).sum::<usize>() + raw.len() > 64 * 1024 * 1024
        {
            return Err("BUSY".into());
        }
        let id = crate::rand_token::random_token_hex()?;
        let length = raw.len();
        values.insert(
            id.clone(),
            Snapshot {
                owner: owner.into(),
                session: session.into(),
                method: method.into(),
                bytes: raw.into_bytes(),
                expires: Instant::now() + Duration::from_secs(90),
            },
        );
        Ok(json!({"$nextResult":{"readId":id,"byteLength":length}}))
    }
    pub fn method(&self, owner: &str, session: &str, id: &str) -> Result<String, String> {
        let values = self.values.try_lock().map_err(|_| "BUSY")?;
        let value = values
            .get(id)
            .filter(|v| v.owner == owner && v.session == session && v.expires > Instant::now())
            .ok_or("SESSION_DENIED")?;
        Ok(value.method.clone())
    }
    pub fn chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        offset: u64,
    ) -> Result<Value, String> {
        let values = self.values.try_lock().map_err(|_| "BUSY")?;
        let value = values
            .get(id)
            .filter(|v| v.owner == owner && v.session == session && v.expires > Instant::now())
            .ok_or("SESSION_DENIED")?;
        let offset = usize::try_from(offset).map_err(|_| "INVALID_REQUEST")?;
        if offset >= value.bytes.len() {
            return Err("INVALID_REQUEST".into());
        }
        let end =
            (offset + cruciblebox_next_protocol::MAX_STORAGE_CHUNK_BYTES).min(value.bytes.len());
        Ok(
            json!({"offset":offset,"data":base64::engine::general_purpose::STANDARD.encode(&value.bytes[offset..end])}),
        )
    }
    pub fn close(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        let mut values = self.values.try_lock().map_err(|_| "BUSY")?;
        if values
            .get(id)
            .is_some_and(|v| v.owner != owner || v.session != session)
        {
            return Err("SESSION_DENIED".into());
        }
        values.remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_results_are_scoped_bounded_and_expire_without_reusing_handles() {
        let store = Results::default();
        let data = json!({"text":"中文".repeat(20000)});
        let opened = store
            .open("document-engine", "session-a", "document.call", &data)
            .unwrap();
        let id = opened["$nextResult"]["readId"].as_str().unwrap();
        assert!(store.chunk("diary", "session-a", id, 0).is_err());
        assert!(store.chunk("document-engine", "session-b", id, 0).is_err());
        assert!(store.close("diary", "session-a", id).is_err());
        let raw = serde_json::to_vec(&data).unwrap();
        let mut actual = Vec::new();
        while actual.len() < raw.len() {
            let page = store
                .chunk("document-engine", "session-a", id, actual.len() as u64)
                .unwrap();
            actual.extend(
                base64::engine::general_purpose::STANDARD
                    .decode(page["data"].as_str().unwrap())
                    .unwrap(),
            );
        }
        assert_eq!(actual, raw);
        assert!(store
            .chunk("document-engine", "session-a", id, raw.len() as u64)
            .is_err());
        for _ in 0..3 {
            store
                .open("document-engine", "session-a", "document.call", &data)
                .unwrap();
        }
        assert_eq!(
            store
                .open("document-engine", "session-a", "document.call", &data)
                .unwrap_err(),
            "BUSY"
        );
        store.values.lock().unwrap().get_mut(id).unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        assert!(store.method("document-engine", "session-a", id).is_err());
        let fresh = store
            .open("document-engine", "session-a", "document.call", &data)
            .unwrap();
        assert_ne!(fresh["$nextResult"]["readId"], id);
        assert!(store
            .open(
                "diary",
                "session-b",
                "document.call",
                &json!({"text":"x".repeat(4194304)})
            )
            .is_err());
    }
}

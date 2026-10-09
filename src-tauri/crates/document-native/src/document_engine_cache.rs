//! Small file-backed cache/model helpers for Document Engine.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const CACHE_SCHEMA_VERSION: u32 = 1;

pub fn cache_key(source_hash: &str, engine: &str, engine_version: &str, options: &Value) -> String {
    let payload = json!({
        "sourceHash": source_hash,
        "engine": engine,
        "engineVersion": engine_version,
        "options": options,
    });
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&payload).unwrap_or_default());
    format!("{:x}", digest.finalize())
}

fn cache_path(root: &Path, key: &str) -> Result<PathBuf, String> {
    if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("缓存 key 格式无效".into());
    }
    Ok(root.join(format!("{key}.json")))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

/// Read a cache entry. Invalid or stale entries are treated as misses and are
/// removed so a corrupt cache can never poison a successful task.
pub fn read_result(root: &Path, key: &str) -> Result<Option<Value>, String> {
    let path = cache_path(root, key)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let envelope: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }
    };
    if envelope["schemaVersion"].as_u64() != Some(u64::from(CACHE_SCHEMA_VERSION))
        || envelope["cacheKey"].as_str() != Some(key)
    {
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }
    Ok(envelope.get("result").cloned())
}

/// Atomically write a cache entry. A temporary sibling file prevents a crash
/// from leaving a partially-written JSON object visible to another task.
pub fn write_result(root: &Path, key: &str, result: &Value) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| format!("创建缓存目录失败: {error}"))?;
    let path = cache_path(root, key)?;
    let temp = path.with_extension("json.tmp");
    let envelope = json!({
        "schemaVersion": CACHE_SCHEMA_VERSION,
        "cacheKey": key,
        "createdAt": now_ms(),
        "result": result,
    });
    let bytes =
        serde_json::to_vec(&envelope).map_err(|error| format!("序列化缓存失败: {error}"))?;
    std::fs::write(&temp, bytes).map_err(|error| format!("写入缓存失败: {error}"))?;
    if let Err(error) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("提交缓存失败: {error}"));
    }
    Ok(())
}

//! Owner-bound file grants. Native code remains trusted local code, not an OS sandbox.
use base64::Engine;
use cruciblebox_next2_protocol::FileReference;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
};

struct Grant {
    owner: String,
    path: PathBuf,
}
#[derive(Default)]
pub struct Files(Mutex<HashMap<String, Grant>>);
impl Files {
    pub fn grant(&self, owner: &str, path: &Path) -> Result<FileReference, String> {
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("INVALID_FILE_NAME")?;
        if name.chars().count() > 240 || metadata.len() > 16 * 1024 * 1024 * 1024 {
            return Err("FILE_BUDGET_EXCEEDED".into());
        }
        let mut grants = self.0.lock().map_err(|_| "FILE_GRANTS_UNAVAILABLE")?;
        if grants.values().filter(|grant| grant.owner == owner).count() >= 512 {
            return Err("FILE_GRANT_LIMIT".into());
        }
        let reference = FileReference {
            r#ref: crate::rand_token::random_token_hex()?,
            name: name.into(),
            bytes: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        };
        grants.insert(
            reference.r#ref.clone(),
            Grant {
                owner: owner.into(),
                path,
            },
        );
        Ok(reference)
    }
    pub fn resolve(
        &self,
        owner: &str,
        caller: Option<&str>,
        reference: &str,
    ) -> Result<PathBuf, String> {
        let grants = self.0.lock().map_err(|_| "FILE_GRANTS_UNAVAILABLE")?;
        let grant = grants.get(reference).ok_or("FILE_REFERENCE_EXPIRED")?;
        if grant.owner != owner && caller != Some(grant.owner.as_str()) {
            return Err("FILE_AUTHORITY_DENIED".into());
        }
        Ok(grant.path.clone())
    }
    pub fn clear(&self, owner: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "FILE_GRANTS_UNAVAILABLE")?
            .retain(|_, grant| grant.owner != owner);
        Ok(())
    }
}
pub fn read(path: &Path, offset: u64, length: u64) -> Result<serde_json::Value, String> {
    let mut input = fs::File::open(path).map_err(|e| e.to_string())?;
    if !input.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("FILE_REQUIRED".into());
    }
    input
        .seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    input
        .by_ref()
        .take(length + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let eof = bytes.len() as u64 <= length;
    bytes.truncate(length as usize);
    Ok(
        serde_json::json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes), "eof":eof}),
    )
}

pub fn fetch(params: &serde_json::Value) -> Result<serde_json::Value, String> {
    let url = params["url"].as_str().unwrap();
    let method = params["method"].as_str().unwrap();
    let (agent, _) = crate::network_policy::current().agent_for_url(url, 10, 20)?;
    let mut request = agent.request(method, url);
    if let Some(headers) = params["headers"].as_object() {
        for (name, value) in headers {
            request = request.set(
                name,
                value.as_str().ok_or("HTTP_HEADER_VALUE_MUST_BE_STRING")?,
            );
        }
    }
    let response = if let Some(body) = params["body"].as_str() {
        request.send_string(body)
    } else {
        request.call()
    };
    let response = match response {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(error) => return Err(error.to_string()),
    };
    let status = response.status();
    let mut headers = serde_json::Map::new();
    for name in response.headers_names() {
        if let Some(value) = response.header(&name) {
            headers.insert(name, value.into());
        }
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(32769)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() > 32768 {
        return Err("HTTP_RESPONSE_TOO_LARGE: use the native backend for large transfers".into());
    }
    let body = String::from_utf8(body).map_err(|_| "HTTP_RESPONSE_REQUIRES_UTF8")?;
    Ok(serde_json::json!({"status":status,"headers":headers,"body":body}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_grants_bind_owner_and_immediate_host_authenticated_caller() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("原文.txt");
        fs::write(&path, "中文🌏").unwrap();
        let files = Files::default();
        let reference = files.grant("client", &path).unwrap();
        assert!(files.resolve("other", None, &reference.r#ref).is_err());
        let resolved = files
            .resolve("sample-provider", Some("client"), &reference.r#ref)
            .unwrap();
        let first = read(&resolved, 0, 3).unwrap();
        assert_eq!(first["data"], "5Lit");
        assert_eq!(first["eof"], false);
        assert_eq!(read(&resolved, 3, 24).unwrap()["eof"], true);
        files.clear("client").unwrap();
        assert!(files.resolve("client", None, &reference.r#ref).is_err());
    }
}

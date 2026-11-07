//! Owner-bound file grants. Native code remains trusted local code, not an OS sandbox.
use base64::Engine;
use cruciblebox_next2_protocol::FileReference;
use std::{
    collections::HashMap,
    fs::{self, File},
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
    let mut input = open_authorized(path)?;
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

fn metadata_value(path: &Path, metadata: &fs::Metadata) -> Result<serde_json::Value, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("INVALID_FILE_NAME")?;
    if name.chars().count() > 240
        || (metadata.is_file() && metadata.len() > 16 * 1024 * 1024 * 1024)
    {
        return Err("FILE_BUDGET_EXCEEDED".into());
    }
    let (kind, bytes) = if metadata.is_dir() {
        ("directory", 0)
    } else {
        ("file", metadata.len())
    };
    Ok(serde_json::json!({"name":name,"kind":kind,"bytes":bytes}))
}

pub fn info(path: &Path) -> Result<serde_json::Value, String> {
    let file = open_authorized(path)?;
    metadata_value(path, &file.metadata().map_err(|error| error.to_string())?)
}

pub fn list(path: &Path) -> Result<serde_json::Value, String> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let directory = open_authorized(path)?;
    if !directory
        .metadata()
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("DIRECTORY_REQUIRED".into());
    }
    let mut entries = Vec::new();
    for (index, entry) in fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .enumerate()
    {
        if index == 128 {
            return Err("DIRECTORY_LIST_LIMIT".into());
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
            entries.push(metadata_value(&entry.path(), &metadata)?);
        }
    }
    entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(serde_json::json!(entries))
}

pub(super) fn open_authorized(path: &Path) -> Result<File, String> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
    open_with_share(path, FILE_SHARE_READ | FILE_SHARE_WRITE)
}

pub(super) fn open_readonly(path: &Path) -> Result<File, String> {
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    open_with_share(path, FILE_SHARE_READ)
}

fn open_with_share(path: &Path, sharing: u32) -> Result<File, String> {
    use std::{
        ffi::OsString,
        os::windows::{
            ffi::OsStringExt,
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        },
    };
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT,
    };

    // Both selection grants and persisted publication receipts store the
    // canonical path from the time access was granted.
    let file = File::options()
        .read(true)
        .share_mode(sharing)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|error| error.to_string())?;
    if file
        .metadata()
        .map_err(|error| error.to_string())?
        .file_attributes()
        & FILE_ATTRIBUTE_REPARSE_POINT
        != 0
    {
        return Err("FILE_AUTHORITY_DENIED".into());
    }
    let mut buffer = vec![0; 32768];
    let size = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle() as _,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    } as usize;
    if size == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if size >= buffer.len() || Path::new(&OsString::from_wide(&buffer[..size])) != path {
        return Err("FILE_AUTHORITY_DENIED".into());
    }
    Ok(file)
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

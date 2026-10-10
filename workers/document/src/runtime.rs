//! Immutable, digest-pinned document runtime installation. Old versions remain available for rollback.
use crate::protocol::{artifact_reference, Reference, WIRE_VERSION};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: String,
    pub wire_version: u32,
    pub files: BTreeMap<String, Reference>,
}
impl Catalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.wire_version != WIRE_VERSION
            || self.version.is_empty()
            || self.version.len() > 64
            || !self
                .version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
        {
            return Err("INVALID_DOCUMENT_RUNTIME_VERSION".into());
        }
        if self.files.len() != 2
            || !self.files.contains_key("document-worker.exe")
            || !self.files.contains_key("pdfium.dll")
        {
            return Err("INVALID_DOCUMENT_RUNTIME_FILES".into());
        }
        for file in self.files.values() {
            if file.bytes == 0
                || file.bytes > 512 * 1024 * 1024
                || file.sha256.len() != 64
                || !file.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("INVALID_DOCUMENT_RUNTIME_DIGEST".into());
            }
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<String, String> {
        self.validate()?;
        use sha2::{Digest, Sha256};
        Ok(format!(
            "{}-{:x}",
            self.version,
            Sha256::digest(serde_json::to_vec(self).map_err(|e| e.to_string())?)
        ))
    }
}
/// A lease keeps the exact validated version selected for the duration of a task.
pub struct Lease {
    pub executable: PathBuf,
    pub pdfium: PathBuf,
    pub identity: String,
    catalog: Catalog,
    _owner: Arc<Store>,
}
impl Lease {
    pub fn verify(&self) -> Result<(), String> {
        self._owner.verify(
            self.executable.parent().ok_or("DOCUMENT_RUNTIME_PARENT")?,
            &self.catalog,
        )
    }
}
pub struct Store {
    root: PathBuf,
    mutation: Mutex<()>,
}
impl Store {
    pub fn new(root: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            root,
            mutation: Mutex::new(()),
        })
    }
    fn verify(&self, directory: &Path, catalog: &Catalog) -> Result<(), String> {
        catalog.validate()?;
        let meta = fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("DOCUMENT_RUNTIME_LINK".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("DOCUMENT_RUNTIME_LINK".into());
            }
        }
        let entries = fs::read_dir(directory)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if entries.len() != catalog.files.len() {
            return Err("DOCUMENT_RUNTIME_UNEXPECTED_FILES".into());
        }
        for (name, expected) in &catalog.files {
            let actual = artifact_reference(&directory.join(name))?;
            if actual.bytes != expected.bytes || actual.sha256 != expected.sha256 {
                return Err("DOCUMENT_RUNTIME_DIGEST_MISMATCH".into());
            }
        }
        Ok(())
    }
    /// Catalog must come from the host's pinned release catalog, never from the installation source.
    pub fn install(self: &Arc<Self>, source: &Path, catalog: &Catalog) -> Result<Lease, String> {
        let _guard = self
            .mutation
            .lock()
            .map_err(|_| "DOCUMENT_RUNTIME_STORE_POISONED")?;
        self.verify(source, catalog)?;
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let identity = catalog.identity()?;
        let destination = self.root.join(&identity);
        if !destination.exists() {
            let mut random = [0u8; 16];
            getrandom::getrandom(&mut random).map_err(|e| e.to_string())?;
            let token = random
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>();
            let stage = self.root.join(format!(".staging-{token}"));
            fs::create_dir(&stage).map_err(|e| e.to_string())?;
            let installed = (|| {
                for name in catalog.files.keys() {
                    let target = stage.join(name);
                    fs::copy(source.join(name), &target).map_err(|e| e.to_string())?;
                    fs::OpenOptions::new()
                        .write(true)
                        .open(&target)
                        .and_then(|f| f.sync_all())
                        .map_err(|e| e.to_string())?;
                }
                self.verify(&stage, catalog)?;
                fs::rename(&stage, &destination).map_err(|e| e.to_string())
            })();
            if installed.is_err() {
                let _ = fs::remove_dir_all(&stage);
            }
            installed?;
        }
        self.verify(&destination, catalog)?;
        Ok(Lease {
            executable: destination.join("document-worker.exe"),
            pdfium: destination.join("pdfium.dll"),
            identity,
            catalog: catalog.clone(),
            _owner: self.clone(),
        })
    }
    /// Selecting an older pinned catalog restores the paired worker and native library together.
    pub fn lease(self: &Arc<Self>, catalog: &Catalog) -> Result<Lease, String> {
        let identity = catalog.identity()?;
        let directory = self.root.join(&identity);
        self.verify(&directory, catalog)?;
        Ok(Lease {
            executable: directory.join("document-worker.exe"),
            pdfium: directory.join("pdfium.dll"),
            identity,
            catalog: catalog.clone(),
            _owner: self.clone(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source(root: &Path, version: &str, bytes: &[u8]) -> (PathBuf, Catalog) {
        let source = root.join(version);
        fs::create_dir(&source).unwrap();
        fs::write(source.join("document-worker.exe"), bytes).unwrap();
        fs::write(source.join("pdfium.dll"), b"paired native library").unwrap();
        let files = ["document-worker.exe", "pdfium.dll"]
            .into_iter()
            .map(|n| (n.to_owned(), artifact_reference(&source.join(n)).unwrap()))
            .collect();
        (
            source,
            Catalog {
                version: version.into(),
                wire_version: WIRE_VERSION,
                files,
            },
        )
    }
    #[test]
    fn immutable_versions_upgrade_and_paired_rollback_keep_active_lease() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path().join("installed"));
        let (one, c1) = source(root.path(), "1.0.0", b"worker one");
        let first = store.install(&one, &c1).unwrap();
        let (two, c2) = source(root.path(), "1.1.0", b"worker two");
        let second = store.install(&two, &c2).unwrap();
        assert_eq!(fs::read(&first.executable).unwrap(), b"worker one");
        assert_ne!(first.identity, second.identity);
        let rollback = store.lease(&c1).unwrap();
        assert_eq!(rollback.executable, first.executable);
        fs::write(&second.pdfium, b"tampered").unwrap();
        assert!(store.lease(&c2).is_err());
        assert!(store.lease(&c1).is_ok());
    }
    #[test]
    fn corrupt_or_extra_source_files_never_install() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path().join("installed"));
        let (source, catalog) = source(root.path(), "1", b"worker");
        fs::write(source.join("extra.dll"), b"extra").unwrap();
        assert!(store.install(&source, &catalog).is_err());
        assert!(!store.root.exists());
        fs::remove_file(source.join("extra.dll")).unwrap();
        fs::write(source.join("pdfium.dll"), b"bad").unwrap();
        assert!(store.install(&source, &catalog).is_err());
        assert!(!store.root.exists());
    }
}

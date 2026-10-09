//! Durable physical file publication. No Tauri dependency; no file I/O under SQL locks.
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const RECOVERY_REQUIRED: &str = "PUBLICATION_RECOVERY_REQUIRED:";
const MAX_PENDING: i64 = 256;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub journal: String,
    pub owner: String,
    pub task_id: String,
    pub terminal: bool,
    pub stage: PathBuf,
    pub target: PathBuf,
    pub backup: PathBuf,
    pub digest: String,
    pub previous_digest: Option<String>,
    pub published: bool,
    #[serde(default)]
    pub directory: bool,
    #[serde(default)]
    pub result_ref: Option<PathBuf>,
    #[serde(default)]
    pub link: bool,
}
#[derive(Default)]
pub struct Recovery {
    pub verified: Vec<Receipt>,
    pub blocked: Vec<(String, String)>,
}
#[derive(Clone, Copy)]
pub struct Publication<'a> {
    pub owner: &'a str,
    pub task_id: &'a str,
    pub journal: &'a str,
    pub stage: &'a Path,
    pub target: &'a Path,
    pub replace: bool,
    pub terminal: bool,
}

pub struct Store {
    conn: Mutex<Connection>,
    active: Mutex<BTreeSet<PathBuf>>,
}
struct Lease<'a> {
    store: &'a Store,
    target: PathBuf,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.store.active.lock() {
            active.remove(&self.target);
        }
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn plain(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("reparse publication path".into());
                }
            }
            if meta.file_type().is_symlink() || (!meta.is_file() && !meta.is_dir()) {
                return Err("publication path is not a regular file".into());
            }
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(err(e)),
    }
}
fn file_digest(path: &Path) -> Result<String, String> {
    if !plain(path)? {
        return Err("publication file missing".into());
    }
    let mut file = fs::File::open(path).map_err(err)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(err)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn digest(path: &Path) -> Result<String, String> {
    if !plain(path)? {
        return Err("publication object missing".into());
    }
    if path.is_file() {
        return file_digest(path);
    }
    let mut hash = Sha256::new();
    hash.update(b"cruciblebox-directory-v1\0");
    let mut pending = vec![(path.to_path_buf(), String::new(), 0usize)];
    let mut entries = 0usize;
    let mut bytes = 0u64;
    while let Some((directory, relative, depth)) = pending.pop() {
        if depth > 64 {
            return Err("publication tree depth exceeded".into());
        }
        let mut children = Vec::new();
        for child in fs::read_dir(&directory).map_err(err)? {
            if entries + children.len() >= 500_000 {
                return Err("publication tree entry budget exceeded".into());
            }
            children.push(child.map_err(err)?);
        }
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            entries += 1;
            if entries > 500_000 {
                return Err("publication tree entry budget exceeded".into());
            }
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| "publication filename is not UTF-8")?;
            let relative = format!("{relative}/{name}");
            let child_path = child.path();
            plain(&child_path)?;
            let metadata = fs::symlink_metadata(&child_path).map_err(err)?;
            hash.update((relative.len() as u64).to_le_bytes());
            hash.update(relative.as_bytes());
            if metadata.is_dir() {
                hash.update(b"D");
                pending.push((child_path, relative, depth + 1));
            } else {
                bytes = bytes
                    .checked_add(metadata.len())
                    .ok_or("publication tree byte overflow")?;
                if bytes > 2 * 1024 * 1024 * 1024 * 1024 {
                    return Err("publication tree byte budget exceeded".into());
                }
                hash.update(b"F");
                hash.update(metadata.len().to_le_bytes());
                hash.update(file_digest(&child_path)?.as_bytes());
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn sync_stage(path: &Path) -> Result<(), String> {
    if path.is_file() {
        return fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(err)?
            .sync_all()
            .map_err(err);
    }
    digest(path)?;
    let mut pending = vec![(path.to_path_buf(), 0usize)];
    let mut entries = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        if depth > 64 {
            return Err("publication tree depth exceeded".into());
        }
        for entry in fs::read_dir(directory).map_err(err)? {
            entries += 1;
            if entries > 500_000 {
                return Err("publication tree entry budget exceeded".into());
            }
            let path = entry.map_err(err)?.path();
            plain(&path)?;
            if path.is_dir() {
                pending.push((path, depth + 1));
            } else {
                sync_stage(&path)?;
            }
        }
    }
    Ok(())
}
fn install_object(source: &Path, target: &Path, directory: bool) -> Result<(), String> {
    if directory {
        #[cfg(windows)]
        {
            fs::rename(source, target).map_err(err)
        }
        #[cfg(not(windows))]
        {
            let _ = (source, target);
            Err("directory publication requires Windows no-replace rename".into())
        }
    } else {
        fs::hard_link(source, target).map_err(err)
    }
}

fn receipt_exists(path: &Path, link: bool) -> Result<bool, String> {
    if !link {
        return plain(path);
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 == 0 || metadata.file_attributes() & 0x10 == 0
                {
                    return Err("activation path is not a directory junction".into());
                }
                fs::read_link(path).map_err(err)?;
                Ok(true)
            }
            #[cfg(not(windows))]
            {
                let _ = metadata;
                Err("junction activation requires Windows".into())
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(err(error)),
    }
}
fn receipt_digest(path: &Path, link: bool) -> Result<String, String> {
    if !link {
        return digest(path);
    }
    if !receipt_exists(path, true)? {
        return Err("activation link missing".into());
    }
    let destination = fs::read_link(path)
        .map_err(err)?
        .canonicalize()
        .map_err(err)?;
    let mut hash = Sha256::new();
    hash.update(b"cruciblebox-junction-v1\0");
    hash.update(destination.to_string_lossy().as_bytes());
    Ok(format!("{:x}", hash.finalize()))
}

fn normalized(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or("publication parent missing")?
        .canonicalize()
        .map_err(err)?;
    let name = path.file_name().ok_or("publication filename missing")?;
    Ok(parent.join(name))
}
impl Receipt {
    pub fn result_reference(&self) -> &Path {
        self.result_ref.as_deref().unwrap_or(&self.target)
    }
}
fn validate(receipt: &Receipt) -> Result<(), String> {
    if let Some(reference) = &receipt.result_ref {
        if !reference.is_absolute()
            || reference.to_string_lossy().len() > 2048
            || !reference.is_dir()
            || reference.canonicalize().map_err(err)? != *reference
            || !receipt.target.starts_with(reference)
        {
            return Err("invalid publication result reference".into());
        }
    }
    if receipt.id.len() != 64
        || !receipt.id.bytes().all(|b| b.is_ascii_hexdigit())
        || [&receipt.journal, &receipt.owner, &receipt.task_id]
            .iter()
            .any(|s| s.is_empty() || s.len() > 1024 || s.contains('\0'))
        || receipt.digest.len() != 64
        || !receipt.digest.bytes().all(|b| b.is_ascii_hexdigit())
        || receipt
            .previous_digest
            .as_ref()
            .is_some_and(|v| v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("invalid publication receipt".into());
    }
    for path in [&receipt.stage, &receipt.target, &receipt.backup] {
        if !path.is_absolute() || path.to_string_lossy().len() > 2048 || normalized(path)? != *path
        {
            return Err("invalid publication path".into());
        }
        if receipt_exists(path, receipt.link)?
            && !receipt.link
            && path.is_dir() != receipt.directory
        {
            return Err("publication object kind changed".into());
        }
    }
    if receipt.stage.parent() != receipt.target.parent()
        || receipt.backup.parent() != receipt.target.parent()
        || receipt.stage == receipt.target
        || receipt.target == receipt.backup
        || receipt.stage == receipt.backup
        || receipt.backup.file_name().and_then(|s| s.to_str())
            != Some(format!(".cb-publication-{}.bak", receipt.id).as_str())
        || !receipt
            .stage
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with('.'))
    {
        return Err("publication paths are not reserved siblings".into());
    }
    Ok(())
}
fn delete_verified(path: &Path, expected: &str, link: bool) -> Result<(), String> {
    if receipt_exists(path, link)? {
        if receipt_digest(path, link)? != expected {
            return Err("publication cleanup digest conflict".into());
        }
        if link {
            fs::remove_dir(path).map_err(err)?;
        } else if path.is_dir() {
            fs::remove_dir_all(path).map_err(err)?;
        } else {
            fs::remove_file(path).map_err(err)?;
        }
    }
    Ok(())
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(err)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(err)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS file_publications(id TEXT PRIMARY KEY,journal TEXT NOT NULL UNIQUE,receipt TEXT NOT NULL);").map_err(err)?;
        Ok(Self {
            conn: Mutex::new(conn),
            active: Mutex::new(BTreeSet::new()),
        })
    }
    fn save(&self, receipt: &Receipt, create: bool) -> Result<(), String> {
        validate(receipt)?;
        let json = serde_json::to_string(receipt).map_err(err)?;
        if json.len() > 16384 {
            return Err("publication receipt budget exceeded".into());
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "publication store unavailable")?;
        let tx = conn.transaction().map_err(err)?;
        if create {
            let count: i64 = tx
                .query_row("SELECT COUNT(*) FROM file_publications", [], |r| r.get(0))
                .map_err(err)?;
            if count >= MAX_PENDING {
                return Err("publication recovery queue full".into());
            }
            tx.execute(
                "INSERT INTO file_publications(id,journal,receipt) VALUES (?1,?2,?3)",
                params![receipt.id, receipt.journal, json],
            )
            .map_err(err)?;
        } else if tx
            .execute(
                "UPDATE file_publications SET receipt=?1 WHERE id=?2 AND journal=?3",
                params![json, receipt.id, receipt.journal],
            )
            .map_err(err)?
            != 1
        {
            return Err("publication receipt disappeared".into());
        }
        tx.commit().map_err(err)
    }
    #[cfg(test)]
    fn prepare(&self, request: Publication<'_>) -> Result<Receipt, String> {
        self.prepare_with_reference(request, None, false)
    }
    fn prepare_with_reference(
        &self,
        request: Publication<'_>,
        result_ref: Option<&Path>,
        link: bool,
    ) -> Result<Receipt, String> {
        let Publication {
            owner,
            task_id,
            journal,
            stage,
            target,
            replace,
            terminal,
        } = request;
        let stage = normalized(stage)?;
        let requested = normalized(target)?;
        let mut target = requested.clone();
        if !replace {
            for index in 0..=999 {
                if !receipt_exists(&target, link)? {
                    break;
                }
                if index == 999 {
                    return Err("publication target names exhausted".into());
                }
                let stem = requested
                    .file_stem()
                    .ok_or("publication name missing")?
                    .to_string_lossy();
                let extension = requested
                    .extension()
                    .map(|v| format!(".{}", v.to_string_lossy()))
                    .unwrap_or_default();
                target = requested.with_file_name(format!("{stem} ({}){extension}", index + 1));
            }
        }
        let mut bytes = [0; 32];
        getrandom::getrandom(&mut bytes).map_err(err)?;
        let id = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let backup = target.with_file_name(format!(".cb-publication-{id}.bak"));
        if receipt_exists(&backup, link)? {
            return Err("publication backup collision".into());
        }
        let directory = !link && stage.is_dir();
        if receipt_exists(&target, link)? && !link && target.is_dir() != directory {
            return Err("publication target kind differs".into());
        }
        if !link {
            sync_stage(&stage)?;
        }
        let previous_digest = if receipt_exists(&target, link)? {
            Some(receipt_digest(&target, link)?)
        } else {
            None
        };
        let receipt = Receipt {
            id,
            journal: journal.into(),
            owner: owner.into(),
            task_id: task_id.into(),
            terminal,
            digest: receipt_digest(&stage, link)?,
            previous_digest,
            stage,
            target,
            backup,
            published: false,
            directory,
            link,
            result_ref: result_ref
                .map(Path::canonicalize)
                .transpose()
                .map_err(err)?,
        };
        self.save(&receipt, true)?;
        Ok(receipt)
    }
    pub fn publish(&self, request: Publication<'_>) -> Result<PathBuf, String> {
        self.publish_with_reference(request, None)
    }
    pub fn publish_with_reference(
        &self,
        request: Publication<'_>,
        reference: Option<&Path>,
    ) -> Result<PathBuf, String> {
        self.publish_kind(request, reference, false)
    }
    pub fn publish_link(
        &self,
        request: Publication<'_>,
        reference: Option<&Path>,
    ) -> Result<PathBuf, String> {
        self.publish_kind(request, reference, true)
    }
    fn publish_kind(
        &self,
        request: Publication<'_>,
        reference: Option<&Path>,
        link: bool,
    ) -> Result<PathBuf, String> {
        let target = request.target;
        let journal = request.journal;
        let target_key = normalized(target)?;
        {
            let mut active = self
                .active
                .lock()
                .map_err(|_| "publication leases unavailable")?;
            if !active.insert(target_key.clone()) {
                return Err("publication target busy".into());
            }
        }
        let _lease = Lease {
            store: self,
            target: target_key,
        };
        let mut receipt = self.prepare_with_reference(request, reference, link)?;
        let publish = (|| {
            if let Some(previous) = &receipt.previous_digest {
                if receipt_digest(&receipt.target, receipt.link)? != *previous {
                    return Err("publication original changed".into());
                }
                // Same-volume rename retains the exact previous object; target installation uses no-clobber hard link.
                fs::rename(&receipt.target, &receipt.backup).map_err(err)?;
            }
            if receipt_digest(&receipt.stage, receipt.link)? != receipt.digest {
                return Err("publication stage changed".into());
            }
            install_object(
                &receipt.stage,
                &receipt.target,
                receipt.directory || receipt.link,
            )?;
            if receipt_digest(&receipt.target, receipt.link)? != receipt.digest {
                return Err("publication result changed".into());
            }
            receipt.published = true;
            self.save(&receipt, false)?;
            Ok(receipt.target.clone())
        })();
        publish.map_err(|e: String| format!("{RECOVERY_REQUIRED} {e}; journal={journal}"))
    }
    /// Only acknowledge after the task authority has persisted the matching publication.
    pub fn acknowledge(&self, journal: &str) -> Result<(), String> {
        let receipt = self
            .load()?
            .into_iter()
            .find(|r| r.journal == journal)
            .ok_or("publication receipt missing")?;
        validate(&receipt)?;
        if !receipt.published || receipt_digest(&receipt.target, receipt.link)? != receipt.digest {
            return Err("publication is not verified".into());
        }
        if receipt_exists(&receipt.stage, receipt.link)?
            && receipt_digest(&receipt.stage, receipt.link)? != receipt.digest
        {
            return Err("publication stage changed".into());
        }
        if let Some(previous) = &receipt.previous_digest {
            if receipt_exists(&receipt.backup, receipt.link)?
                && receipt_digest(&receipt.backup, receipt.link)? != *previous
            {
                return Err("publication backup changed".into());
            }
        }
        delete_verified(&receipt.stage, &receipt.digest, receipt.link)?;
        if let Some(previous) = &receipt.previous_digest {
            delete_verified(&receipt.backup, previous, receipt.link)?;
        }
        self.forget(&receipt.id)
    }
    fn forget(&self, id: &str) -> Result<(), String> {
        self.conn
            .lock()
            .map_err(|_| "publication store unavailable")?
            .execute("DELETE FROM file_publications WHERE id=?1", [id])
            .map_err(err)?;
        Ok(())
    }
    fn load(&self) -> Result<Vec<Receipt>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "publication store unavailable")?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM file_publications", [], |r| r.get(0))
            .map_err(err)?;
        if count > MAX_PENDING {
            return Err("publication row budget exceeded".into());
        }
        let mut statement = conn
            .prepare("SELECT id,journal,receipt FROM file_publications WHERE length(CAST(receipt AS BLOB))<=16384")
            .map_err(err)?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        if rows.len() as i64 != count {
            return Err("publication receipt budget exceeded".into());
        }
        rows.iter()
            .map(|(id, journal, raw)| {
                let receipt: Receipt = serde_json::from_str(raw).map_err(err)?;
                if receipt.id != *id || receipt.journal != *journal {
                    return Err("publication row identity mismatch".into());
                }
                Ok(receipt)
            })
            .collect()
    }
    pub fn recover(&self) -> Result<Recovery, String> {
        let rows = self.load()?;
        let mut recovery = Recovery::default();
        for mut receipt in rows {
            let result = (|| {
                validate(&receipt)?;
                let target = if receipt_exists(&receipt.target, receipt.link)? {
                    Some(receipt_digest(&receipt.target, receipt.link)?)
                } else {
                    None
                };
                let backup = if receipt_exists(&receipt.backup, receipt.link)? {
                    Some(receipt_digest(&receipt.backup, receipt.link)?)
                } else {
                    None
                };
                if backup.is_some() && backup != receipt.previous_digest {
                    return Err("publication backup changed".into());
                }
                if target.as_deref() == Some(&receipt.digest)
                    && (receipt.published
                        || receipt.previous_digest.as_deref() != Some(&receipt.digest)
                        || backup.is_some())
                {
                    receipt.published = true;
                    self.save(&receipt, false)?;
                    return Ok(true);
                }
                if receipt.published {
                    return Err("published output missing or changed".into());
                }
                match (target, backup, receipt.previous_digest.as_ref()) {
                    (Some(current), None, Some(previous)) if current == *previous => {}
                    (None, Some(_), Some(_)) => {
                        install_object(
                            &receipt.backup,
                            &receipt.target,
                            receipt.directory || receipt.link,
                        )?;
                        if receipt_digest(&receipt.target, receipt.link)?
                            != *receipt.previous_digest.as_ref().unwrap()
                        {
                            return Err("restored original changed".into());
                        }
                        delete_verified(
                            &receipt.backup,
                            receipt.previous_digest.as_ref().unwrap(),
                            receipt.link,
                        )?;
                    }
                    (None, None, None) => {}
                    _ => return Err("ambiguous physical publication; files retained".into()),
                }
                self.forget(&receipt.id)?;
                Ok(false)
            })();
            match result {
                Ok(true) => recovery.verified.push(receipt),
                Ok(false) => {}
                Err(e) => recovery.blocked.push((receipt.journal.clone(), e)),
            }
        }
        Ok(recovery)
    }
}

#[cfg(test)]
mod tests {

    #[cfg(windows)]
    #[test]
    fn directory_publication_recovery_checks_content_and_retains_changed_backup() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".directory.stage");
        let target = root.path().join("runtime");
        fs::create_dir_all(stage.join("empty")).unwrap();
        fs::write(stage.join("runtime.exe"), b"new runtime").unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("runtime.exe"), b"old runtime").unwrap();
        let db = root.path().join("receipts.sqlite");
        let store = Store::open(&db).unwrap();
        let receipt = store
            .prepare(Publication {
                owner: "unienv",
                task_id: "install",
                journal: "publication:directory",
                stage: &stage,
                target: &target,
                replace: true,
                terminal: false,
            })
            .unwrap();
        assert!(receipt.directory);
        fs::rename(&target, &receipt.backup).unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        assert!(store.recover().unwrap().verified.is_empty());
        assert_eq!(
            fs::read(target.join("runtime.exe")).unwrap(),
            b"old runtime"
        );
        assert!(stage.join("empty").is_dir());
        store
            .publish(Publication {
                owner: "unienv",
                task_id: "install",
                journal: "publication:directory2",
                stage: &stage,
                target: &target,
                replace: true,
                terminal: false,
            })
            .unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        let report = store.recover().unwrap();
        assert_eq!(report.verified.len(), 1);
        let receipt = &report.verified[0];
        fs::write(
            receipt.backup.join("user-file.txt"),
            b"retained external data",
        )
        .unwrap();
        assert!(store.acknowledge("publication:directory2").is_err());
        assert_eq!(
            fs::read(receipt.backup.join("user-file.txt")).unwrap(),
            b"retained external data"
        );
        assert_eq!(
            fs::read(target.join("runtime.exe")).unwrap(),
            b"new runtime"
        );
    }

    #[cfg(windows)]
    #[test]
    fn directory_digest_detects_empty_directory_change_and_crash_after_promotion() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".stage");
        let target = root.path().join("output");
        fs::create_dir_all(stage.join("empty")).unwrap();
        fs::write(stage.join("file"), b"output").unwrap();
        let original = digest(&stage).unwrap();
        fs::remove_dir(stage.join("empty")).unwrap();
        assert_ne!(digest(&stage).unwrap(), original);
        fs::create_dir(stage.join("empty")).unwrap();
        let store = Store::open(&root.path().join("receipts.sqlite")).unwrap();
        let receipt = store
            .prepare(Publication {
                owner: "archive",
                task_id: "extract",
                journal: "publication:promoted",
                stage: &stage,
                target: &target,
                replace: false,
                terminal: false,
            })
            .unwrap();
        fs::rename(&stage, &target).unwrap();
        let report = store.recover().unwrap();
        assert_eq!(report.verified.len(), 1);
        assert_eq!(report.verified[0].digest, receipt.digest);
        store.acknowledge("publication:promoted").unwrap();
        assert!(store.load().unwrap().is_empty());
        assert!(target.join("empty").is_dir());
    }

    #[test]
    fn corrupted_row_identity_and_oversized_receipt_preserve_all_files() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".stage");
        let target = root.path().join("output.txt");
        fs::write(&stage, b"new").unwrap();
        fs::write(&target, b"old").unwrap();
        let store = Store::open(&root.path().join("receipts.sqlite")).unwrap();
        let receipt = store
            .prepare(Publication {
                owner: "owner",
                task_id: "task",
                journal: "publication:bad",
                stage: &stage,
                target: &target,
                replace: true,
                terminal: true,
            })
            .unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE file_publications SET journal='different' WHERE id=?1",
                [&receipt.id],
            )
            .unwrap();
        assert!(store.recover().err().unwrap().contains("identity mismatch"));
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE file_publications SET receipt=?1 WHERE id=?2",
                params!["x".repeat(16385), receipt.id],
            )
            .unwrap();
        assert!(store.recover().err().unwrap().contains("budget exceeded"));
        assert_eq!(fs::read(stage).unwrap(), b"new");
        assert_eq!(fs::read(target).unwrap(), b"old");
    }

    #[test]
    fn identical_content_without_physical_publication_does_not_prove_success() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".stage");
        let target = root.path().join("output.txt");
        fs::write(&stage, b"same").unwrap();
        fs::write(&target, b"same").unwrap();
        let store = Store::open(&root.path().join("receipts.sqlite")).unwrap();
        store
            .prepare(Publication {
                owner: "owner",
                task_id: "task",
                journal: "publication:same",
                stage: &stage,
                target: &target,
                replace: true,
                terminal: true,
            })
            .unwrap();
        let report = store.recover().unwrap();
        assert!(report.verified.is_empty());
        assert!(report.blocked.is_empty());
        assert!(store.load().unwrap().is_empty());
        assert_eq!(fs::read(stage).unwrap(), b"same");
        assert_eq!(fs::read(target).unwrap(), b"same");
    }

    use super::*;
    #[test]
    fn real_publish_is_durable_until_task_acknowledgement() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".stage");
        let target = root.path().join("output.txt");
        fs::write(&stage, b"new").unwrap();
        fs::write(&target, b"old").unwrap();
        let db = root.path().join("receipts.sqlite");
        let store = Store::open(&db).unwrap();
        assert_eq!(
            store
                .publish(Publication {
                    owner: "document",
                    task_id: "task",
                    journal: "publication:1",
                    stage: &stage,
                    target: &target,
                    replace: true,
                    terminal: true
                })
                .unwrap(),
            target.canonicalize().unwrap()
        );
        drop(store);
        let store = Store::open(&db).unwrap();
        let report = store.recover().unwrap();
        assert!(report.blocked.is_empty());
        assert_eq!(report.verified.len(), 1);
        assert!(report.verified[0].terminal);
        store.acknowledge("publication:1").unwrap();
        assert!(store.load().unwrap().is_empty());
        assert_eq!(fs::read(target).unwrap(), b"new");
        assert!(!stage.exists());
    }
    #[test]
    fn crash_after_original_move_restores_old_file_and_retains_stage() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".stage");
        let target = root.path().join("output.txt");
        fs::write(&stage, b"new").unwrap();
        fs::write(&target, b"old").unwrap();
        let store = Store::open(&root.path().join("receipts.sqlite")).unwrap();
        let receipt = store
            .prepare(Publication {
                owner: "owner",
                task_id: "task",
                journal: "publication:2",
                stage: &stage,
                target: &target,
                replace: true,
                terminal: true,
            })
            .unwrap();
        fs::rename(&receipt.target, &receipt.backup).unwrap();
        let report = store.recover().unwrap();
        assert!(report.verified.is_empty());
        assert!(report.blocked.is_empty());
        assert_eq!(fs::read(target).unwrap(), b"old");
        assert_eq!(fs::read(stage).unwrap(), b"new");
    }
    #[test]
    fn crash_after_link_proves_output_but_changed_backup_blocks_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".stage");
        let target = root.path().join("output.txt");
        fs::write(&stage, b"new").unwrap();
        fs::write(&target, b"old").unwrap();
        let store = Store::open(&root.path().join("receipts.sqlite")).unwrap();
        let receipt = store
            .prepare(Publication {
                owner: "owner",
                task_id: "task",
                journal: "publication:3",
                stage: &stage,
                target: &target,
                replace: true,
                terminal: false,
            })
            .unwrap();
        fs::rename(&receipt.target, &receipt.backup).unwrap();
        fs::hard_link(&receipt.stage, &receipt.target).unwrap();
        let report = store.recover().unwrap();
        assert_eq!(report.verified.len(), 1);
        assert!(!report.verified[0].terminal);
        fs::write(&receipt.backup, b"external change").unwrap();
        assert!(store.acknowledge("publication:3").is_err());
        assert_eq!(fs::read(&receipt.backup).unwrap(), b"external change");
        assert!(stage.exists());
    }
    #[test]
    fn collision_preserves_foreign_file_and_receipt_is_blocked() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join(".stage");
        let target = root.path().join("output.txt");
        fs::write(&stage, b"new").unwrap();
        let store = Store::open(&root.path().join("receipts.sqlite")).unwrap();
        let receipt = store
            .prepare(Publication {
                owner: "owner",
                task_id: "task",
                journal: "publication:4",
                stage: &stage,
                target: &target,
                replace: false,
                terminal: true,
            })
            .unwrap();
        fs::write(&receipt.target, b"foreign").unwrap();
        let report = store.recover().unwrap();
        assert_eq!(report.blocked.len(), 1);
        assert!(report.verified.is_empty());
        assert_eq!(fs::read(target).unwrap(), b"foreign");
        assert_eq!(fs::read(stage).unwrap(), b"new");
    }
}

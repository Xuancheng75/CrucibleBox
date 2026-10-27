use base64::Engine;
use cruciblebox_next2_protocol::{validate_manifest, Backend, Manifest, SignatureRecord};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublisherKey {
    pub publisher_id: String,
    pub key_id: String,
    pub public_key: [u8; 32],
    #[serde(default)]
    pub official: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Provenance {
    Verified {
        publisher_id: String,
        key_id: String,
    },
    LocalUnsigned,
}
pub struct InspectedPlugin {
    root: PathBuf,
    manifest: Manifest,
    manifest_sha256: [u8; 32],
    provenance: Provenance,
}
pub struct ApprovedPlugin {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    manifest_sha256: [u8; 32],
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Approval {
    pub manifest_sha256: [u8; 32],
    pub accept_native_code: bool,
    pub accept_unsigned_local: bool,
    pub accept_permissions: bool,
}
impl InspectedPlugin {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
    pub fn manifest_sha256(&self) -> [u8; 32] {
        self.manifest_sha256
    }
    /// Called only after the main host UI confirms this immutable staged snapshot.
    pub fn approve(self, decision: Approval) -> Result<ApprovedPlugin, String> {
        if decision.manifest_sha256 != self.manifest_sha256 || !decision.accept_permissions {
            return Err("INSTALL_APPROVAL_MISMATCH".into());
        }
        if matches!(self.manifest.backend, Some(Backend::NativeBackend(_)))
            && !decision.accept_native_code
        {
            return Err("NATIVE_TRUST_REQUIRED".into());
        }
        if self.provenance == Provenance::LocalUnsigned && !decision.accept_unsigned_local {
            return Err("LOCAL_TRUST_REQUIRED".into());
        }
        Ok(ApprovedPlugin {
            root: self.root,
            manifest: self.manifest,
            manifest_sha256: self.manifest_sha256,
        })
    }
}
impl ApprovedPlugin {
    pub fn manifest_sha256(&self) -> [u8; 32] {
        self.manifest_sha256
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn revalidate(&self) -> Result<(), String> {
        let raw = read_limited(
            &self.root.join("plugin.json"),
            cruciblebox_next2_protocol::BUDGET_MANIFEST_BYTES,
        )?;
        if <[u8; 32]>::from(Sha256::digest(&raw)) != self.manifest_sha256 {
            return Err("APPROVED_MANIFEST_CHANGED".into());
        }
        verify_payload(&self.root, &self.manifest)
    }
}
pub(crate) fn plain(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.file_type().is_symlink() && (meta.is_file() || meta.is_dir())
}
pub(crate) fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !plain(&meta) || !meta.is_file() {
        return Err("NON_REGULAR_PACKAGE_METADATA".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("PACKAGE_METADATA_TOO_LARGE".into());
    }
    Ok(bytes)
}
fn verify_pe(file: &mut File, executable: bool) -> Result<(), String> {
    let mut dos = [0u8; 64];
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.read_exact(&mut dos))
        .map_err(|e| format!("INVALID_PE: {e}"))?;
    if &dos[..2] != b"MZ" {
        return Err("INVALID_PE".into());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap());
    let mut pe = [0u8; 26];
    file.seek(SeekFrom::Start(offset.into()))
        .and_then(|_| file.read_exact(&mut pe))
        .map_err(|e| format!("INVALID_PE: {e}"))?;
    if &pe[..4] != b"PE\0\0"
        || u16::from_le_bytes([pe[4], pe[5]]) != 0x8664
        || u16::from_le_bytes([pe[24], pe[25]]) != 0x20b
    {
        return Err("NATIVE_ARCHITECTURE_MISMATCH".into());
    }
    if executable && u16::from_le_bytes([pe[22], pe[23]]) & 0x2000 != 0 {
        return Err("NATIVE_ENTRY_IS_DLL".into());
    }
    Ok(())
}
struct Payload<'a> {
    root: &'a Path,
    expected: HashMap<String, &'a cruciblebox_next2_protocol::FileEntry>,
    metadata_name: &'a str,
    native: bool,
    max_entries: usize,
}
impl Payload<'_> {
    fn walk(&self, at: &Path, seen: &mut HashSet<String>, count: &mut usize) -> Result<(), String> {
        for entry in fs::read_dir(at).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = path
                .strip_prefix(self.root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("INVALID_PACKAGE_PATH")?
                .replace('\\', "/");
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if !plain(&meta) {
                return Err(format!("NON_REGULAR_PACKAGE_ENTRY: {name}"));
            }
            *count += 1;
            if *count > self.max_entries {
                return Err("PACKAGE_ENTRY_BUDGET".into());
            }
            if meta.is_dir() {
                self.walk(&path, seen, count)?;
                continue;
            }
            // The host installer rejects these markers in source packages and
            // creates its journal after staging. It is never executable payload.
            if name == self.metadata_name
                || name == "signature.json"
                || (self.metadata_name == "plugin.json" && name == ".openbox-host-transaction.json")
            {
                continue;
            }
            let declared = self
                .expected
                .get(&name)
                .ok_or_else(|| format!("UNDECLARED_PACKAGE_FILE: {name}"))?;
            let mut file = File::open(&path).map_err(|e| e.to_string())?;
            if file.metadata().map_err(|e| e.to_string())?.len() != declared.bytes {
                return Err(format!("PACKAGE_SIZE_MISMATCH: {name}"));
            }
            let mut hasher = Sha256::new();
            let mut buffer = [0u8; 65536];
            let mut remaining = declared.bytes + 1;
            let mut first = true;
            let mut pe_content = false;
            loop {
                let capacity = buffer.len().min(remaining as usize);
                let read = file
                    .read(&mut buffer[..capacity])
                    .map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                if first {
                    pe_content = buffer[..read].starts_with(b"MZ");
                    first = false;
                }
                remaining -= read as u64;
                hasher.update(&buffer[..read]);
                if remaining == 0 {
                    return Err(format!("PACKAGE_SIZE_MISMATCH: {name}"));
                }
            }
            if format!("{:x}", hasher.finalize()) != declared.sha256 {
                return Err(format!("PACKAGE_DIGEST_MISMATCH: {name}"));
            }
            let lower = name.to_ascii_lowercase();
            if pe_content || lower.ends_with(".exe") || lower.ends_with(".dll") {
                if !self.native {
                    return Err("NATIVE_TRUST_DECLARATION_MISSING".into());
                }
                verify_pe(&mut file, lower.ends_with(".exe"))?;
            }
            seen.insert(name);
        }
        Ok(())
    }
}
pub(crate) fn verify_files(
    root: &Path,
    files: &[cruciblebox_next2_protocol::FileEntry],
    metadata_name: &str,
    native: bool,
    max_entries: usize,
) -> Result<(), String> {
    let payload = Payload {
        root,
        expected: files
            .iter()
            .map(|entry| (entry.path.clone(), entry))
            .collect(),
        metadata_name,
        native,
        max_entries,
    };
    let mut seen = HashSet::new();
    payload.walk(root, &mut seen, &mut 0)?;
    if seen.len() != payload.expected.len() {
        return Err("PACKAGE_FILE_MISSING".into());
    }
    Ok(())
}
pub fn inspect_plugin(
    root: &Path,
    trusted_keys: &[PublisherKey],
    host_version: &str,
) -> Result<InspectedPlugin, String> {
    let meta = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !plain(&meta) || !meta.is_dir() {
        return Err("INVALID_PACKAGE_ROOT".into());
    }
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let raw = read_limited(
        &root.join("plugin.json"),
        cruciblebox_next2_protocol::BUDGET_MANIFEST_BYTES,
    )?;
    let manifest = validate_manifest(std::str::from_utf8(&raw).map_err(|e| e.to_string())?)?;
    if semver::Version::parse(host_version).map_err(|e| e.to_string())?
        < semver::Version::parse(&manifest.min_host_version).map_err(|e| e.to_string())?
    {
        return Err("HOST_VERSION_INCOMPATIBLE".into());
    }
    let provenance = verify_provenance(&root, &raw, &manifest.publisher.id, trusted_keys)?;
    verify_payload(&root, &manifest)?;
    Ok(InspectedPlugin {
        root,
        manifest,
        manifest_sha256: Sha256::digest(&raw).into(),
        provenance,
    })
}
pub(crate) fn verify_provenance(
    root: &Path,
    raw: &[u8],
    publisher: &str,
    trusted_keys: &[PublisherKey],
) -> Result<Provenance, String> {
    let signature_path = root.join("signature.json");
    match fs::symlink_metadata(&signature_path) {
        Ok(meta) => {
            if !plain(&meta) || !meta.is_file() {
                return Err("INVALID_SIGNATURE_FILE".into());
            }
            let bytes = read_limited(&signature_path, 4096)?;
            let signature: SignatureRecord =
                serde_json::from_slice(&bytes).map_err(|e| format!("INVALID_SIGNATURE: {e}"))?;
            if signature.signature_version != 1 || signature.publisher_id != publisher {
                return Err("PUBLISHER_MISMATCH".into());
            }
            let key = trusted_keys
                .iter()
                .find(|key| {
                    key.key_id == signature.key_id && key.publisher_id == signature.publisher_id
                })
                .ok_or("SIGNING_KEY_NOT_TRUSTED")?;
            let signature_bytes = base64::engine::general_purpose::STANDARD
                .decode(&signature.signature)
                .map_err(|e| format!("INVALID_SIGNATURE: {e}"))?;
            ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key.public_key)
                .verify(raw, &signature_bytes)
                .map_err(|_| "SIGNATURE_INVALID")?;
            Ok(Provenance::Verified {
                publisher_id: key.publisher_id.clone(),
                key_id: key.key_id.clone(),
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Provenance::LocalUnsigned),
        Err(error) => Err(error.to_string()),
    }
}
fn verify_payload(root: &Path, manifest: &Manifest) -> Result<(), String> {
    verify_files(
        root,
        &manifest.files,
        "plugin.json",
        matches!(manifest.backend, Some(Backend::NativeBackend(_))),
        20000,
    )
}

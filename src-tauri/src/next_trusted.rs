//! Fixed Next trusted-service policy: installation and every native call fail closed.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, sync::OnceLock};
fn policy(service: &str) -> Result<&'static Value, String> {
    static POLICY: OnceLock<Result<Value, String>> = OnceLock::new();
    POLICY
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../shared/trusted-service-policies.json"))
                .map_err(|_| "PERMISSION_DENIED".into())
        })
        .as_ref()
        .map_err(Clone::clone)?
        .get(service)
        .filter(|v| v["manifestVersion"] == 5)
        .ok_or_else(|| "PERMISSION_DENIED".into())
}
fn plain(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|_| "PERMISSION_DENIED")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("PERMISSION_DENIED".into());
        }
    }
    if m.file_type().is_symlink() || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err("PERMISSION_DENIED".into());
    }
    Ok(())
}
pub fn files(service: &str) -> Result<Vec<String>, String> {
    policy(service)?["files"]
        .as_array()
        .ok_or("PERMISSION_DENIED")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "PERMISSION_DENIED".into())
        })
        .collect()
}
pub fn verify(service: &str, directory: &Path, permissions: &[String]) -> Result<(), String> {
    let p = policy(service)?;
    plain(directory, true)?;
    let manifest =
        fs::read_to_string(directory.join("plugin.json")).map_err(|_| "PERMISSION_DENIED")?;
    let manifest =
        cruciblebox_next_protocol::validate_manifest(&manifest).map_err(|_| "PERMISSION_DENIED")?;
    if manifest.id != service
        || manifest.version != p["version"].as_str().ok_or("PERMISSION_DENIED")?
        || manifest.backend.is_some()
        || serde_json::to_value(permissions).map_err(|_| "PERMISSION_DENIED")? != p["permissions"]
        || manifest.permissions != permissions
    {
        return Err("PERMISSION_DENIED".into());
    }
    let mut names = files(service)?;
    names.sort();
    let mut hash = Sha256::new();
    let mut total = 0u64;
    for name in names {
        if name.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.contains('\\')
                || part.contains(':')
        }) {
            return Err("PERMISSION_DENIED".into());
        }
        let mut path = directory.to_path_buf();
        let components: Vec<_> = name.split('/').collect();
        for (index, part) in components.iter().enumerate() {
            path.push(part);
            plain(&path, index + 1 < components.len())?
        }
        hash.update(name.as_bytes());
        hash.update([0]);
        let mut file = fs::File::open(path).map_err(|_| "PERMISSION_DENIED")?;
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|_| "PERMISSION_DENIED")?;
            if n == 0 {
                break;
            }
            total = total.saturating_add(n as u64);
            if total > 64 * 1024 * 1024 {
                return Err("PERMISSION_DENIED".into());
            }
            hash.update(&buffer[..n]);
        }
        hash.update([0]);
    }
    if format!("{:x}", hash.finalize()) != p["digest"].as_str().ok_or("PERMISSION_DENIED")? {
        return Err("PERMISSION_DENIED".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_runtime_rejects_tampering_missing_files_and_capability_changes() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/archive-extractor");
        let manifest = cruciblebox_next_protocol::validate_manifest(
            &fs::read_to_string(source.join("plugin.json")).unwrap(),
        )
        .unwrap();
        verify("archive-extractor", &source, &manifest.permissions).unwrap();
        let root = tempfile::tempdir().unwrap();
        for name in files("archive-extractor").unwrap() {
            let to = root.path().join(&name);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::copy(source.join(&name), to).unwrap();
        }
        verify("archive-extractor", root.path(), &manifest.permissions).unwrap();
        let mut permissions = manifest.permissions.clone();
        permissions.pop();
        assert!(verify("archive-extractor", root.path(), &permissions).is_err());
        fs::write(
            root.path().join("dist/renderer.js"),
            "export function mount(){/* changed */}",
        )
        .unwrap();
        assert!(verify("archive-extractor", root.path(), &manifest.permissions).is_err());
        fs::remove_file(root.path().join("plugin.json")).unwrap();
        assert!(verify("archive-extractor", root.path(), &manifest.permissions).is_err());
    }
}

//! Remove managed installation payloads while retaining user-owned files.
use crate::transaction::{
    assert_direct_child, is_plugin_name, path_entry_exists, remove_internal_directory,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn cleanup(
    plugins: &Path,
    retained: Option<&Path>,
    cache: &Path,
    name: &str,
) -> Result<Vec<String>, String> {
    if !is_plugin_name(name) {
        return Err("INVALID_PLUGIN_ID".into());
    }
    // Managed resources are stored in an exclusive owner namespace, outside user data.
    let resources = plugins
        .parent()
        .ok_or("INVALID_PLUGIN_DIRECTORY")?
        .join("plugin-resources");
    if path_entry_exists(&resources)? {
        remove_internal_directory(&resources, &resources.join(name), name)?;
    }
    remove_cached_packages(cache, name)?;

    let prefix = format!(".{name}.retained-");
    let mut roots: Vec<PathBuf> = retained.into_iter().map(Path::to_path_buf).collect();
    for entry in fs::read_dir(plugins).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_name()
            .to_str()
            .and_then(|value| value.strip_prefix(&prefix))
            .is_some_and(crate::transaction::is_transaction_id)
            && !roots.contains(&entry.path())
        {
            roots.push(entry.path());
        }
    }
    let mut retained_files = Vec::new();
    for root in roots {
        let basename = root
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("INVALID_RETAINED_PATH")?;
        assert_direct_child(plugins, &root, basename)?;
        crate::transaction::assert_plain_directory(&root, "Retained plugin")?;
        if path_entry_exists(&root.join("plugin.json"))? {
            let manifest = crate::manifest::read_manifest(&root)?;
            if manifest.name != name {
                return Err("UNINSTALL_IDENTITY_MISMATCH".into());
            }
            if manifest.has_versioned_code() {
                let code = cruciblebox_next2_protocol::CODE_DIRECTORY;
                remove_internal_directory(&root, &root.join(code), code)?;
                fs::remove_file(root.join("plugin.json")).map_err(|error| error.to_string())?;
            }
        }
        if has_user_files(&root)? {
            retained_files.push(
                retain_user_files(plugins, &root, name)?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        if retained != Some(root.as_path()) {
            remove_empty_retained_directory(&root)?;
        }
    }
    Ok(retained_files)
}

fn retain_user_files(plugins: &Path, root: &Path, name: &str) -> Result<PathBuf, String> {
    let mut destination = plugins
        .parent()
        .ok_or("INVALID_PLUGIN_DIRECTORY")?
        .to_path_buf();
    for part in ["plugin-data", name, "uninstalled-files"] {
        destination.push(part);
        if path_entry_exists(&destination)? {
            crate::transaction::assert_plain_directory(&destination, "Retained user data")?;
        } else {
            fs::create_dir(&destination).map_err(|error| error.to_string())?;
        }
    }
    destination.push(root.file_name().ok_or("INVALID_RETAINED_PATH")?);
    if path_entry_exists(&destination)? {
        crate::transaction::assert_plain_directory(&destination, "Retained user files")?;
    } else {
        fs::create_dir(&destination).map_err(|error| error.to_string())?;
    }
    // Leave the committed journal in place until its caller clears it. Recovery
    // can resume moving remaining entries without overwriting anything already kept.
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(crate::journal::JOURNAL_FILENAME)
        {
            continue;
        }
        let target = destination.join(entry.file_name());
        if path_entry_exists(&target)? {
            return Err(format!("RETAINED_USER_FILE_EXISTS: {}", target.display()));
        }
        fs::rename(entry.path(), target).map_err(|error| error.to_string())?;
    }
    Ok(destination)
}

fn has_user_files(root: &Path) -> Result<bool, String> {
    let has_files = fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .map(|entry| entry.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|entry| {
            !entry
                .file_name()
                .to_string_lossy()
                .starts_with(crate::journal::JOURNAL_FILENAME)
        });
    Ok(has_files)
}

fn remove_cached_packages(cache: &Path, name: &str) -> Result<(), String> {
    if !path_entry_exists(cache)? {
        return Ok(());
    }
    crate::transaction::assert_plain_directory(cache, "Plugin download cache")?;
    for source in fs::read_dir(cache).map_err(|error| error.to_string())? {
        let source = source.map_err(|error| error.to_string())?;
        let source_name = source.file_name().to_string_lossy().into_owned();
        if source_name != "official"
            && !source_name
                .strip_prefix("source-")
                .is_some_and(|value| value.parse::<i64>().is_ok())
        {
            continue;
        }
        let source_path = source.path();
        assert_direct_child(cache, &source_path, &source_name)?;
        crate::transaction::assert_plain_directory(&source_path, "Plugin download source")?;
        for entry in fs::read_dir(&source_path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let filename = entry.file_name().to_string_lossy().into_owned();
            let candidate = filename.strip_prefix('.').unwrap_or(&filename);
            let artifact = candidate.strip_suffix(".part").unwrap_or(candidate);
            let Some(version) = artifact
                .strip_prefix(&format!("{name}-"))
                .and_then(|value| value.strip_suffix(".zip"))
            else {
                continue;
            };
            if semver::Version::parse(version).is_err() {
                continue;
            }
            assert_direct_child(&source_path, &entry.path(), &filename)?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("INVALID_PLUGIN_CACHE_FILE".into());
            }
            fs::remove_file(entry.path())
                .map_err(|error| format!("PLUGIN_CACHE_REMOVE_FAILED: {error}"))?;
        }
    }
    Ok(())
}

pub(crate) fn remove_empty_retained_directory(path: &Path) -> Result<(), String> {
    if fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .next()
        .is_none()
    {
        fs::remove_dir(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_uninstalled_user_files_outside_plugins_and_resumes_without_overwrite() {
        let temporary = tempfile::tempdir().unwrap();
        let plugins = temporary.path().join("plugins");
        let retained = plugins.join(format!(".acme.retained-{}", "a".repeat(32)));
        fs::create_dir_all(&retained).unwrap();
        fs::write(
            retained.join(crate::journal::JOURNAL_FILENAME),
            "committed journal",
        )
        .unwrap();
        fs::write(retained.join("用户文档.txt"), "original").unwrap();
        let target = retain_user_files(&plugins, &retained, "acme").unwrap();
        assert!(target.starts_with(temporary.path().join("plugin-data/acme/uninstalled-files")));
        assert_eq!(
            fs::read_to_string(target.join("用户文档.txt")).unwrap(),
            "original"
        );
        assert!(retained.join(crate::journal::JOURNAL_FILENAME).exists());
        fs::write(retained.join("next.txt"), "remaining after interruption").unwrap();
        assert_eq!(
            retain_user_files(&plugins, &retained, "acme").unwrap(),
            target
        );
        assert_eq!(
            fs::read_to_string(target.join("next.txt")).unwrap(),
            "remaining after interruption"
        );
        fs::write(retained.join("用户文档.txt"), "new file").unwrap();
        assert!(retain_user_files(&plugins, &retained, "acme")
            .unwrap_err()
            .starts_with("RETAINED_USER_FILE_EXISTS"));
        assert_eq!(
            fs::read_to_string(target.join("用户文档.txt")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_to_string(retained.join("用户文档.txt")).unwrap(),
            "new file"
        );
    }

    #[test]
    fn removes_only_exact_owner_cache_artifacts_and_keeps_similarly_named_plugins() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        let official = cache.join("official");
        fs::create_dir_all(&official).unwrap();
        for name in [
            "acme-1.0.0.zip",
            ".acme-2.0.0-beta.1.zip.part",
            "acme-other-1.0.0.zip",
            "other-1.0.0.zip",
        ] {
            fs::write(official.join(name), "fixture").unwrap();
        }
        remove_cached_packages(&cache, "acme").unwrap();
        assert!(!official.join("acme-1.0.0.zip").exists());
        assert!(!official.join(".acme-2.0.0-beta.1.zip.part").exists());
        assert!(official.join("acme-other-1.0.0.zip").exists());
        assert!(official.join("other-1.0.0.zip").exists());
    }

    #[test]
    fn removes_exclusive_resources_without_touching_user_data_or_another_owner() {
        let root = tempfile::tempdir().unwrap();
        let plugins = root.path().join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        for directory in [
            "plugin-resources/acme",
            "plugin-resources/other",
            "plugin-data/acme",
        ] {
            fs::create_dir_all(root.path().join(directory)).unwrap();
            fs::write(root.path().join(directory).join("keep.txt"), "original").unwrap();
        }
        cleanup(&plugins, None, &root.path().join("cache"), "acme").unwrap();
        assert!(!root.path().join("plugin-resources/acme").exists());
        assert!(root.path().join("plugin-resources/other/keep.txt").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("plugin-data/acme/keep.txt")).unwrap(),
            "original"
        );
    }
}

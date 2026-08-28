//! Small file-backed cache/model helpers for Document Engine.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_LIST_ENTRIES: usize = 10_000;
const CACHE_SCHEMA_VERSION: u32 = 1;
const MAX_MODEL_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

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

/// Compute a streaming SHA-256 for a local source file. The cache key is
/// content based, not path based, so moving a document does not invalidate it.
pub fn file_hash(path: &Path) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("打开缓存源文件失败: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|error| format!("读取缓存源文件失败: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
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

pub fn clear_directory(path: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(path).map_err(|error| format!("创建缓存目录失败: {error}"))?;
    let mut removed = 0usize;
    let entries = std::fs::read_dir(path).map_err(|error| format!("读取缓存目录失败: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取缓存条目失败: {error}"))?;
        let target = entry.path();
        if target.is_dir() {
            std::fs::remove_dir_all(&target)
                .map_err(|error| format!("删除缓存目录失败: {error}"))?;
        } else {
            std::fs::remove_file(&target).map_err(|error| format!("删除缓存文件失败: {error}"))?;
        }
        removed += 1;
    }
    Ok(removed)
}

pub fn list_files(root: &Path) -> Result<Vec<Value>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|left, right| {
        left["relativePath"]
            .as_str()
            .unwrap_or("")
            .cmp(right["relativePath"].as_str().unwrap_or(""))
    });
    Ok(files)
}

pub fn remove_child(root: &Path, child: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("模型目录不可用: {error}"))?;
    let child = child
        .canonicalize()
        .map_err(|error| format!("模型文件不存在: {error}"))?;
    if child == root || !child.starts_with(&root) {
        return Err("只能删除模型目录内的文件或子目录".into());
    }
    if child.is_dir() {
        std::fs::remove_dir_all(child).map_err(|error| format!("删除模型目录失败: {error}"))
    } else {
        std::fs::remove_file(child).map_err(|error| format!("删除模型文件失败: {error}"))
    }
}

pub fn install_local(root: &Path, source: &Path, name: &str) -> Result<PathBuf, String> {
    validate_model_name(name)?;
    let source_metadata =
        std::fs::symlink_metadata(source).map_err(|error| format!("模型源不存在: {error}"))?;
    if source_metadata.file_type().is_symlink() {
        return Err("不接受符号链接模型源".into());
    }
    let source = source
        .canonicalize()
        .map_err(|error| format!("模型源不存在: {error}"))?;
    if !source.is_file() && !source.is_dir() {
        return Err("模型源必须是文件或目录".into());
    }
    std::fs::create_dir_all(root).map_err(|error| format!("创建模型目录失败: {error}"))?;
    let target = root.join(name);
    if target.exists() {
        return Err("目标模型已存在，为避免覆盖请先删除旧模型".into());
    }
    if source.is_file() {
        std::fs::copy(&source, &target).map_err(|error| format!("复制模型失败: {error}"))?;
    } else {
        copy_directory(&source, &target)?;
    }
    Ok(target)
}

fn validate_model_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 256
        || name.contains(['\\', '/', ':'])
        || name.contains("..")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("模型名称不安全".into());
    }
    Ok(())
}

fn validate_model_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") || url.contains('@') {
        return Err("模型下载地址必须使用 HTTPS 且不能包含凭据".into());
    }
    let host = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let allowed = [
        "github.com",
        "raw.githubusercontent.com",
        "huggingface.co",
        "cdn-lfs.huggingface.co",
        "paddle-model-ecology.bj.bcebos.com",
    ];
    if !allowed
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        return Err("模型下载地址域名不在允许列表中".into());
    }
    Ok(())
}

/// Download one model file with a strict host allow-list and SHA-256
/// verification, then atomically install it. The overwrite flag is used by
/// model updates and never writes directly into an existing model file.
pub fn install_remote(
    root: &Path,
    url: &str,
    name: &str,
    expected_sha256: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
    validate_model_name(name)?;
    validate_model_url(url)?;
    if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("必须提供 64 位 SHA-256 模型校验值".into());
    }
    std::fs::create_dir_all(root).map_err(|error| format!("创建模型目录失败: {error}"))?;
    let target = root.join(name);
    if target.exists() && !overwrite {
        return Err("目标模型已存在，为避免覆盖请先使用更新或删除旧模型".into());
    }
    let part = root.join(format!(".{name}.download"));
    let _ = std::fs::remove_file(&part);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("下载模型失败: {error}"))?;
    validate_model_url(response.get_url())?;
    let content_length = response
        .header("content-length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if content_length > MAX_MODEL_DOWNLOAD_BYTES {
        return Err("模型下载响应超过 512 MiB 限制".into());
    }
    let mut reader = response.into_reader();
    let mut file =
        std::fs::File::create(&part).map_err(|error| format!("创建模型临时文件失败: {error}"))?;
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut reader, &mut buffer)
            .map_err(|error| format!("读取模型下载失败: {error}"))?;
        if read == 0 {
            break;
        }
        bytes = bytes.saturating_add(read as u64);
        if bytes > MAX_MODEL_DOWNLOAD_BYTES {
            let _ = std::fs::remove_file(&part);
            return Err("模型下载内容超过 512 MiB 限制".into());
        }
        std::io::Write::write_all(&mut file, &buffer[..read])
            .map_err(|error| format!("写入模型临时文件失败: {error}"))?;
        digest.update(&buffer[..read]);
    }
    file.sync_all()
        .map_err(|error| format!("刷新模型临时文件失败: {error}"))?;
    let actual = format!("{:x}", digest.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "模型 SHA-256 校验失败：期望 {expected_sha256}，实际 {actual}"
        ));
    }
    if overwrite && target.exists() {
        std::fs::remove_file(&target).map_err(|error| format!("替换旧模型失败: {error}"))?;
    }
    if let Err(error) = std::fs::rename(&part, &target) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("提交模型失败: {error}"));
    }
    Ok(target)
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|error| format!("创建模型子目录失败: {error}"))?;
    for entry in std::fs::read_dir(source).map_err(|error| format!("读取模型源失败: {error}"))?
    {
        let entry = entry.map_err(|error| format!("读取模型源条目失败: {error}"))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| format!("读取模型源条目类型失败: {error}"))?
            .is_symlink()
        {
            return Err(format!(
                "模型源包含不支持的符号链接: {}",
                source_path.display()
            ));
        }
        if source_path.is_dir() {
            copy_directory(&source_path, &target_path)?;
        } else if source_path.is_file() {
            std::fs::copy(&source_path, &target_path)
                .map_err(|error| format!("复制模型文件失败: {error}"))?;
        }
    }
    Ok(())
}

fn collect_files(root: &Path, current: &Path, output: &mut Vec<Value>) -> Result<(), String> {
    if output.len() >= MAX_LIST_ENTRIES {
        return Err("模型文件数量超过限制".into());
    }
    for entry in std::fs::read_dir(current).map_err(|error| format!("读取模型目录失败: {error}"))?
    {
        let entry = entry.map_err(|error| format!("读取模型条目失败: {error}"))?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| format!("读取模型条目类型失败: {error}"))?
            .is_symlink()
        {
            return Err(format!("模型目录包含不支持的符号链接: {}", path.display()));
        }
        if path.is_dir() {
            collect_files(root, &path, output)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = entry
                .metadata()
                .map_err(|error| format!("读取模型元数据失败: {error}"))?;
            output.push(json!({
                "relativePath": relative,
                "path": path.to_string_lossy(),
                "bytes": metadata.len(),
                "extension": path.extension().and_then(|value| value.to_str()).unwrap_or("")
            }));
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn resolve_child(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() || relative.contains("..") {
        return Err("模型相对路径不安全".into());
    }
    Ok(root.join(relative_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "cb-doc-cache-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cache_key_is_stable() {
        let one = cache_key("abc", "native", "1", &json!({ "a": 1 }));
        let two = cache_key("abc", "native", "1", &json!({ "a": 1 }));
        assert_eq!(one, two);
        assert_eq!(one.len(), 64);
    }

    #[test]
    fn clear_and_list_are_scoped() {
        let dir = temp_dir("clear");
        std::fs::write(dir.join("a.bin"), b"a").unwrap();
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested/b.bin"), b"b").unwrap();
        assert_eq!(list_files(&dir).unwrap().len(), 2);
        assert_eq!(clear_directory(&dir).unwrap(), 2);
        assert!(list_files(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn result_cache_roundtrip_and_corrupt_entry_is_miss() {
        let dir = temp_dir("roundtrip");
        let key = cache_key("source", "engine", "1", &json!({ "x": true }));
        let value = json!({ "text": "cached" });
        write_result(&dir, &key, &value).unwrap();
        assert_eq!(read_result(&dir, &key).unwrap(), Some(value));
        std::fs::write(dir.join(format!("{key}.json")), b"not-json").unwrap();
        assert_eq!(read_result(&dir, &key).unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn remote_model_url_is_allowlisted() {
        assert!(validate_model_url("https://github.com/example/model.onnx").is_ok());
        assert!(validate_model_url("https://huggingface.co/example/model.onnx").is_ok());
        assert!(validate_model_url("http://github.com/example/model.onnx").is_err());
        assert!(validate_model_url("https://example.invalid/model.onnx").is_err());
        assert!(validate_model_name("../model.onnx").is_err());
    }
}

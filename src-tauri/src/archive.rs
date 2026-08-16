// 插件 zip 安全策略 + 预算 + 解压（1.9.3，对等 plugin-system/PluginArchivePolicy.ts）
// 约束：零 unwrap/expect（panic=abort），错误一律 Result<_, String>。
#![allow(dead_code)] // 1.9.3 安装链后续步骤接入后消除

use std::collections::HashSet;
use std::path::Path;

/// zip 文件最大字节数（对等 MAX_PLUGIN_ZIP_BYTES）
pub const MAX_ZIP_BYTES: u64 = 200 * 1024 * 1024;

/// 归档条目数上限（对等 MAX_PLUGIN_ARCHIVE_ENTRIES）
pub const MAX_ARCHIVE_ENTRIES: usize = 5_000;

/// 解压总字节预算（对等 MAX_PLUGIN_ARCHIVE_BYTES）
pub const MAX_ARCHIVE_BYTES: u64 = 600 * 1024 * 1024;

/// 规范化归档条目路径（对等 normalizeArchiveEntryPath）
pub fn normalize_archive_entry_path(raw: &str) -> Result<String, String> {
    let portable = raw.replace('\\', "/");
    let without_trailing = portable.strip_suffix('/').unwrap_or(&portable);
    let segments: Vec<&str> = without_trailing.split('/').collect();
    let unsafe_path = without_trailing.is_empty()
        || utf16_len(without_trailing) > 512
        || portable.starts_with('/')
        || is_win32_absolute(raw)
        || contains_control(raw)
        || segments.iter().any(|segment| {
            segment.is_empty()
                || *segment == "."
                || *segment == ".."
                || segment.ends_with('.')
                || segment.ends_with(' ')
                || segment
                    .chars()
                    .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
                || is_windows_reserved_basename(segment)
        })
        || posix_normalize(without_trailing) != without_trailing;
    if unsafe_path {
        return Err(format!("Plugin archive contains an unsafe path: {raw}"));
    }
    Ok(without_trailing.to_string())
}

/// 解压插件 zip 到 dest（对等 extractPluginArchive）
pub fn extract_plugin_archive(zip_path: &Path, dest: &Path) -> Result<(), String> {
    // zip 文件必须常规文件非 symlink、≤200MB
    let metadata = std::fs::symlink_metadata(zip_path)
        .map_err(|error| format!("Plugin package must be a regular ZIP file: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Plugin package must be a regular ZIP file".to_string());
    }
    if metadata.len() > MAX_ZIP_BYTES {
        return Err("Plugin package is too large".to_string());
    }
    let file = std::fs::File::open(zip_path)
        .map_err(|error| format!("failed to open plugin package: {error}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("invalid ZIP archive: {error}"))?;

    // 条目数 1..5000
    let entry_count = archive.len();
    if entry_count == 0 || entry_count > MAX_ARCHIVE_ENTRIES {
        return Err("Plugin archive has an invalid entry count".to_string());
    }
    std::fs::create_dir_all(dest)
        .map_err(|error| format!("failed to create destination directory: {error}"))?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut total_bytes: u64 = 0;
    for index in 0..entry_count {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("failed to read archive entry {index}: {error}"))?;
        let name = entry.name().to_string();
        let normalized = normalize_archive_entry_path(&name)?;

        // 大小写不敏感去重
        let path_key = normalized.to_lowercase();
        if !seen.insert(path_key) {
            return Err(format!(
                "Plugin archive contains duplicate paths: {normalized}"
            ));
        }

        // 拒绝符号链接（unix_mode & 0o170000 == 0o120000）
        if let Some(mode) = entry.unix_mode() {
            if (mode & 0o170000) == 0o120000 {
                return Err(format!(
                    "Plugin archive contains a symbolic link: {normalized}"
                ));
            }
        }

        // enclosed_name 防穿越（返回 None 时拒绝）
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("Plugin archive contains an unsafe path: {normalized}"))?;

        // 解压总字节 ≤600MB（流式计数，溢出检查）
        let size = entry.size();
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| "Plugin archive exceeds its uncompressed byte budget".to_string())?;
        if total_bytes > MAX_ARCHIVE_BYTES {
            return Err("Plugin archive exceeds its uncompressed byte budget".to_string());
        }

        let destination = dest.join(&enclosed);
        if entry.is_dir() {
            std::fs::create_dir_all(&destination)
                .map_err(|error| format!("failed to create directory {destination:?}: {error}"))?;
        } else {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create parent directory: {error}"))?;
            }
            let mut output = std::fs::File::create(&destination)
                .map_err(|error| format!("failed to create file {destination:?}: {error}"))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| format!("failed to extract {normalized}: {error}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

/// 控制字符：code ≤0x1f 或 0x7f（对等 containsControlCharacter）
fn contains_control(value: &str) -> bool {
    value
        .chars()
        .any(|c| (c as u32) <= 0x1f || (c as u32) == 0x7f)
}

/// UTF-16 code unit 计数（对等 JS String.prototype.length）
fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// win32 绝对路径：`C:\...` / `C:/...` / `\\server\share`（对等 win32.isAbsolute）
fn is_win32_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
    {
        return true;
    }
    if bytes.len() >= 2 && bytes[0] == b'\\' && bytes[1] == b'\\' {
        return true;
    }
    false
}

/// posix.normalize 的等价实现（对等 node:path posix.normalize）
fn posix_normalize(value: &str) -> String {
    if value.is_empty() {
        return ".".to_string();
    }
    let mut out: Vec<&str> = Vec::new();
    for segment in value.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

/// Windows 保留名：con|prn|aux|nul|com[1-9]|lpt[1-9]，大小写不敏感，
/// 取 basename 首个 `.` 前部分比对（对等 WINDOWS_RESERVED_BASENAME）
fn is_windows_reserved_basename(segment: &str) -> bool {
    let base = segment.split('.').next().unwrap_or("");
    let lower = base.to_ascii_lowercase();
    if matches!(lower.as_str(), "con" | "prn" | "aux" | "nul") {
        return true;
    }
    if lower.len() == 4 {
        let (prefix, digit) = lower.split_at(3);
        if (prefix == "com" || prefix == "lpt") && digit.len() == 1 {
            if let Some(byte) = digit.as_bytes().first() {
                if (b'1'..=b'9').contains(byte) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn expect_ok<T>(result: Result<T, String>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok, got Err: {error}"),
        }
    }

    fn expect_err<T>(result: Result<T, String>) -> String {
        match result {
            Err(error) => error,
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    fn io_ok<T>(result: std::io::Result<T>) -> Result<T, String> {
        result.map_err(|error| error.to_string())
    }

    fn temp_dir(tag: &str) -> Result<std::path::PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "cruciblebox-archive-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }

    fn find_signature(bytes: &[u8], signature: &[u8]) -> Option<usize> {
        bytes
            .windows(signature.len())
            .position(|window| window == signature)
    }

    fn make_zip<S: AsRef<str>>(entries: &[(S, &[u8])]) -> Result<Vec<u8>, String> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            for (name, data) in entries {
                writer
                    .start_file(name.as_ref(), options)
                    .map_err(|error| error.to_string())?;
                writer.write_all(data).map_err(|error| error.to_string())?;
            }
            writer.finish().map_err(|error| error.to_string())?;
        }
        Ok(buffer.into_inner())
    }

    /// 生成单条目 zip 并把 central directory 的 external attributes 打成 unix 符号链接模式
    fn make_symlink_zip(name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut bytes = make_zip(&[(name, data)])?;
        let central = find_signature(&bytes, &[0x50, 0x4b, 0x01, 0x02])
            .ok_or_else(|| "central directory header not found".to_string())?;
        let external_attributes = 0o120777u32 << 16;
        bytes[central + 38..central + 42].copy_from_slice(&external_attributes.to_le_bytes());
        Ok(bytes)
    }

    /// 把首条目的 uncompressed size（local header + central directory）改成超大值
    fn patch_uncompressed_size(bytes: &mut [u8], new_size: u32) -> Result<(), String> {
        let local = find_signature(bytes, &[0x50, 0x4b, 0x03, 0x04])
            .ok_or_else(|| "local file header not found".to_string())?;
        bytes[local + 22..local + 26].copy_from_slice(&new_size.to_le_bytes());
        let central = find_signature(bytes, &[0x50, 0x4b, 0x01, 0x02])
            .ok_or_else(|| "central directory header not found".to_string())?;
        bytes[central + 24..central + 28].copy_from_slice(&new_size.to_le_bytes());
        Ok(())
    }

    #[test]
    fn normalize_path_accepts_safe() {
        assert_eq!(expect_ok(normalize_archive_entry_path("a/b.js")), "a/b.js");
        assert_eq!(expect_ok(normalize_archive_entry_path("a\\b.js")), "a/b.js");
        assert_eq!(expect_ok(normalize_archive_entry_path("dir/")), "dir");
        assert_eq!(expect_ok(normalize_archive_entry_path("a/b/")), "a/b");
        assert_eq!(
            expect_ok(normalize_archive_entry_path("a/b/c.txt")),
            "a/b/c.txt"
        );
    }

    #[test]
    fn normalize_path_rejects_unsafe() {
        for bad in [
            "",
            "../evil.js",
            "a/../../evil.js",
            "/abs",
            "C:\\evil",
            "a//b",
            "a/./b",
            "a/b.",
            "a/b ",
            "a/b<c",
            "a/b>c",
            "a/b:c",
            "a/b\"c",
            "a/b|c",
            "a/b?c",
            "a/b*c",
            "con.txt",
            "COM1",
            "aux",
            "nul",
            "lpt9",
            "a/con.txt",
            "a/COM2/x",
        ] {
            assert!(
                normalize_archive_entry_path(bad).is_err(),
                "path {bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn archive_duplicate_paths_rejected() {
        let dir = expect_ok(temp_dir("archive_dup"));
        let zip_path = dir.join("dup.zip");
        let bytes = expect_ok(make_zip(&[("a.js", b"1"), ("A.JS", b"2")]));
        expect_ok(io_ok(std::fs::write(&zip_path, &bytes)));
        let error = expect_err(extract_plugin_archive(&zip_path, &dir.join("out")));
        assert!(error.contains("duplicate paths"), "error: {error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_symlink_rejected() {
        let dir = expect_ok(temp_dir("archive_symlink"));
        let zip_path = dir.join("link.zip");
        let bytes = expect_ok(make_symlink_zip("link.js", b"x"));
        expect_ok(io_ok(std::fs::write(&zip_path, &bytes)));
        let error = expect_err(extract_plugin_archive(&zip_path, &dir.join("out")));
        assert!(error.contains("symbolic link"), "error: {error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_budget_rejected() {
        let dir = expect_ok(temp_dir("archive_budget"));
        let zip_path = dir.join("pkg.zip");
        let mut bytes = expect_ok(make_zip(&[("a.js", b"x")]));
        expect_ok(patch_uncompressed_size(&mut bytes, 700 * 1024 * 1024));
        expect_ok(io_ok(std::fs::write(&zip_path, &bytes)));
        let error = expect_err(extract_plugin_archive(&zip_path, &dir.join("out")));
        assert!(error.contains("byte budget"), "error: {error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_entry_count_bounds() {
        let dir = expect_ok(temp_dir("archive_count"));
        // 0 条目
        let empty_path = dir.join("empty.zip");
        let bytes = expect_ok(make_zip::<&str>(&[]));
        expect_ok(io_ok(std::fs::write(&empty_path, &bytes)));
        let error = expect_err(extract_plugin_archive(&empty_path, &dir.join("out")));
        assert!(error.contains("entry count"), "error: {error}");

        // 超过 5000 条目
        let mut entries: Vec<(String, &[u8])> = Vec::new();
        for index in 0..5001 {
            entries.push((format!("f{index}.js"), b"x"));
        }
        let many_path = dir.join("many.zip");
        let bytes = expect_ok(make_zip(&entries));
        expect_ok(io_ok(std::fs::write(&many_path, &bytes)));
        let error = expect_err(extract_plugin_archive(&many_path, &dir.join("out")));
        assert!(error.contains("entry count"), "error: {error}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_must_be_regular_file() {
        let dir = expect_ok(temp_dir("archive_regular"));
        let zip_path = dir.join("pkg.zip");
        expect_ok(io_ok(std::fs::create_dir_all(&zip_path)));
        let error = expect_err(extract_plugin_archive(&zip_path, &dir.join("out")));
        assert!(error.contains("regular ZIP file"), "error: {error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_extracts_successfully() {
        let dir = expect_ok(temp_dir("archive_ok"));
        let zip_path = dir.join("ok.zip");
        let bytes = expect_ok(make_zip(&[
            ("main.js", b"console.log(1)"),
            ("lib/util.js", b"// util"),
        ]));
        expect_ok(io_ok(std::fs::write(&zip_path, &bytes)));
        let dest = dir.join("out");
        expect_ok(extract_plugin_archive(&zip_path, &dest));
        assert!(dest.join("main.js").exists());
        assert!(dest.join("lib").join("util.js").exists());
        let content = expect_ok(io_ok(std::fs::read_to_string(dest.join("main.js"))));
        assert_eq!(content, "console.log(1)");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! Host side of the official `archive-extractor` plugin.
//!
//! The plugin renderer owns the user flow.  This module owns the parts that
//! must not be implemented in JavaScript: locating the bundled 7-Zip binary,
//! validating archive member paths, running a cancellable extraction process,
//! and committing a staged result to the destination directory.

use crate::unienv_task::{TaskContext, TaskManager};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const RESOURCE_KEY: &str = "archive-extraction";
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 500_000;
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024 * 1024;
const MAX_LIST_OUTPUT_BYTES: u64 = 256 * 1024 * 1024;
const LOW_DISK_RESERVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ERROR_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
struct ArchiveEntry {
    path: String,
    is_dir: bool,
    size: u64,
    encrypted: bool,
}

#[derive(Clone, Debug)]
struct ArchiveInfo {
    source: PathBuf,
    format: String,
    source_size: u64,
    entries: Vec<ArchiveEntry>,
    file_count: usize,
    directory_count: usize,
    unpacked_bytes: u64,
    encrypted: bool,
}

pub struct Service {
    tasks: Arc<TaskManager>,
}
impl Service {
    pub fn new(runtime: Arc<crate::task_runtime::TaskRuntime>) -> Self {
        Self {
            tasks: Arc::new(TaskManager::with_runtime("archive-extractor", runtime)),
        }
    }
}

fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

fn failure(code: &str, message: impl Into<String>) -> Value {
    json!({ "ok": false, "code": code, "message": message.into() })
}

fn payload_string(payload: Option<&Value>, key: &str) -> Result<String, String> {
    payload
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{key} is required"))
}

fn payload_bool(payload: Option<&Value>, key: &str, default: bool) -> bool {
    payload
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn source_path(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err("archive source must be an absolute path".into());
    }
    let metadata =
        fs::symlink_metadata(&path).map_err(|error| format!("无法读取归档文件：{error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("归档源必须是普通文件，不能是目录或符号链接".into());
    }
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(format!(
            "归档文件超过大小上限（{} GiB）",
            MAX_ARCHIVE_BYTES / 1024 / 1024 / 1024
        ));
    }
    fs::canonicalize(&path).map_err(|error| format!("无法解析归档路径：{error}"))
}

fn destination_path(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err("解压目录必须是绝对路径".into());
    }
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("解压目录必须是普通目录，不能是符号链接".into());
        }
    } else {
        fs::create_dir_all(&path).map_err(|error| format!("无法创建解压目录：{error}"))?;
    }
    let metadata =
        fs::symlink_metadata(&path).map_err(|error| format!("无法读取解压目录：{error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("解压目录必须是普通目录".into());
    }
    fs::canonicalize(&path).map_err(|error| format!("无法解析解压目录：{error}"))
}

fn seven_zip_path() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("CRUCIBLEBOX_7Z_PATH") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("resources").join("7zip").join("7za.exe"));
            candidates.push(parent.join("7zip").join("7za.exe"));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("7zip")
            .join("7za.exe"),
    );

    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let dll = candidate.parent().map(|parent| parent.join("7za.dll"));
        if dll.as_ref().is_some_and(|path| path.is_file()) {
            return Ok(candidate);
        }
    }

    Err("快速解压运行时缺少内置 7-Zip 组件，请重新安装工具箱".into())
}

fn seven_zip_args(source: &Path, password: Option<&str>) -> Vec<String> {
    let mut args = vec!["-sccUTF-8".into(), "-bb0".into(), "-y".into()];
    if let Some(password) = password.filter(|value| !value.is_empty()) {
        // 密码只存在于当前请求和子进程生命周期内，不写入配置、日志或任务快照。
        args.push(format!("-p{password}"));
    }
    args.push(source.to_string_lossy().into_owned());
    args
}

fn run_listing(source: &Path, password: Option<&str>) -> Result<Output, String> {
    let executable = seven_zip_path()?;
    let output = Command::new(executable)
        .arg("l")
        .arg("-slt")
        .args(seven_zip_args(source, password))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("无法启动内置 7-Zip：{error}"))?;
    if output.stdout.len() as u64 + output.stderr.len() as u64 > MAX_LIST_OUTPUT_BYTES {
        return Err("归档目录清单过大，已停止读取".into());
    }
    Ok(output)
}

fn looks_like_password_error(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("encrypted headers")
        || lower.contains("wrong password")
        || lower.contains("can not open encrypted")
}

fn normalize_member_path(raw: &str) -> Result<String, String> {
    let replaced = raw.replace('\\', "/");
    if replaced.is_empty()
        || replaced.starts_with('/')
        || replaced.starts_with("//")
        || replaced
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_control())
    {
        return Err(format!("归档包含不安全路径：{raw}"));
    }
    let first = replaced.split('/').next().unwrap_or_default();
    if first.len() >= 2 && first.as_bytes()[1] == b':' {
        return Err(format!("归档包含绝对路径：{raw}"));
    }

    let mut parts = Vec::new();
    for part in replaced.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".."
            || part.contains(':')
            || part.chars().any(|character| {
                character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
            })
            || part.ends_with(' ')
            || part.ends_with('.')
            || is_reserved_windows_name(part)
        {
            return Err(format!("归档包含不安全路径：{raw}"));
        }
        if part.len() > 240 {
            return Err("归档成员名称过长".into());
        }
        parts.push(part.to_string());
    }
    if parts.is_empty() {
        return Err(format!("归档包含空路径：{raw}"));
    }
    let normalized = parts.join("/");
    if normalized.len() > 32_000 {
        return Err("归档成员路径过长".into());
    }
    Ok(normalized)
}

fn is_reserved_windows_name(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn parse_listing(source: &Path, output: &Output) -> Result<ArchiveInfo, Value> {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        if looks_like_password_error(&combined) {
            return Err(failure("password-required", "此归档需要密码"));
        }
        return Err(failure(
            "inspect-failed",
            format!("无法读取归档：{}", compact_error(&combined)),
        ));
    }

    let mut format = String::new();
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut after_separator = false;
    let mut current_path: Option<String> = None;
    let mut current_folder = false;
    let mut current_size = 0_u64;
    let mut current_encrypted = false;

    let flush = |entries: &mut Vec<ArchiveEntry>,
                 seen: &mut HashSet<String>,
                 current_path: &mut Option<String>,
                 current_folder: &mut bool,
                 current_size: &mut u64,
                 current_encrypted: &mut bool|
     -> Result<(), Value> {
        let Some(raw_path) = current_path.take() else {
            return Ok(());
        };
        let path =
            normalize_member_path(&raw_path).map_err(|message| failure("unsafe-path", message))?;
        let folded = path.to_lowercase();
        if !seen.insert(folded) {
            return Err(failure("unsafe-path", format!("归档包含重复路径：{path}")));
        }
        if entries.len() >= MAX_ENTRIES {
            return Err(failure("archive-too-large", "归档成员数量超过上限"));
        }
        entries.push(ArchiveEntry {
            path,
            is_dir: *current_folder,
            size: *current_size,
            encrypted: *current_encrypted,
        });
        *current_folder = false;
        *current_size = 0;
        *current_encrypted = false;
        Ok(())
    };

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        if trimmed == "----------" {
            after_separator = true;
            continue;
        }
        let Some((key, value)) = trimmed.split_once(" = ") else {
            continue;
        };
        match key {
            "Type" if !after_separator && format.is_empty() => format = value.to_string(),
            "Path" => {
                if after_separator {
                    flush(
                        &mut entries,
                        &mut seen,
                        &mut current_path,
                        &mut current_folder,
                        &mut current_size,
                        &mut current_encrypted,
                    )?;
                    current_path = Some(value.to_string());
                }
            }
            "Folder" if after_separator => current_folder = value == "+",
            "Size" if after_separator => current_size = value.parse().unwrap_or(0),
            "Encrypted" if after_separator => current_encrypted = value == "+",
            _ => {}
        }
    }
    flush(
        &mut entries,
        &mut seen,
        &mut current_path,
        &mut current_folder,
        &mut current_size,
        &mut current_encrypted,
    )?;

    let source_size = fs::metadata(source)
        .map_err(|error| failure("inspect-failed", format!("无法读取归档大小：{error}")))?
        .len();
    let unpacked_bytes = entries
        .iter()
        .try_fold(0_u64, |total, entry| total.checked_add(entry.size))
        .ok_or_else(|| failure("archive-too-large", "归档展开大小溢出"))?;
    if unpacked_bytes > MAX_UNPACKED_BYTES {
        return Err(failure("archive-too-large", "归档展开大小超过上限"));
    }
    let file_count = entries.iter().filter(|entry| !entry.is_dir).count();
    let directory_count = entries.iter().filter(|entry| entry.is_dir).count();
    let encrypted = entries.iter().any(|entry| entry.encrypted);
    Ok(ArchiveInfo {
        source: source.to_path_buf(),
        format: if format.is_empty() {
            source
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("archive")
                .to_string()
        } else {
            format
        },
        source_size,
        entries,
        file_count,
        directory_count,
        unpacked_bytes,
        encrypted,
    })
}

fn inspect_archive(source: &Path, password: Option<&str>) -> Result<ArchiveInfo, Value> {
    let output =
        run_listing(source, password).map_err(|message| failure("tool-missing", message))?;
    parse_listing(source, &output)
}

fn root_names(info: &ArchiveInfo) -> Vec<String> {
    let mut names = HashSet::new();
    for entry in &info.entries {
        if let Some(root) = entry.path.split('/').next() {
            names.insert(root.to_string());
        }
    }
    let mut result: Vec<_> = names.into_iter().collect();
    result.sort_by_key(|value| value.to_lowercase());
    result
}

fn strip_archive_suffix(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for suffix in [
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tar", ".zip", ".7z", ".rar", ".cab", ".gz",
        ".bz2", ".xz",
    ] {
        if lower.ends_with(suffix) && name.len() > suffix.len() {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    name.to_string()
}

fn suggested_destination(source: &Path) -> PathBuf {
    let stem = source
        .file_name()
        .and_then(|value| value.to_str())
        .map(strip_archive_suffix)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "解压结果".into());
    source.parent().unwrap_or_else(|| Path::new(".")).join(stem)
}

fn available_bytes(path: &Path) -> u64 {
    let Ok(canonical) = fs::canonicalize(path) else {
        return 0;
    };
    let mut best_mount: Option<&Path> = None;
    let mut best_available = 0_u64;
    for disk in sysinfo::Disks::new_with_refreshed_list().iter() {
        let mount = disk.mount_point();
        if canonical.starts_with(mount)
            && best_mount.is_none_or(|current| mount.as_os_str().len() > current.as_os_str().len())
        {
            best_mount = Some(mount);
            best_available = disk.available_space();
        }
    }
    best_available
}

fn public_info(info: &ArchiveInfo) -> Value {
    let preview = info
        .entries
        .iter()
        .take(100)
        .map(|entry| {
            json!({
                "path": entry.path,
                "isDirectory": entry.is_dir,
                "size": entry.size,
                "encrypted": entry.encrypted,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "source": info.source,
        "format": info.format,
        "sourceSize": info.source_size,
        "fileCount": info.file_count,
        "directoryCount": info.directory_count,
        "unpackedBytes": info.unpacked_bytes,
        "encrypted": info.encrypted,
        "suggestedDestination": suggested_destination(&info.source),
        "topLevel": root_names(info),
        "entriesPreview": preview,
    })
}

fn preflight(info: &ArchiveInfo, destination: &Path) -> Value {
    let available = available_bytes(destination);
    let required = info.unpacked_bytes.saturating_add(LOW_DISK_RESERVE_BYTES);
    let conflicts = root_names(info)
        .into_iter()
        .filter(|name| destination.join(name).exists())
        .collect::<Vec<_>>();
    ok(json!({
        "info": public_info(info),
        "destination": destination,
        "conflicts": conflicts,
        "disk": {
            "availableBytes": available,
            "requiredBytes": required,
            "low": available != 0 && available < required,
        }
    }))
}

fn validate_policy(value: Option<&Value>) -> Result<String, String> {
    let policy = value
        .and_then(Value::as_str)
        .unwrap_or("ask")
        .trim()
        .to_ascii_lowercase();
    if matches!(policy.as_str(), "ask" | "overwrite" | "skip" | "rename") {
        Ok(policy)
    } else {
        Err("冲突策略无效".into())
    }
}

fn spawn_reader<R: Read + Send + 'static>(reader: R, sender: Sender<StreamEvent>) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let _ = sender.send(StreamEvent::Line(line));
        }
        let _ = sender.send(StreamEvent::Done);
    });
}

enum StreamEvent {
    Line(String),
    Done,
}

fn update_progress(ctx: &TaskContext, line: &str) {
    let Some(percent_end) = line.find('%') else {
        return;
    };
    let digits = line[..percent_end]
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if let Ok(percent) = digits.parse::<u32>() {
        ctx.update_progress("extracting", percent.min(100), "正在解压文件…");
    }
}

fn run_extraction(
    ctx: &TaskContext,
    source: &Path,
    staging: &Path,
    password: Option<&str>,
) -> Result<(), String> {
    let executable = seven_zip_path()?;
    let mut args = vec![
        "x".into(),
        "-y".into(),
        "-snld".into(),
        "-bsp1".into(),
        "-sccUTF-8".into(),
        format!("-o{}", staging.to_string_lossy()),
    ];
    if let Some(password) = password.filter(|value| !value.is_empty()) {
        args.push(format!("-p{password}"));
    }
    args.push(source.to_string_lossy().into_owned());

    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动解压进程：{error}"))?;
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        spawn_reader(stdout, sender.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader(stderr, sender);
    }

    let mut finished_streams = 0;
    let mut error_output = String::new();
    let mut status = None;
    loop {
        if ctx.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("操作已取消".into());
        }
        loop {
            match receiver.try_recv() {
                Ok(StreamEvent::Line(line)) => {
                    update_progress(ctx, &line);
                    if error_output.len() < MAX_ERROR_OUTPUT_BYTES
                        && line.to_ascii_lowercase().contains("error")
                    {
                        error_output.push_str(&line);
                        error_output.push('\n');
                    }
                }
                Ok(StreamEvent::Done) => finished_streams += 1,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|error| format!("读取解压进程状态失败：{error}"))?;
        }
        if status.is_some() && finished_streams >= 2 {
            break;
        }
        thread::sleep(Duration::from_millis(80));
    }

    if !status.is_some_and(|value| value.success()) {
        if looks_like_password_error(&error_output) {
            return Err("密码错误或归档需要密码".into());
        }
        return Err(if error_output.trim().is_empty() {
            "7-Zip 解压失败".into()
        } else {
            compact_error(&error_output)
        });
    }
    Ok(())
}

fn audit_tree(root: &Path) -> Result<(u64, u64), String> {
    let metadata =
        fs::symlink_metadata(root).map_err(|error| format!("无法检查解压临时目录：{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("解压结果包含符号链接，已拒绝写入".into());
    }
    if metadata.is_file() {
        return Ok((1, metadata.len()));
    }
    if !metadata.is_dir() {
        return Err("解压结果包含不支持的文件类型".into());
    }
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    for child in fs::read_dir(root).map_err(|error| format!("无法读取解压临时目录：{error}"))?
    {
        let child = child.map_err(|error| format!("无法读取解压结果：{error}"))?;
        let (child_files, child_bytes) = audit_tree(&child.path())?;
        files = files.saturating_add(child_files);
        bytes = bytes.saturating_add(child_bytes);
    }
    Ok((files, bytes))
}

fn remove_tree(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法检查旧解压结果：{error}")),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).map_err(|error| format!("无法删除旧解压结果：{error}"))?;
        return Ok(());
    }
    for child in fs::read_dir(path).map_err(|error| format!("无法读取旧解压结果：{error}"))?
    {
        remove_tree(
            &child
                .map_err(|error| format!("无法读取旧解压结果：{error}"))?
                .path(),
        )?;
    }
    fs::remove_dir(path).map_err(|error| format!("无法删除旧解压目录：{error}"))
}

fn unique_renamed_path(destination: &Path, name: &str) -> PathBuf {
    let path = PathBuf::from(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..100_000_u32 {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = destination.join(&candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    destination.join(format!("{name} ({})", now_nonce()))
}

fn now_nonce() -> String {
    let mut bytes = [0_u8; 8];
    if getrandom::getrandom(&mut bytes).is_err() {
        return "copy".into();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
fn commit_staging(
    staging: &Path,
    destination: &Path,
    policy: &str,
) -> Result<(u64, u64, u64), String> {
    commit_staging_with_context(staging, destination, policy, None)
}
fn commit_staging_with_context(
    staging: &Path,
    destination: &Path,
    policy: &str,
    context: Option<&TaskContext>,
) -> Result<(u64, u64, u64), String> {
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    let mut skipped = 0_u64;
    let roots = fs::read_dir(staging)
        .map_err(|error| format!("无法读取解压临时目录：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("无法读取解压结果：{error}"))?;
    for root in roots {
        let source = root.path();
        let name = root.file_name();
        let target = destination.join(&name);
        let final_target = if target.exists() {
            match policy {
                "skip" => {
                    let (root_files, _) = audit_tree(&source)?;
                    skipped = skipped.saturating_add(root_files);
                    remove_tree(&source)?;
                    continue;
                }
                "rename" => unique_renamed_path(destination, &name.to_string_lossy()),
                "overwrite" => target,
                _ => return Err("解压冲突策略仍为询问，请先完成冲突确认".into()),
            }
        } else {
            target
        };
        let (root_files, root_bytes) = audit_tree(&source)?;
        if let Some(ctx) = context {
            ctx.check_cancelled()?;
            let stage = destination.join(format!(".cruciblebox-extract-output-{}", now_nonce()));
            if stage.exists() {
                return Err("解压发布暂存路径冲突".into());
            }
            fs::rename(&source, &stage)
                .map_err(|error| format!("移动解压暂存结果失败：{error}"))?;
            let publication = ctx.runtime_context().publish_object(
                &stage,
                &final_target,
                policy == "overwrite",
                false,
                Some(destination),
            );
            if let Err(error) = publication {
                if !ctx.runtime_context().publication_pending() {
                    fs::rename(&stage, &source).map_err(|restore| {
                        format!(
                            "{error}; 恢复解压暂存失败：{restore}; 暂存位于 {}",
                            stage.display()
                        )
                    })?;
                }
                return Err(error);
            }
            files = files.saturating_add(root_files);
            bytes = bytes.saturating_add(root_bytes);
            continue;
        }
        let backup = if policy == "overwrite" && final_target.exists() {
            let backup = destination.join(format!(".cruciblebox-replaced-{}", now_nonce()));
            if backup.exists() {
                return Err("无法分配解压备份路径".into());
            }
            fs::rename(&final_target, &backup)
                .map_err(|error| format!("无法备份旧解压结果：{error}"))?;
            Some(backup)
        } else {
            None
        };
        if let Err(error) = fs::rename(&source, &final_target) {
            if let Some(backup) = backup {
                fs::rename(&backup, &final_target).map_err(|restore| {
                    format!(
                        "无法写入解压结果：{error}；恢复旧结果失败：{restore}；备份位于 {}",
                        backup.display()
                    )
                })?;
            }
            return Err(format!("无法写入解压结果：{error}"));
        }
        if let Some(backup) = backup {
            if let Err(error) = remove_tree(&backup) {
                eprintln!(
                    "[archive] retained replaced backup {}: {error}",
                    backup.display()
                );
            }
        }
        files = files.saturating_add(root_files);
        bytes = bytes.saturating_add(root_bytes);
    }
    remove_tree(staging)?;
    Ok((files, bytes, skipped))
}

fn execute_task(
    ctx: &TaskContext,
    source: PathBuf,
    destination: PathBuf,
    policy: String,
    password: Option<String>,
    open_after_extract: bool,
) -> Result<Value, String> {
    let info = inspect_archive(&source, password.as_deref()).map_err(|value| {
        value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("无法读取归档")
            .to_string()
    })?;
    if policy == "ask"
        && root_names(&info)
            .iter()
            .any(|name| destination.join(name).exists())
    {
        return Err("解压冲突策略仍为询问，请先完成冲突确认".into());
    }
    let staging = destination.join(format!(".cruciblebox-extracting-{}", now_nonce()));
    fs::create_dir_all(&staging).map_err(|error| format!("无法创建临时目录：{error}"))?;
    ctx.update_progress("preparing", 1, "正在准备解压目录…");
    let result = (|| {
        run_extraction(ctx, &source, &staging, password.as_deref())?;
        ctx.check_cancelled()?;
        audit_tree(&staging)?;
        ctx.update_progress("committing", 96, "正在写入解压结果…");
        {
            let (files, bytes, skipped) =
                commit_staging_with_context(&staging, &destination, &policy, Some(ctx))?;
            if open_after_extract {
                #[cfg(windows)]
                {
                    let _ = Command::new("explorer.exe").arg(&destination).spawn();
                }
            }
            ctx.update_progress("done", 100, "解压完成");
            Ok(json!({
                "source": source,
                "destination": destination,
                "format": info.format,
                "files": files,
                "bytes": bytes,
                "skipped": skipped,
            }))
        }
    })();
    if result.is_err() && !ctx.runtime_context().publication_pending() {
        let _ = remove_tree(&staging);
    }
    result
}

fn inspect_operation(payload: Option<&Value>) -> Result<Value, String> {
    let source = source_path(&payload_string(payload, "source")?)?;
    let password = payload
        .and_then(|value| value.get("password"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    match inspect_archive(&source, password) {
        Ok(info) => Ok(ok(public_info(&info))),
        Err(value) => Ok(value),
    }
}

fn preflight_operation(payload: Option<&Value>) -> Result<Value, String> {
    let source = source_path(&payload_string(payload, "source")?)?;
    let destination = destination_path(&payload_string(payload, "destination")?)?;
    let password = payload
        .and_then(|value| value.get("password"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let info = match inspect_archive(&source, password) {
        Ok(info) => info,
        Err(value) => return Ok(value),
    };
    Ok(preflight(&info, &destination))
}

fn start_operation(context: &Service, payload: Option<&Value>) -> Result<Value, String> {
    let source = source_path(&payload_string(payload, "source")?)?;
    let destination = destination_path(&payload_string(payload, "destination")?)?;
    let policy = validate_policy(payload.and_then(|value| value.get("conflictPolicy")))?;
    if policy == "ask" {
        return Err("start requires a resolved conflict policy".into());
    }
    let password = payload
        .and_then(|value| value.get("password"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let open_after_extract = payload_bool(payload, "openAfterExtract", true);
    let task_id = context.tasks.start(
        RESOURCE_KEY,
        Box::new(move |ctx| {
            execute_task(
                ctx,
                source,
                destination,
                policy,
                password,
                open_after_extract,
            )
        }),
    )?;
    Ok(ok(json!({ "taskId": task_id })))
}

fn create_operation(context: &Service, payload: Option<&Value>) -> Result<Value, String> {
    let source_values = payload
        .and_then(|value| value.get("sources"))
        .and_then(Value::as_array)
        .ok_or_else(|| "请选择需要压缩的文件".to_string())?;
    if source_values.is_empty() {
        return Err("请选择需要压缩的文件".into());
    }
    let sources = source_values
        .iter()
        .map(|value| {
            let raw = value.as_str().ok_or_else(|| "文件路径无效".to_string())?;
            let path = PathBuf::from(raw);
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if !path.is_absolute()
                || metadata.file_type().is_symlink()
                || !(metadata.is_file() || metadata.is_dir())
            {
                return Err("压缩来源必须是本地文件或目录".into());
            }
            fs::canonicalize(path).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let destination = PathBuf::from(payload_string(payload, "destination")?);
    let format = payload_string(payload, "format")?;
    if !matches!(format.as_str(), "zip" | "7z") {
        return Err("压缩格式仅支持 ZIP 和 7z".into());
    }
    if destination.extension().and_then(|value| value.to_str()) != Some(format.as_str()) {
        return Err("输出文件扩展名与压缩格式不一致".into());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "输出路径无效".to_string())?;
    if !parent.is_dir() {
        return Err("输出目录不存在".into());
    }
    let password = payload
        .and_then(|value| value.get("password"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let task_id = context.tasks.start(
        RESOURCE_KEY,
        Box::new(move |ctx| {
            let executable = seven_zip_path()?;
            let output = crate::output_transaction::OutputTransaction::new(&destination, false)?;
            // 7-Zip creates its own archive and rejects a reserved empty file.
            // The randomly named stage remains task-owned and is cleaned on failure.
            fs::remove_file(output.stage_path())
                .map_err(|error| format!("准备归档暂存文件失败：{error}"))?;
            ctx.update_progress("compressing", 5, "正在创建归档…");
            let mut command = Command::new(executable);
            command.arg("a").arg(format!("-t{format}")).arg("-y");
            if let Some(password) = password.as_deref() {
                command.arg(format!("-p{password}"));
            }
            command.arg(output.stage_path()).args(&sources);
            let mut child = command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("无法启动压缩进程：{error}"))?;
            loop {
                if ctx.is_cancelled() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("压缩已取消".into());
                }
                if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                    if !status.success() {
                        return Err("7-Zip 压缩失败".into());
                    }
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            ctx.check_cancelled()?;
            let committed = output.publish_durable(ctx.runtime_context(), true, |stage| {
                let metadata = fs::metadata(stage)
                    .map_err(|error| format!("读取归档结果失败：{error}"))?;
                if metadata.len() == 0 {
                    return Err("归档结果为空".into());
                }
                let listing = run_listing(stage, password.as_deref())?;
                if !listing.status.success() {
                    return Err("归档结果无法重新列出".into());
                }
                Ok(())
            })?;
            Ok(json!({ "destination": committed, "files": sources.len(), "bytes": fs::metadata(&committed).map(|value| value.len()).unwrap_or(0), "skipped": 0, "format": format }))
        }),
    )?;
    Ok(ok(json!({ "taskId": task_id })))
}

fn open_folder_operation(payload: Option<&Value>) -> Result<Value, String> {
    let folder = destination_path(&payload_string(payload, "path")?)?;
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg(&folder)
            .spawn()
            .map_err(|error| format!("无法打开目录：{error}"))?;
        Ok(ok(json!({ "opened": true })))
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Err("当前平台不支持打开资源管理器".into())
    }
}

fn compact_error(text: &str) -> String {
    let compact = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        "未知错误".into()
    } else {
        compact.chars().take(500).collect()
    }
}

/// Trusted service entry point used by `envelope_host`.
pub fn dispatch(
    context: &Service,
    _plugin_id: &str,
    operation: &str,
    payload: Option<&Value>,
) -> Result<Value, String> {
    if operation == "message" {
        let message_type = payload
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str)
            .ok_or_else(|| "archive message requires type".to_string())?;
        return dispatch(context, _plugin_id, message_type, payload);
    }
    match operation {
        "activate" => Ok(ok(json!({
            "service": "archive-extractor",
            "tool": seven_zip_path().is_ok(),
        }))),
        "deactivate" => {
            context.tasks.cancel_all_active();
            Ok(ok(json!({ "cancelled": true })))
        }
        "getToolInfo" => Ok(ok(json!({
            "available": seven_zip_path().is_ok(),
            "offline": true,
            "formats": ["zip", "7z", "rar", "tar", "gz", "bz2", "xz", "cab"],
        }))),
        "inspect" => inspect_operation(payload),
        "preflight" => preflight_operation(payload),
        "start" => start_operation(context, payload),
        "create" => create_operation(context, payload),
        "getTask" => {
            let task_id = payload_string(payload, "taskId")?;
            Ok(ok(json!({ "task": context.tasks.get(&task_id) })))
        }
        "cancel" => {
            let task_id = payload_string(payload, "taskId")?;
            Ok(ok(json!({ "cancelled": context.tasks.cancel(&task_id) })))
        }
        "openFolder" => open_folder_operation(payload),
        _ => Ok(failure(
            "unknown-operation",
            format!("未知解压操作：{operation}"),
        )),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn durable_extraction_keeps_bounded_directory_reference_after_many_roots() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        let destination = root.path().join("destination");
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&destination).unwrap();
        for index in 0..40 {
            fs::write(staging.join(format!("file-{index}.txt")), b"new").unwrap();
        }
        fs::create_dir_all(staging.join("folder")).unwrap();
        fs::create_dir_all(destination.join("folder")).unwrap();
        fs::write(staging.join("folder/new.txt"), b"new").unwrap();
        fs::write(destination.join("folder/old.txt"), b"old").unwrap();
        let journal = root.path().join("tasks.sqlite");
        let runtime = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        let manager = Arc::new(TaskManager::with_runtime(
            "archive-extractor",
            runtime.clone(),
        ));
        let output = destination.clone();
        let id = manager
            .start(
                RESOURCE_KEY,
                Box::new(move |ctx| {
                    let (files, bytes, skipped) =
                        commit_staging_with_context(&staging, &output, "overwrite", Some(ctx))?;
                    Ok(json!({"files":files,"bytes":bytes,"skipped":skipped}))
                }),
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if manager.get(&id).unwrap()["status"] == "succeeded" {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let snapshot = manager.get(&id).unwrap();
        assert_eq!(snapshot["status"], "succeeded", "{snapshot}");
        assert_eq!(snapshot["result"]["files"], 41);
        assert_eq!(snapshot["resultRefs"].as_array().unwrap().len(), 1);
        assert_eq!(
            fs::read(destination.join("folder/new.txt")).unwrap(),
            b"new"
        );
        assert!(!destination.join("folder/old.txt").exists());
        drop(manager);
        drop(runtime);
        let reopened = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        assert_eq!(
            reopened.get("archive-extractor", &id).unwrap()["resultRefs"][0],
            destination
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_ref()
        );
    }

    use super::*;

    #[test]
    fn member_paths_reject_escape_and_windows_devices() {
        assert!(normalize_member_path("../evil.txt").is_err());
        assert!(normalize_member_path("C:/evil.txt").is_err());
        assert!(normalize_member_path("folder/CON.txt").is_err());
        assert!(normalize_member_path("folder/name?.txt").is_err());
        assert_eq!(
            normalize_member_path("folder\\file.txt").unwrap(),
            "folder/file.txt"
        );
    }

    #[test]
    fn archive_suffixes_produce_readable_default_folder_names() {
        assert_eq!(strip_archive_suffix("project.tar.gz"), "project");
        assert_eq!(strip_archive_suffix("project.7z"), "project");
        assert_eq!(strip_archive_suffix("project.zip"), "project");
    }

    #[test]
    fn rename_keeps_extension_and_avoids_existing_path() {
        let root = std::env::temp_dir().join(format!("cruciblebox-archive-test-{}", now_nonce()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("data.txt"), b"x").unwrap();
        let renamed = unique_renamed_path(&root, "data.txt");
        assert_eq!(renamed.file_name().unwrap(), "data (1).txt");
        let _ = remove_tree(&root);
    }

    #[test]
    fn overwrite_commits_staged_tree_and_removes_backup() {
        let root = std::env::temp_dir().join(format!("cruciblebox-overwrite-{}", now_nonce()));
        let staging = root.join("staging");
        let destination = root.join("destination");
        fs::create_dir_all(staging.join("folder")).unwrap();
        fs::create_dir_all(destination.join("folder")).unwrap();
        fs::write(staging.join("folder").join("new.txt"), b"new").unwrap();
        fs::write(destination.join("folder").join("old.txt"), b"old").unwrap();
        let (files, bytes, skipped) = commit_staging(&staging, &destination, "overwrite").unwrap();
        assert_eq!((files, bytes, skipped), (1, 3, 0));
        assert_eq!(
            fs::read(destination.join("folder").join("new.txt")).unwrap(),
            b"new"
        );
        assert!(!destination.join("folder").join("old.txt").exists());
        assert!(fs::read_dir(&destination).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".cruciblebox-replaced-")));
        let _ = remove_tree(&root);
    }

    #[test]
    fn failed_extraction_task_remains_failed_after_runtime_restart() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("corrupt.zip");
        let destination = root.path().join("destination");
        fs::write(&source, b"not a zip archive").unwrap();
        fs::create_dir(&destination).unwrap();
        let journal = root.path().join("tasks.sqlite");
        let runtime = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        let service = Service::new(runtime.clone());
        let response = start_operation(
            &service,
            Some(&json!({
                "source": source.to_string_lossy(),
                "destination": destination.to_string_lossy(),
                "conflictPolicy": "skip"
            })),
        )
        .unwrap();
        let id = response["data"]["taskId"].as_str().unwrap().to_string();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let failed = loop {
            let snapshot = service.tasks.get(&id).unwrap();
            if snapshot["status"] == "failed" {
                break snapshot;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "archive task did not fail: {snapshot}"
            );
            thread::sleep(Duration::from_millis(5));
        };
        assert!(!failed["error"]["message"].as_str().unwrap().is_empty());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        drop(service);
        drop(runtime);

        let reopened = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        let recovered = reopened.get("archive-extractor", &id).unwrap();
        assert_eq!(recovered["status"], "failed");
        assert_eq!(recovered["error"], failed["error"]);
        assert_eq!(recovered["resultRefs"], json!([]));
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    }

    #[test]
    fn archive_round_trip_publishes_outputs_and_recovers_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let journal = root.path().join("tasks.sqlite");
        let runtime = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        let service = Service::new(runtime.clone());
        let source = root.path().join("源文件.txt");
        let contents = "归档产物必须可读取";
        fs::write(&source, contents).unwrap();
        let archive = root.path().join("round-trip.zip");

        let created = create_operation(
            &service,
            Some(&json!({
                "sources": [source.to_string_lossy()],
                "destination": archive.to_string_lossy(),
                "format": "zip"
            })),
        )
        .unwrap();
        let create_id = created["data"]["taskId"].as_str().unwrap().to_string();
        let create_deadline = std::time::Instant::now() + Duration::from_secs(30);
        let create_snapshot = loop {
            let snapshot = service.tasks.get(&create_id).unwrap();
            if matches!(snapshot["status"].as_str(), Some("succeeded" | "failed")) {
                break snapshot;
            }
            assert!(
                std::time::Instant::now() < create_deadline,
                "archive creation did not finish: {snapshot}"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(create_snapshot["status"], "succeeded", "{create_snapshot}");
        assert!(archive.is_file());
        assert_eq!(inspect_archive(&archive, None).unwrap().file_count, 1);

        let destination = root.path().join("extracted");
        let started = start_operation(
            &service,
            Some(&json!({
                "source": archive.to_string_lossy(),
                "destination": destination.to_string_lossy(),
                "conflictPolicy": "skip",
                "openAfterExtract": false
            })),
        )
        .unwrap();
        let extract_id = started["data"]["taskId"].as_str().unwrap().to_string();
        let extract_deadline = std::time::Instant::now() + Duration::from_secs(30);
        let extract_snapshot = loop {
            let snapshot = service.tasks.get(&extract_id).unwrap();
            if matches!(snapshot["status"].as_str(), Some("succeeded" | "failed")) {
                break snapshot;
            }
            assert!(
                std::time::Instant::now() < extract_deadline,
                "archive extraction did not finish: {snapshot}"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            extract_snapshot["status"], "succeeded",
            "{extract_snapshot}"
        );
        assert_eq!(extract_snapshot["result"]["files"], 1);
        assert_eq!(
            fs::read_to_string(destination.join("源文件.txt")).unwrap(),
            contents
        );

        drop(service);
        drop(runtime);
        let reopened = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        let recovered_create = reopened.get("archive-extractor", &create_id).unwrap();
        let recovered_extract = reopened.get("archive-extractor", &extract_id).unwrap();
        assert_eq!(recovered_create["status"], "succeeded");
        assert_eq!(recovered_extract["status"], "succeeded");
        let archive_reference = fs::canonicalize(&archive).unwrap();
        assert_eq!(
            recovered_create["resultRefs"],
            json!([archive_reference.to_string_lossy()])
        );
        let published_dir = destination.canonicalize().unwrap();
        assert_eq!(
            recovered_extract["resultRefs"],
            json!([published_dir.to_string_lossy()])
        );
        assert_eq!(
            fs::read_to_string(published_dir.join("源文件.txt")).unwrap(),
            contents
        );
    }

    #[test]
    fn creates_zip_from_files_and_reads_it_back() {
        let context = Service::new(crate::task_runtime::TaskRuntime::memory());
        let root = std::env::temp_dir().join(format!("cruciblebox-create-zip-{}", now_nonce()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("hello.txt");
        fs::write(&source, "归档测试").unwrap();
        let destination = root.join("created.zip");
        let response = create_operation(
            &context,
            Some(&json!({
                "sources": [source.to_string_lossy()],
                "destination": destination.to_string_lossy(),
                "format": "zip"
            })),
        )
        .unwrap();
        let task_id = response["data"]["taskId"].as_str().unwrap();
        let mut terminal = None;
        for _ in 0..100 {
            let snapshot = context.tasks.get(task_id).unwrap();
            if snapshot["status"] == "succeeded" || snapshot["status"] == "failed" {
                terminal = Some(snapshot);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(terminal.unwrap()["status"], "succeeded");
        let info = inspect_archive(&destination, None).unwrap();
        assert_eq!(info.file_count, 1);
        let original = fs::read(&destination).unwrap();
        let second = create_operation(
            &context,
            Some(&json!({
                "sources": [source.to_string_lossy()],
                "destination": destination.to_string_lossy(),
                "format": "zip"
            })),
        )
        .unwrap();
        let second_id = second["data"]["taskId"].as_str().unwrap();
        let mut second_terminal = None;
        for _ in 0..100 {
            let snapshot = context.tasks.get(second_id).unwrap();
            if snapshot["status"] == "succeeded" || snapshot["status"] == "failed" {
                second_terminal = Some(snapshot);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let second_terminal = second_terminal.unwrap();
        assert_eq!(second_terminal["status"], "succeeded");
        let second_path = PathBuf::from(second_terminal["result"]["destination"].as_str().unwrap());
        assert_eq!(
            second_terminal["resultRefs"],
            json!([second_path.to_string_lossy()])
        );
        assert_ne!(second_path, destination);
        assert_eq!(fs::read(&destination).unwrap(), original);
        assert_eq!(inspect_archive(&second_path, None).unwrap().file_count, 1);
        let _ = remove_tree(&root);
    }
}

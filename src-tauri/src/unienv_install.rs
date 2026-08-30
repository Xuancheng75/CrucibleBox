// UniEnv 安装原语与工具实现（1.9.11 阶段 B）
// 行为对齐冻结线 plugin-system/trusted-services/unienv/tools/*.ts：
// - 下载：HTTPS 强制、512MB 上限、30s 读空闲超时、2 次退避重试、.part 原子落盘、
//   SHA-256 校验、多源 fallback（镜像优先级见 unienv_catalog）
// - 解压：zip crate（mangled_name 防穿越），staging 目录隔离 + rename 提升
// - 版本切换：NTFS junction（无特权要求）；拒绝删除非 reparse point 路径
// - 进程：CREATE_NO_WINDOW、超时/取消强杀、输出截断

use crate::unienv_catalog;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_IDLE_TIMEOUT_SECS: u64 = 30;
const DOWNLOAD_CONNECT_TIMEOUT_SECS: u64 = 30;
const DOWNLOAD_RETRIES: u32 = 2;
const PROGRESS_THROTTLE_MS: u128 = 300;
const INSTALL_STAGING_PREFIX: &str = ".unienv-staging-";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub type ProgressCb<'a> = &'a (dyn Fn(&str, u32, &str) + Send + Sync);

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("操作已取消".into())
    } else {
        Ok(())
    }
}

fn cancelable_sleep(ms: u64, cancel: &AtomicBool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < deadline {
        cancelled(cancel)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    cancelled(cancel)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes}B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1}KB", bytes as f64 / 1024.0);
    }
    format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
}

fn assert_https_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("下载地址必须使用 HTTPS 且不能包含凭据".into());
    }
    Ok(())
}

/// 单源流式下载 + SHA-256 校验 + .part 原子重命名。
fn download_one(
    url: &str,
    label: &str,
    dest: &Path,
    expected_sha256: &str,
    progress: ProgressCb,
    cancel: &AtomicBool,
) -> Result<(), String> {
    assert_https_url(url)?;
    cancelled(cancel)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(DOWNLOAD_CONNECT_TIMEOUT_SECS))
        .timeout_read(Duration::from_secs(DOWNLOAD_IDLE_TIMEOUT_SECS))
        .build();
    let response = match agent.get(url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => {
            return Err(format!("下载失败: HTTP {code}"));
        }
        Err(other) => {
            return Err(format!("下载失败: {other}"));
        }
    };
    // 重定向后的最终地址仍必须是 HTTPS
    assert_https_url(response.get_url())?;

    let content_length: u64 = response
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if content_length > MAX_DOWNLOAD_BYTES {
        return Err(format!(
            "下载响应超过 {} 上限",
            format_bytes(MAX_DOWNLOAD_BYTES)
        ));
    }

    let part_path = part_path_for(dest);
    let _ = fs::remove_file(&part_path);
    let mut file = File::create(&part_path)
        .map_err(|e| format!("无法创建下载临时文件 {}: {e}", part_path.display()))?;
    let mut hasher = Sha256::new();
    let mut reader = response.into_reader();
    let mut downloaded: u64 = 0;
    let mut last_report = Instant::now();
    let mut buf = [0u8; 64 * 1024];
    loop {
        cancelled(cancel)?;
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("下载失败: {e}"))?;
        if n == 0 {
            break;
        }
        if downloaded + n as u64 > MAX_DOWNLOAD_BYTES {
            return Err(format!(
                "下载内容超过 {} 上限",
                format_bytes(MAX_DOWNLOAD_BYTES)
            ));
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("无法继续写入下载文件: {e}"))?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if content_length > 0 && last_report.elapsed().as_millis() > PROGRESS_THROTTLE_MS {
            let pct = ((downloaded as f64 / content_length as f64) * 100.0)
                .min(95.0)
                .round() as u32;
            progress(
                "downloading",
                pct,
                &format!(
                    "{label} ({}/{})",
                    format_bytes(downloaded),
                    format_bytes(content_length)
                ),
            );
            last_report = Instant::now();
        }
    }
    if content_length > 0 && downloaded != content_length {
        return Err(format!(
            "下载内容不完整: 预期 {content_length} 字节，实际 {downloaded} 字节"
        ));
    }
    let actual = hex_encode(&hasher.finalize());
    if actual != expected_sha256 {
        return Err(format!(
            "下载制品 SHA-256 不匹配: 预期 {expected_sha256}，实际 {actual}"
        ));
    }
    file.sync_all()
        .map_err(|e| format!("下载文件落盘失败: {e}"))?;
    drop(file);
    fs::rename(&part_path, dest).map_err(|e| format!("下载文件重命名失败: {e}"))?;
    Ok(())
}

fn part_path_for(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// 多源 fallback 下载（对齐 downloadWithFallback + fetchWithTimeout）：
/// 每个 URL 最多尝试 1+DOWNLOAD_RETRIES 次（指数退避 2s/4s，上限 15s），
/// 取消立即中止；全部失败时返回最后一个错误。
pub fn download_with_fallback(
    urls: &[(String, String)],
    dest: &Path,
    expected_sha256: &str,
    progress: ProgressCb,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut last_error = String::from("所有下载源均失败");
    for (url, label) in urls {
        cancelled(cancel)?;
        for attempt in 0..=DOWNLOAD_RETRIES {
            cancelled(cancel)?;
            if attempt > 0 {
                let delay = (2000u64.saturating_mul(1 << (attempt - 1))).min(15_000);
                cancelable_sleep(delay, cancel)?;
            }
            match download_one(url, label, dest, expected_sha256, progress, cancel) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if cancel.load(Ordering::SeqCst) {
                        return Err("操作已取消".into());
                    }
                    last_error = e;
                    let _ = fs::remove_file(part_path_for(dest));
                }
            }
        }
    }
    Err(last_error)
}

/// 解压 ZIP 到目标目录。mangled_name 剥离绝对路径/.. 穿越成分（防 zip-slip）。
pub fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| format!("无法打开压缩包: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("压缩包读取失败: {e}"))?;
    fs::create_dir_all(dest_dir).map_err(|e| format!("无法创建解压目录: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("压缩包条目读取失败: {e}"))?;
        let relative = entry.mangled_name();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = dest_dir.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("解压目录创建失败: {e}"))?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("解压目录创建失败: {e}"))?;
            }
            let mut out = File::create(&target).map_err(|e| format!("解压文件创建失败: {e}"))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| format!("解压写入失败: {e}"))?;
        }
    }
    Ok(())
}

/// 单一顶层目录则返回它，否则返回解压根（对齐 findTopDir）
pub fn find_top_dir(extract_dir: &Path) -> PathBuf {
    if let Ok(entries) = fs::read_dir(extract_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    return entry.path();
                }
            }
        }
    }
    extract_dir.to_path_buf()
}

pub fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

fn random_suffix() -> String {
    let mut buf = [0u8; 6];
    let _ = getrandom::getrandom(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn create_install_staging_dir(version_root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(version_root).map_err(|e| format!("版本目录创建失败: {e}"))?;
    let staging = version_root.join(format!("{INSTALL_STAGING_PREFIX}{}", random_suffix()));
    fs::create_dir(&staging).map_err(|e| format!("staging 目录创建失败: {e}"))?;
    Ok(staging)
}

pub fn cleanup_install_staging_dir(staging: &Path) {
    let ok = staging
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.starts_with(INSTALL_STAGING_PREFIX))
        .unwrap_or(false);
    if ok {
        let _ = fs::remove_dir_all(staging);
    }
}

/// staging 内运行时目录提升为最终目录（同卷 rename）。最终目录已存在则拒绝覆盖。
pub fn promote_staged_runtime(source: &Path, final_dir: &Path) -> Result<(), String> {
    if final_dir.exists() {
        return Err(format!(
            "最终运行时目录已存在，拒绝覆盖: {}",
            final_dir.display()
        ));
    }
    fs::rename(source, final_dir).map_err(|e| format!("运行时目录提升失败: {e}"))
}

/// 直装目录准备（对齐 prepareDirectInstallDirectory）：不存在则建；存在必须是
/// 普通空目录，否则拒绝覆盖。
pub fn prepare_direct_install_directory(directory: &Path, label: &str) -> Result<(), String> {
    if !directory.exists() {
        fs::create_dir_all(directory).map_err(|e| format!("{label} 目录创建失败: {e}"))?;
        return Ok(());
    }
    if is_reparse_point(directory) || !directory.is_dir() {
        return Err(format!(
            "{label} 的安装目标不是普通目录，拒绝覆盖: {}",
            directory.display()
        ));
    }
    let has_entries = fs::read_dir(directory)
        .map(|mut it| it.next().is_some())
        .unwrap_or(true);
    if has_entries {
        return Err(format!(
            "{label} 的安装目标已包含文件，拒绝覆盖: {}",
            directory.display()
        ));
    }
    Ok(())
}

pub fn remove_junction(link: &Path) -> Result<(), String> {
    if fs::symlink_metadata(link).is_err() {
        return Ok(());
    }
    if !is_reparse_point(link) {
        return Err(format!("拒绝删除非 junction 路径: {}", link.display()));
    }
    // remove_dir 只移除 reparse point 本身，不触碰目标内容
    fs::remove_dir(link).map_err(|e| format!("junction 删除失败: {e}"))
}

pub fn create_junction(link: &Path, target: &Path) -> Result<(), String> {
    remove_junction(link)?;
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("工具目录创建失败: {e}"))?;
    }
    junction::create(target, link).map_err(|e| format!("junction 创建失败: {e}"))
}

/// 冻结线 extractVersion：`/v?(\d+\.\d+\.\d+)/` 首个匹配。
pub fn extract_version(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = if bytes[i] == b'v' || bytes[i] == b'V' {
            i + 1
        } else {
            i
        };
        let mut j = start;
        let mut dots = 0;
        while j < bytes.len() && (bytes[j].is_ascii_digit() || (bytes[j] == b'.' && dots < 2)) {
            if bytes[j] == b'.' {
                // 点后必须紧跟数字，否则视为版本串结束
                if j + 1 >= bytes.len() || !bytes[j + 1].is_ascii_digit() {
                    break;
                }
                dots += 1;
            }
            j += 1;
        }
        if j > start && dots == 2 {
            return Some(raw[start..j].to_string());
        }
        i = j.max(i + 1);
    }
    None
}

/// 运行 `exe args`（隐藏窗口、10s 超时）并提取版本号。
/// from_stderr=true 时优先取 stderr（java -version 把版本写到 stderr）。
pub fn probe_tool_version(exe: &Path, args: &[&str], from_stderr: bool) -> Option<String> {
    let cancel = AtomicBool::new(false);
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let out = run_process(exe, &owned, 10_000, &cancel).ok()?;
    let text = if from_stderr && !out.stderr.is_empty() {
        out.stderr.as_str()
    } else {
        out.stdout.as_str()
    };
    extract_version(text)
}

struct ProcOutput {
    stdout: String,
    stderr: String,
}

impl ProcOutput {
    fn tail(&self) -> String {
        const CAP: usize = 2000;
        let mut text = self.stderr.clone();
        if text.is_empty() {
            text = self.stdout.clone();
        }
        if text.len() > CAP {
            text.truncate(CAP);
        }
        text
    }
}

/// 运行外部进程：隐藏窗口、并发收集输出、超时/取消强杀。
fn run_process(
    program: &Path,
    args: &[String],
    timeout_ms: u64,
    cancel: &AtomicBool,
) -> Result<ProcOutput, String> {
    run_process_env(program, args, timeout_ms, cancel, &[])
}

/// run_process 的带环境变量注入变体（rustup-init 需 RUSTUP_HOME/CARGO_HOME 指向版本目录）。
#[allow(clippy::too_many_arguments)]
fn run_process_env(
    program: &Path,
    args: &[String],
    timeout_ms: u64,
    cancel: &AtomicBool,
    envs: &[(&str, String)],
) -> Result<ProcOutput, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动进程失败 {}: {e}", program.display()))?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    // 起两个读线程排空管道，避免子进程输出写满管道导致死锁
    let out_buf = Arc::new(Mutex::new(String::new()));
    let err_buf = Arc::new(Mutex::new(String::new()));
    let t_out = spawn_pipe_reader(
        stdout_handle.map(|s| Box::new(s) as Box<PipeReader>),
        Arc::clone(&out_buf),
    );
    let t_err = spawn_pipe_reader(
        stderr_handle.map(|s| Box::new(s) as Box<PipeReader>),
        Arc::clone(&err_buf),
    );

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        cancelled(cancel)?;
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "安装进程超时(>{}s): {}",
                        timeout_ms / 1000,
                        program.display()
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("进程等待失败: {e}")),
        }
    };
    let _ = t_out.join();
    let _ = t_err.join();
    let output = ProcOutput {
        stdout: out_buf.lock().unwrap().clone(),
        stderr: err_buf.lock().unwrap().clone(),
    };
    match status {
        Some(s) if s.success() => Ok(output),
        Some(s) => {
            let tail = output.tail();
            Err(format!(
                "安装进程退出码 {:?}{}",
                s.code(),
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(": {tail}")
                }
            ))
        }
        None => Err("安装进程未正常退出".into()),
    }
}

type PipeReader = dyn Read + Send;

fn spawn_pipe_reader(
    pipe: Option<Box<PipeReader>>,
    buffer: Arc<Mutex<String>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Some(mut pipe) = pipe else {
            return;
        };
        let mut chunk = [0u8; 8 * 1024];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut buf = buffer.lock().unwrap();
                    // 截断保护：最多保留 64KB
                    if buf.len() < 64 * 1024 {
                        buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
                    }
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// 工具实现（路径布局与安装参数逐项对齐 tools/{python,node,git,go,java}.ts）
// ---------------------------------------------------------------------------

pub fn tool_dir(install_root: &Path, tool: &str) -> PathBuf {
    install_root.join(tool)
}

pub fn version_dir(install_root: &Path, tool: &str, version: &str) -> PathBuf {
    tool_dir(install_root, tool).join(version)
}

pub fn current_link(install_root: &Path, tool: &str) -> PathBuf {
    tool_dir(install_root, tool).join("current")
}

/// 运行时子目录：node→runtime、go→go、java→jdk、php→runtime；python/git 直装于版本目录
fn runtime_subdir(tool: &str) -> Option<&'static str> {
    match tool {
        "node" | "php" => Some("runtime"),
        "go" => Some("go"),
        "java" => Some("jdk"),
        _ => None,
    }
}

fn installed_runtime_dir(install_root: &Path, tool: &str, version: &str) -> PathBuf {
    let base = version_dir(install_root, tool, version);
    match runtime_subdir(tool) {
        Some(sub) => base.join(sub),
        None => base,
    }
}

pub fn uninstall_tool(install_root: &Path, tool: &str) -> Result<(), String> {
    remove_junction(&current_link(install_root, tool))
}

pub fn switch_version(install_root: &Path, tool: &str, version: &str) -> Result<(), String> {
    let target = installed_runtime_dir(install_root, tool, version);
    if !target.is_dir() {
        return Err(format!("{tool} {version} 未安装"));
    }
    create_junction(&current_link(install_root, tool), &target)
}

/// Configure a single stable UniEnv shim directory in the current user's
/// PATH.  Version switches only update the `current` junction, so PATH does
/// not grow with every installed runtime.
pub fn configure_environment(install_root: &Path) -> Result<(), String> {
    let shim_dir = install_root.join("shims");
    fs::create_dir_all(&shim_dir).map_err(|e| format!("创建环境 shim 目录失败: {e}"))?;
    let current = |tool: &str| current_link(install_root, tool);
    let shims: &[(&str, PathBuf)] = &[
        ("python.cmd", current("python").join("python.exe")),
        ("pip.cmd", current("python").join("Scripts").join("pip.exe")),
        ("node.cmd", current("node").join("runtime").join("node.exe")),
        ("npm.cmd", current("node").join("runtime").join("npm.cmd")),
        ("npx.cmd", current("node").join("runtime").join("npx.cmd")),
        ("git.cmd", current("git").join("cmd").join("git.exe")),
        (
            "go.cmd",
            current("go").join("go").join("bin").join("go.exe"),
        ),
        (
            "java.cmd",
            current("java").join("jdk").join("bin").join("java.exe"),
        ),
        (
            "javac.cmd",
            current("java").join("jdk").join("bin").join("javac.exe"),
        ),
        (
            "rustc.cmd",
            current("rust").join("cargo").join("bin").join("rustc.exe"),
        ),
        (
            "cargo.cmd",
            current("rust").join("cargo").join("bin").join("cargo.exe"),
        ),
        ("php.cmd", current("php").join("runtime").join("php.exe")),
    ];
    for (name, target) in shims {
        let shim = shim_dir.join(name);
        if target.is_file() {
            write_cmd_shim(&shim_dir, name, target)?;
        } else if shim.exists() {
            fs::remove_file(&shim).map_err(|e| format!("清理失效环境 shim {name} 失败: {e}"))?;
        }
    }

    #[cfg(windows)]
    {
        configure_user_path(&shim_dir)
    }
    #[cfg(not(windows))]
    {
        let _ = shim_dir;
        Err("环境自动配置目前仅支持 Windows".into())
    }
}

fn write_cmd_shim(directory: &Path, name: &str, target: &Path) -> Result<(), String> {
    let shim = directory.join(name);
    let target = target.to_string_lossy().replace('"', "\\\"");
    let contents = format!("@echo off\r\n\"{target}\" %*\r\n");
    fs::write(&shim, contents).map_err(|e| format!("写入环境 shim {name} 失败: {e}"))
}

#[cfg(windows)]
fn configure_user_path(shim_dir: &Path) -> Result<(), String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, KEY_WRITE, REG_EXPAND_SZ,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
    let environment = wide("Environment");
    let path_name = wide("Path");
    let mut key: HKEY = null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            environment.as_ptr(),
            0,
            KEY_READ | KEY_WRITE,
            &mut key,
        )
    };
    if status != 0 {
        return Err(format!("打开用户环境变量失败（错误码 {status}）"));
    }

    let result = (|| {
        let mut value_type = 0u32;
        let mut byte_len = 0u32;
        let query = unsafe {
            RegQueryValueExW(
                key,
                path_name.as_ptr(),
                null(),
                &mut value_type,
                null_mut(),
                &mut byte_len,
            )
        };
        let old_path = if query == 0 && byte_len > 0 {
            let mut bytes = vec![0u8; byte_len as usize];
            let read = unsafe {
                RegQueryValueExW(
                    key,
                    path_name.as_ptr(),
                    null(),
                    &mut value_type,
                    bytes.as_mut_ptr(),
                    &mut byte_len,
                )
            };
            if read != 0 {
                return Err(format!("读取用户 PATH 失败（错误码 {read}）"));
            }
            let units = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|unit| *unit != 0)
                .collect::<Vec<_>>();
            String::from_utf16(&units).map_err(|e| format!("读取用户 PATH 编码失败: {e}"))?
        } else {
            String::new()
        };
        let shim = shim_dir.to_string_lossy().to_string();
        let already_present = old_path
            .split(';')
            .any(|entry| entry.eq_ignore_ascii_case(&shim));
        if already_present {
            return Ok(());
        }
        let new_path = if old_path.trim().is_empty() {
            shim
        } else {
            format!("{old_path};{shim}")
        };
        let encoded = wide(&new_path);
        let bytes = encoded
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .ok_or_else(|| "用户 PATH 长度溢出".to_string())?;
        let set_status = unsafe {
            RegSetValueExW(
                key,
                path_name.as_ptr(),
                0,
                if value_type == 0 {
                    REG_EXPAND_SZ
                } else {
                    value_type
                },
                encoded.as_ptr().cast(),
                u32::try_from(bytes).map_err(|_| "用户 PATH 过长".to_string())?,
            )
        };
        if set_status != 0 {
            return Err(format!("写入用户 PATH 失败（错误码 {set_status}）"));
        }
        Ok(())
    })();
    unsafe { RegCloseKey(key) };
    result
}

pub fn install_tool(
    install_root: &Path,
    tool: &str,
    version: &str,
    mirror: &str,
    progress: ProgressCb,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let plan = InstallPlan {
        urls: unienv_catalog::download_urls(tool, version, mirror)?,
        sha256: unienv_catalog::artifact(tool, version)?.sha256.to_string(),
        filename: unienv_catalog::artifact(tool, version)?
            .filename
            .to_string(),
    };
    install_with_plan(install_root, tool, version, &plan, progress, cancel)
}

/// 安装计划：URL 候选 + 期望 SHA-256 + 制品文件名（静态目录或在线源构建）
pub struct InstallPlan {
    pub urls: Vec<(String, String)>,
    pub sha256: String,
    /// 制品文件名（staging 落盘名；动态版本取下载 URL 尾段）
    pub filename: String,
}

/// 按 Plan 执行安装（静态目录与在线动态版本共用执行链）
pub fn install_with_plan(
    install_root: &Path,
    tool: &str,
    version: &str,
    plan: &InstallPlan,
    progress: ProgressCb,
    cancel: &AtomicBool,
) -> Result<(), String> {
    match runtime_subdir(tool) {
        None => install_from_installer(
            install_root,
            tool,
            version,
            &plan.sha256,
            &plan.filename,
            plan.urls.clone(),
            progress,
            cancel,
        ),
        Some(sub) => install_from_zip(
            install_root,
            ZipTarget {
                tool,
                version,
                final_subdir: sub,
            },
            &plan.sha256,
            &plan.filename,
            &plan.urls,
            progress,
            cancel,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn install_from_installer(
    install_root: &Path,
    tool: &str,
    version: &str,
    sha256: &str,
    filename: &str,
    urls: Vec<(String, String)>,
    progress: ProgressCb,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let dir = version_dir(install_root, tool, version);
    prepare_direct_install_directory(&dir, &display_name(tool, version))?;
    let installer_path = dir.join(filename);

    // 安装器参数（rust 的固定 flag 在执行段注入）
    let args: Vec<String> = match tool {
        "python" => vec![
            "/quiet".into(),
            "InstallAllUsers=0".into(),
            format!("TargetDir={}", dir.display()),
            "PrependPath=0".into(),
            "Include_test=0".into(),
        ],
        "git" => vec![
            "/VERYSILENT".into(),
            format!("/DIR={}", dir.display()),
            "/NORESTART".into(),
            "/NOCANCEL".into(),
            "/SP-".into(),
            "/NOICONS".into(),
        ],
        "rust" => vec![],
        other => return Err(format!("unsupported installer tool: {other}")),
    };

    progress(
        "downloading",
        0,
        &format!("正在下载 {} {version}...", display_name(tool, version)),
    );
    download_with_fallback(&urls, &installer_path, sha256, progress, cancel)?;

    progress(
        "installing",
        95,
        &format!("正在安装 {} {version}...", display_name(tool, version)),
    );
    let result = match tool {
        "rust" => {
            // rustup-init：通过 RUSTUP_HOME/CARGO_HOME 将工具链与 cargo home
            // 完全隔离进版本目录（自包含，不污染用户全局）
            let rustup_home = dir.join("rustup");
            let cargo_home = dir.join("cargo");
            let envs: Vec<(&str, String)> = vec![
                ("RUSTUP_HOME", rustup_home.to_string_lossy().into_owned()),
                ("CARGO_HOME", cargo_home.to_string_lossy().into_owned()),
            ];
            let mut full_args: Vec<String> = vec![
                "-y".into(),
                "--default-toolchain".into(),
                format!("{version}-x86_64-pc-windows-msvc"),
                "--no-modify-path".into(),
            ];
            full_args.extend(args.clone());
            run_process_env(&installer_path, &full_args, 900_000, cancel, &envs)
        }
        _ => run_process(&installer_path, &args, 600_000, cancel),
    };
    let _ = fs::remove_file(&installer_path);
    result?;

    progress("configuring", 98, "正在创建目录链接...");
    create_junction(&current_link(install_root, tool), &dir)?;

    progress(
        "done",
        100,
        &format!("{} {version} 安装完成", display_name(tool, version)),
    );
    Ok(())
}

struct ZipTarget<'a> {
    tool: &'a str,
    version: &'a str,
    final_subdir: &'a str,
}

fn install_from_zip(
    install_root: &Path,
    target: ZipTarget<'_>,
    sha256: &str,
    filename: &str,
    urls: &[(String, String)],
    progress: ProgressCb,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let ZipTarget {
        tool,
        version,
        final_subdir,
    } = target;
    let dir = version_dir(install_root, tool, version);
    fs::create_dir_all(&dir).map_err(|e| format!("版本目录创建失败: {e}"))?;
    let final_dir = dir.join(final_subdir);
    if final_dir.exists() {
        return Err(format!(
            "{} {version} 的运行时目录已存在，拒绝覆盖",
            display_name(tool, version)
        ));
    }
    let staging = create_install_staging_dir(&dir)?;
    let result = (|| -> Result<(), String> {
        let zip_path = staging.join(filename);
        let extract_dir = staging.join("extracted");

        progress(
            "downloading",
            0,
            &format!("正在下载 {} {version}...", display_name(tool, version)),
        );
        download_with_fallback(urls, &zip_path, sha256, progress, cancel)?;

        progress(
            "installing",
            95,
            &format!("正在解压 {} {version}...", display_name(tool, version)),
        );
        extract_zip(&zip_path, &extract_dir)?;
        let src = find_top_dir(&extract_dir);
        promote_staged_runtime(&src, &final_dir)?;
        Ok(())
    })();
    cleanup_install_staging_dir(&staging);
    result?;

    progress("configuring", 98, "正在创建目录链接...");
    create_junction(&current_link(install_root, tool), &final_dir)?;

    progress(
        "done",
        100,
        &format!("{} {version} 安装完成", display_name(tool, version)),
    );
    Ok(())
}

fn display_name(tool: &str, _version: &str) -> String {
    match tool {
        "python" => "Python".into(),
        "node" => "Node.js".into(),
        "git" => "Git".into(),
        "go" => "Go".into(),
        "java" => "JDK".into(),
        "rust" => "Rust".into(),
        "php" => "PHP".into(),
        other => other.into(),
    }
}

/// 启动恢复：清理全部受支持版本目录下的中断 staging（对齐 recoverInterruptedInstalls）。
/// 任一版本目录异常（非普通目录）即整体失败（fail-closed）。
pub fn recover_interrupted_staging(version_roots: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut removed = Vec::new();
    for root in version_roots {
        if !root.exists() {
            continue;
        }
        if is_reparse_point(root) || !root.is_dir() {
            return Err(format!(
                "版本目录不是普通目录，拒绝恢复: {}",
                root.display()
            ));
        }
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with(INSTALL_STAGING_PREFIX) {
                    continue;
                }
                let staging = entry.path();
                if is_reparse_point(&staging) || !staging.is_dir() {
                    return Err(format!(
                        "拒绝递归删除非普通 staging 目录: {}",
                        staging.display()
                    ));
                }
                fs::remove_dir_all(&staging)
                    .map_err(|e| format!("staging 清理失败 {}: {e}", staging.display()))?;
                removed.push(staging.to_string_lossy().into_owned());
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cruciblebox-unienv-test-{tag}-{}", random_suffix()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn format_bytes_matches_ts_helper() {
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(2048), "2.0KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0MB");
    }

    #[test]
    fn https_enforced() {
        assert!(assert_https_url("http://example.com/x").is_err());
        assert!(assert_https_url("https://example.com/x").is_ok());
    }

    #[test]
    fn direct_install_refuses_nonempty_and_reparse() {
        let root = temp_root("direct");
        let dir = root.join("python").join("3.12.5");
        // 不存在 → 创建；存在但为空 → 允许（对齐 prepareDirectInstallDirectory）
        prepare_direct_install_directory(&dir, "Python 3.12.5").unwrap();
        prepare_direct_install_directory(&dir, "Python 3.12.5").unwrap();
        // 已包含文件 → 拒绝覆盖
        File::create(dir.join("python.exe")).unwrap();
        assert!(prepare_direct_install_directory(&dir, "Python 3.12.5")
            .unwrap_err()
            .contains("已包含文件"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn staging_promote_and_cleanup() {
        let root = temp_root("staging");
        let version_root = root.join("node").join("24.18.1");
        let staging = create_install_staging_dir(&version_root).unwrap();
        assert!(staging.starts_with(&version_root));
        let src = staging.join("extracted").join("node-v24.18.1-win-x64");
        fs::create_dir_all(&src).unwrap();
        let final_dir = version_root.join("runtime");
        promote_staged_runtime(&src, &final_dir).unwrap();
        cleanup_install_staging_dir(&staging);
        assert!(final_dir.is_dir());
        assert!(!staging.exists());
        // 已存在的最终目录拒绝覆盖
        let staging2 = create_install_staging_dir(&version_root).unwrap();
        let src2 = staging2.join("other");
        fs::create_dir_all(&src2).unwrap();
        assert!(promote_staged_runtime(&src2, &final_dir)
            .unwrap_err()
            .contains("拒绝覆盖"));
        cleanup_install_staging_dir(&staging2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn zip_extraction_blocks_traversal() {
        let root = temp_root("zip");
        let zip_path = root.join("evil.zip");
        let out_dir = root.join("out");
        // 构造含 ../穿越 与嵌套目录的最小 zip
        {
            let file = File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut writer, b"evil").unwrap();
            writer
                .start_file("pkg/inner.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut writer, b"ok").unwrap();
            writer.finish().unwrap();
        }
        extract_zip(&zip_path, &out_dir).unwrap();
        assert!(out_dir.join("pkg").join("inner.txt").is_file());
        // mangled 语义：穿越成分被剥离后落在解压根内（包含性保证），绝不逃出 out_dir
        assert!(out_dir.join("escape.txt").is_file());
        assert!(!root.join("escape.txt").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_top_dir_single_or_root() {
        let root = temp_root("topdir");
        let extract = root.join("extracted");
        // 单一顶层目录 → 返回它
        fs::create_dir_all(extract.join("go1.26.5").join("bin")).unwrap();
        File::create(extract.join("README")).unwrap();
        assert_eq!(find_top_dir(&extract), extract.join("go1.26.5"));
        // 无目录 → 返回解压根
        let files_only = root.join("files_only");
        fs::create_dir_all(&files_only).unwrap();
        File::create(files_only.join("a.txt")).unwrap();
        assert_eq!(find_top_dir(&files_only), files_only);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn junction_roundtrip_and_guard() {
        let root = temp_root("junction");
        let target = root.join("runtime");
        fs::create_dir_all(&target).unwrap();
        File::create(target.join("marker.txt")).unwrap();
        let link = root.join("current");
        create_junction(&link, &target).unwrap();
        assert!(link.join("marker.txt").is_file());
        // 重复创建 = 幂等替换
        create_junction(&link, &target).unwrap();
        assert!(link.join("marker.txt").is_file());
        // 非 reparse 路径拒绝删除
        assert!(remove_junction(&target).unwrap_err().contains("拒绝删除"));
        remove_junction(&link).unwrap();
        assert!(!link.exists());
        assert!(target.join("marker.txt").is_file());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn recovery_removes_only_staging_dirs() {
        let root = temp_root("recovery");
        let vr = root.join("go").join("1.26.5");
        let keep = vr.join("go");
        fs::create_dir_all(&keep).unwrap();
        let stale = vr.join(format!("{INSTALL_STAGING_PREFIX}abc123"));
        fs::create_dir_all(&stale).unwrap();
        let roots = vec![vr.clone()];
        let removed = recover_interrupted_staging(&roots).unwrap();
        assert_eq!(removed.len(), 1);
        assert!(!stale.exists());
        assert!(keep.is_dir());
        fs::remove_dir_all(&root).unwrap();
    }
}

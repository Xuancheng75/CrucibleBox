//! OCR Worker host manager.
//!
//! The worker is a long-lived Rust process speaking newline-delimited JSON on
//! stdin/stdout.  The manager serializes requests, keeps the process warm
//! between jobs, and tears it down on cancellation, timeout, protocol errors,
//! or an unexpected crash.

use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const PROTOCOL_VERSION: u32 = 1;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const MAX_RESPONSE_LINE_BYTES: usize = 8 * 1024 * 1024;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrWorkerRequest {
    pub protocol_version: u32,
    pub request_id: String,
    pub task: &'static str,
    pub input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OcrWorkerOptions>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrWorkerOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dictionary_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_profile: Option<String>,
}

impl OcrWorkerRequest {
    pub fn new(
        request_id: String,
        input: String,
        language: Option<String>,
        device: Option<String>,
        model_directory: Option<String>,
        dictionary_path: Option<String>,
        model_profile: Option<String>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            task: "ocr",
            input,
            options: Some(OcrWorkerOptions {
                language,
                device,
                model_directory,
                dictionary_path,
                model_profile,
            }),
        }
    }
}

#[derive(Default)]
struct WorkerState {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    responses: Option<Receiver<String>>,
}

/// A single warm OCR worker.  `request_gate` is deliberately separate from
/// `state`: the request loop does not hold the process mutex while waiting for
/// a frame, so cancellation can terminate the child immediately.
pub struct OcrWorkerManager {
    worker_exe: PathBuf,
    timeout: Duration,
    state: Mutex<WorkerState>,
    request_gate: Mutex<()>,
}

impl OcrWorkerManager {
    pub fn new(worker_exe: PathBuf, timeout: Duration) -> Self {
        Self {
            worker_exe,
            timeout,
            state: Mutex::new(WorkerState::default()),
            request_gate: Mutex::new(()),
        }
    }

    pub fn discover(timeout: Duration) -> Self {
        let explicit = std::env::var_os("OCR_WORKER_EXE").map(PathBuf::from);
        let worker_exe = explicit.unwrap_or_else(|| {
            Self::resolve_worker_exe(
                std::env::current_exe()
                    .ok()
                    .and_then(|path| path.parent().map(PathBuf::from)),
                std::env::current_dir().ok(),
            )
        });
        if !worker_exe.is_file() {
            eprintln!(
                "[ocr-worker] executable not found; resolved={}",
                worker_exe.display()
            );
        }
        Self::new(worker_exe, timeout)
    }

    /// Resolve both development and packaged Tauri layouts.  The resolver is
    /// intentionally pure so it can be tested without launching a process.
    pub fn resolve_worker_exe(exe_dir: Option<PathBuf>, cwd: Option<PathBuf>) -> PathBuf {
        const WORKER_TRIPLE: &str = "ocr-worker-x86_64-pc-windows-msvc.exe";
        const WORKER_PLAIN: &str = "ocr-worker.exe";
        let mut candidates = Vec::new();
        if let Some(dir) = &cwd {
            candidates.push(dir.join("target/debug").join(WORKER_PLAIN));
            candidates.push(dir.join("ocr-worker/target/debug").join(WORKER_PLAIN));
            candidates.push(dir.join("target/release").join(WORKER_PLAIN));
            candidates.push(dir.join("ocr-worker/target/release").join(WORKER_PLAIN));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("target/debug").join(WORKER_PLAIN));
                candidates.push(parent.join("target/release").join(WORKER_PLAIN));
                candidates.push(parent.join("ocr-worker/target/debug").join(WORKER_PLAIN));
                candidates.push(parent.join("ocr-worker/target/release").join(WORKER_PLAIN));
            }
        }
        if let Some(dir) = &exe_dir {
            candidates.push(dir.join(WORKER_PLAIN));
            candidates.push(dir.join("resources").join(WORKER_PLAIN));
            candidates.push(dir.join(WORKER_TRIPLE));
            candidates.push(dir.join("resources").join(WORKER_TRIPLE));
        }
        candidates
            .into_iter()
            .find(|candidate| candidate.is_file())
            .unwrap_or_default()
    }

    pub fn status(&self) -> Value {
        let running = self.is_running();
        json!({
            "available": self.worker_exe.is_file(),
            "running": running,
            "path": self.worker_exe.to_string_lossy(),
            "timeoutSeconds": self.timeout.as_secs(),
            "protocolVersion": PROTOCOL_VERSION,
        })
    }

    /// Execute one request and return its result frame. Progress frames are
    /// forwarded to `on_progress`; result/error frames are consumed here.
    pub fn run(
        &self,
        request: &OcrWorkerRequest,
        cancelled: &AtomicBool,
        on_progress: &dyn Fn(Value),
    ) -> Result<Value, String> {
        let _gate = self
            .request_gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cancelled.load(Ordering::SeqCst) {
            return Err("操作已取消".into());
        }
        let request_json = serde_json::to_string(request)
            .map_err(|error| format!("serialize OCR request failed: {error}"))?;
        if request_json.len() > 64 * 1024 {
            return Err("OCR request exceeds protocol line limit".into());
        }

        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            self.ensure_started(&mut state)?;
            let stdin = state
                .stdin
                .as_mut()
                .ok_or_else(|| "OCR worker stdin is unavailable".to_string())?;
            stdin
                .write_all(request_json.as_bytes())
                .and_then(|_| stdin.write_all(b"\n"))
                .and_then(|_| stdin.flush())
                .map_err(|error| {
                    Self::terminate_locked(&mut state);
                    format!("write OCR request failed: {error}")
                })?;
        }

        let deadline = Instant::now() + self.timeout;
        loop {
            if cancelled.load(Ordering::SeqCst) {
                self.terminate();
                return Err("操作已取消".into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.terminate();
                return Err(format!(
                    "OCR worker timed out after {}s",
                    self.timeout.as_secs()
                ));
            }
            let wait_for = remaining.min(POLL_INTERVAL);
            let received = {
                let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                let responses = state
                    .responses
                    .as_ref()
                    .ok_or_else(|| "OCR worker response channel is unavailable".to_string())?;
                responses.recv_timeout(wait_for)
            };
            match received {
                Ok(line) => {
                    if line.len() > MAX_RESPONSE_LINE_BYTES {
                        self.terminate();
                        return Err("OCR worker response exceeds protocol limit".into());
                    }
                    let frame: Value = match serde_json::from_str(&line) {
                        Ok(value) => value,
                        Err(error) => {
                            self.terminate();
                            return Err(format!("invalid OCR worker frame: {error}"));
                        }
                    };
                    let frame_request_id = frame.get("requestId").and_then(Value::as_str);
                    if frame_request_id != Some(request.request_id.as_str()) {
                        self.terminate();
                        return Err("OCR worker response requestId mismatch".into());
                    }
                    if frame.get("protocolVersion").and_then(Value::as_u64)
                        != Some(u64::from(PROTOCOL_VERSION))
                    {
                        self.terminate();
                        return Err("OCR worker response protocol version mismatch".into());
                    }
                    match frame.get("type").and_then(Value::as_str).unwrap_or("") {
                        "progress" => on_progress(frame),
                        "result" => return Ok(frame),
                        "error" => {
                            let code = frame
                                .get("code")
                                .and_then(Value::as_str)
                                .unwrap_or("worker-error");
                            let message = frame
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("OCR worker returned an error");
                            return Err(format!("{code}: {message}"));
                        }
                        other => {
                            self.terminate();
                            return Err(format!("unknown OCR worker frame type: {other}"));
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    let exited = {
                        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                        match state.child.as_mut() {
                            Some(child) => child
                                .try_wait()
                                .map(|status| status.is_some())
                                .unwrap_or(true),
                            None => true,
                        }
                    };
                    if exited {
                        self.terminate();
                        return Err("OCR worker exited before returning a result".into());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.terminate();
                    return Err("OCR worker stdout closed unexpectedly".into());
                }
            }
        }
    }

    /// Kill the active worker, causing an in-flight request to fail promptly.
    pub fn cancel_current(&self) {
        self.terminate();
    }

    pub fn shutdown(&self) {
        self.terminate();
    }

    fn is_running(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match state.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => {
                    Self::terminate_locked(&mut state);
                    false
                }
            },
            None => false,
        }
    }

    fn ensure_started(&self, state: &mut WorkerState) -> Result<(), String> {
        if let Some(child) = state.child.as_mut() {
            match child.try_wait() {
                Ok(None) if state.responses.is_some() && state.stdin.is_some() => return Ok(()),
                Ok(Some(_)) | Err(_) => Self::terminate_locked(state),
                Ok(None) => Self::terminate_locked(state),
            }
        }
        if !self.worker_exe.is_file() {
            return Err(format!(
                "OCR worker executable is unavailable: {}",
                self.worker_exe.display()
            ));
        }
        let mut command = Command::new(&self.worker_exe);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(parent) = self.worker_exe.parent() {
            command.current_dir(parent);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("spawn OCR worker failed: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "OCR worker stdin was not piped".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "OCR worker stdout was not piped".to_string())?;
        if let Some(stderr) = child.stderr.take() {
            thread::Builder::new()
                .name("ocr-worker-stderr".into())
                .spawn(move || {
                    let mut reader = std::io::BufReader::new(stderr);
                    let mut sink = Vec::new();
                    let _ = reader.read_to_end(&mut sink);
                })
                .map_err(|error| format!("spawn OCR worker stderr thread failed: {error}"))?;
        }
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("ocr-worker-stdout".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|error| format!("spawn OCR worker stdout thread failed: {error}"))?;
        state.child = Some(child);
        state.stdin = Some(stdin);
        state.responses = Some(rx);
        Ok(())
    }

    fn terminate(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        Self::terminate_locked(&mut state);
    }

    fn terminate_locked(state: &mut WorkerState) {
        if let Some(child) = state.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        state.child = None;
        state.stdin = None;
        state.responses = None;
    }
}

impl Drop for OcrWorkerManager {
    fn drop(&mut self) {
        Self::terminate_locked(self.state.get_mut().unwrap_or_else(|p| p.into_inner()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cruciblebox-ocr-manager-{tag}-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn request_uses_worker_protocol_shape() {
        let request = OcrWorkerRequest::new(
            "task-1".into(),
            "C:\\input.png".into(),
            Some("ch".into()),
            Some("cpu".into()),
            Some("C:\\models".into()),
            Some("C:\\models\\dict.txt".into()),
            Some("ppocrv4-mobile-zh-en".into()),
        );
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["requestId"], "task-1");
        assert_eq!(value["task"], "ocr");
        assert_eq!(value["options"]["dictionaryPath"], "C:\\models\\dict.txt");
    }

    #[test]
    fn resolver_finds_development_worker() {
        let dir = temp_dir("resolver");
        let worker = dir.join("ocr-worker/target/debug/ocr-worker.exe");
        fs::create_dir_all(worker.parent().unwrap()).unwrap();
        fs::write(&worker, b"test").unwrap();
        let found = OcrWorkerManager::resolve_worker_exe(None, Some(dir.clone()));
        assert_eq!(found, worker);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unavailable_worker_fails_without_spawning() {
        let manager = OcrWorkerManager::new(
            PathBuf::from("C:\\definitely-missing\\ocr-worker.exe"),
            Duration::from_millis(100),
        );
        let request = OcrWorkerRequest::new(
            "task-1".into(),
            "input.png".into(),
            None,
            None,
            None,
            None,
            None,
        );
        let cancelled = AtomicBool::new(false);
        let error = manager.run(&request, &cancelled, &|_| {}).unwrap_err();
        assert!(error.contains("unavailable"));
    }
}

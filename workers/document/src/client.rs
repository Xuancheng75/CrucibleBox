use crate::protocol::*;
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Admission {
    active: usize,
    waiting: usize,
}
pub struct Client {
    executable: PathBuf,
    pdfium: Option<PathBuf>,
    jobs: PathBuf,
    timeout: Duration,
    admission: Mutex<Admission>,
    changed: Condvar,
    lease: Option<crate::runtime::Lease>,
}
struct Permit<'a>(&'a Client);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = self.0.admission.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        self.0.changed.notify_one();
    }
}
pub struct JobResult {
    pub value: Value,
    root: PathBuf,
    artifacts: Vec<Artifact>,
}
impl JobResult {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn artifact(&self, path: &str) -> Result<PathBuf, String> {
        let candidate = Path::new(path);
        let canonical = candidate.canonicalize().map_err(|e| e.to_string())?;
        let root = self
            .root
            .join("artifacts")
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !canonical.starts_with(&root) {
            return Err("DOCUMENT_ARTIFACT_ESCAPE".into());
        }
        let mut current = candidate.to_path_buf();
        while current.starts_with(&self.root) {
            let meta = fs::symlink_metadata(&current).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("DOCUMENT_ARTIFACT_LINK".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("DOCUMENT_ARTIFACT_LINK".into());
                }
            }
            if current == self.root {
                break;
            }
            if !current.pop() {
                return Err("DOCUMENT_ARTIFACT_ESCAPE".into());
            }
        }
        let relative = canonical
            .strip_prefix(&root)
            .map_err(|_| "DOCUMENT_ARTIFACT_ESCAPE")?;
        let expected = self
            .artifacts
            .iter()
            .find(|a| a.path == relative)
            .ok_or("DOCUMENT_ARTIFACT_UNDECLARED")?;
        let actual = artifact_reference(&canonical)?;
        if actual.bytes != expected.reference.bytes || actual.sha256 != expected.reference.sha256 {
            return Err("DOCUMENT_ARTIFACT_CHANGED".into());
        }
        Ok(canonical)
    }
}
impl Drop for JobResult {
    fn drop(&mut self) {
        if self
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("document-job-"))
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
struct Process {
    child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}
impl Process {
    fn attach(mut child: Child) -> Result<Self, String> {
        #[cfg(windows)]
        unsafe {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                let _ = child.kill();
                let _ = child.wait();
                return Err("DOCUMENT_JOB_CREATE_FAILED".into());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0
            {
                CloseHandle(job);
                let _ = child.kill();
                let _ = child.wait();
                return Err("DOCUMENT_JOB_ASSIGN_FAILED".into());
            }
            Ok(Self { child, job })
        }
        #[cfg(not(windows))]
        {
            Ok(Self { child })
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Client {
    pub fn new(
        executable: PathBuf,
        pdfium: Option<PathBuf>,
        jobs: PathBuf,
        timeout: Duration,
    ) -> Self {
        Self {
            executable,
            pdfium,
            jobs,
            timeout,
            admission: Mutex::new(Admission::default()),
            changed: Condvar::new(),
            lease: None,
        }
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    fn admit(&self, deadline: Instant, cancelled: &dyn Fn() -> bool) -> Result<Permit<'_>, String> {
        let mut state = self
            .admission
            .lock()
            .map_err(|_| "DOCUMENT_ADMISSION_POISONED")?;
        if state.active >= 2 && state.waiting >= 16 {
            return Err("DOCUMENT_QUEUE_FULL".into());
        }
        state.waiting += 1;
        loop {
            if cancelled() || Instant::now() >= deadline {
                state.waiting -= 1;
                return Err(if cancelled() {
                    "DOCUMENT_CANCELLED"
                } else {
                    "DOCUMENT_TIMEOUT"
                }
                .into());
            }
            if state.active < 2 {
                state.waiting -= 1;
                state.active += 1;
                return Ok(Permit(self));
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(20))
                .map_err(|_| "DOCUMENT_ADMISSION_POISONED")?
                .0;
        }
    }
    pub fn leased(lease: crate::runtime::Lease, jobs: PathBuf, timeout: Duration) -> Self {
        let mut client = Self::new(
            lease.executable.clone(),
            Some(lease.pdfium.clone()),
            jobs,
            timeout,
        );
        client.lease = Some(lease);
        client
    }
    pub fn run(
        &self,
        operation: &Operation,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<JobResult, String> {
        let deadline = Instant::now() + self.timeout;
        let _permit = self.admit(deadline, cancelled)?;
        if let Some(lease) = &self.lease {
            lease.verify()?;
        }
        if !self.executable.is_file() {
            return Err("DOCUMENT_WORKER_UNAVAILABLE".into());
        }
        fs::create_dir_all(&self.jobs).map_err(|e| e.to_string())?;
        let mut random = [0u8; 32];
        getrandom::getrandom(&mut random).map_err(|e| e.to_string())?;
        let nonce = random
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        let request_id = format!("document-{nonce}");
        let root = self.jobs.join(format!("document-job-{nonce}"));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let mut result = JobResult {
            value: Value::Null,
            artifacts: Vec::new(),
            root,
        };
        let request = Request {
            wire_version: WIRE_VERSION,
            nonce: nonce.clone(),
            request_id: request_id.clone(),
            input: write_reference(&result.root.join("input.json"), operation)?,
        };
        let raw = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        if raw.len() + 1 > MAX_FRAME {
            return Err("DOCUMENT_FRAME_BUDGET".into());
        }
        let mut command = Command::new(&self.executable);
        command
            .args([result.root.to_string_lossy().as_ref(), &nonce, &request_id])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(pdfium) = &self.pdfium {
            command.env("PDFIUM_LIB_PATH", pdfium);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut process = Process::attach(
            command
                .spawn()
                .map_err(|e| format!("DOCUMENT_WORKER_SPAWN: {e}"))?,
        )?;
        let stdout = process.child.stdout.take().ok_or("DOCUMENT_WORKER_PIPE")?;
        if let Some(stderr) = process.child.stderr.take() {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut error = Vec::new();
                let _ = stderr.take(64 * 1024).read_to_end(&mut error);
                if !error.is_empty() {
                    eprintln!("[document-worker] {}", String::from_utf8_lossy(&error));
                }
            });
        }
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(read_frame(stdout));
        });
        let mut stdin = process.child.stdin.take().ok_or("DOCUMENT_WORKER_PIPE")?;
        stdin
            .write_all(&raw)
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| e.to_string())?;
        drop(stdin);
        let bytes = loop {
            if cancelled() {
                return Err("DOCUMENT_CANCELLED".into());
            }
            if Instant::now() >= deadline {
                return Err("DOCUMENT_TIMEOUT".into());
            }
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(value) => break value.map_err(|e| format!("DOCUMENT_WORKER_EXITED: {e}"))?,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("DOCUMENT_WORKER_EXITED".into()),
            }
        };
        let response: Response =
            serde_json::from_slice(&bytes).map_err(|_| "INVALID_DOCUMENT_RESPONSE")?;
        if response.wire_version != WIRE_VERSION
            || response.nonce != nonce
            || response.request_id != request_id
            || response.result.is_some() == response.error.is_some()
        {
            return Err("INVALID_DOCUMENT_RESPONSE".into());
        }
        loop {
            if cancelled() {
                return Err("DOCUMENT_CANCELLED".into());
            }
            if Instant::now() >= deadline {
                return Err("DOCUMENT_TIMEOUT".into());
            }
            match process.child.try_wait().map_err(|e| e.to_string())? {
                Some(status) if status.success() => break,
                Some(_) => return Err("DOCUMENT_WORKER_EXITED".into()),
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        if let Some(error) = response.error {
            return Err(format!("DOCUMENT_OPERATION_FAILED: {error}"));
        }
        let completed: Completed = read_reference(
            &result.root.join("result.json"),
            &response.result.ok_or("INVALID_DOCUMENT_RESPONSE")?,
        )?;
        if completed.artifacts.len() > 4096 {
            return Err("DOCUMENT_ARTIFACT_COUNT".into());
        }
        let mut paths = std::collections::HashSet::new();
        for artifact in &completed.artifacts {
            if artifact.path.as_os_str().is_empty()
                || artifact
                    .path
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
                || !paths.insert(artifact.path.clone())
            {
                return Err("INVALID_DOCUMENT_ARTIFACT_PATH".into());
            }
        }
        result.value = completed.value;
        result.artifacts = completed.artifacts;
        for artifact in &result.artifacts {
            result.artifact(
                &result
                    .root
                    .join("artifacts")
                    .join(&artifact.path)
                    .to_string_lossy(),
            )?;
        }
        Ok(result)
    }
}

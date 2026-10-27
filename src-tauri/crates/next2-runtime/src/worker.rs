pub use crate::process::StopHandle;
use crate::{package::ApprovedPlugin, process::Process, transport};
use cruciblebox_next2_protocol::{
    Backend, Request, ResourceMount, RpcError, BUDGET_REQUEST_TIMEOUT_MS,
};
use serde_json::{json, Value};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

enum Output {
    Frame(Value),
    Failure(String),
}
enum Outcome {
    Value(Value),
    DomainError(RpcError),
}
pub struct Worker {
    process: Process,
    input: Option<mpsc::SyncSender<Value>>,
    output: Option<mpsc::Receiver<Output>>,
    threads: Vec<JoinHandle<()>>,
    session: String,
    sequence: u64,
}
impl Worker {
    pub fn start(
        plugin: &ApprovedPlugin,
        data: &Path,
        resources: &[ResourceMount],
    ) -> Result<Self, String> {
        plugin.revalidate()?;
        let Some(Backend::NativeBackend(backend)) = &plugin.manifest.backend else {
            return Err("NATIVE_BACKEND_REQUIRED".into());
        };
        let command = Command::new(plugin.root.join(&backend.entry));
        Self::start_command(plugin, data, resources, command)
    }
    /// The host supplies its own QuickJS runner; the package can only select a declared JS entry.
    pub fn start_js(
        plugin: &ApprovedPlugin,
        data: &Path,
        resources: &[ResourceMount],
        runner: &Path,
    ) -> Result<Self, String> {
        let Some(Backend::JsBackend(backend)) = &plugin.manifest.backend else {
            return Err("JS_BACKEND_REQUIRED".into());
        };
        let mut command = Command::new(runner);
        command.arg("--next2").arg(&plugin.root).arg(&backend.entry);
        Self::start_command(plugin, data, resources, command)
    }
    fn start_command(
        plugin: &ApprovedPlugin,
        data: &Path,
        resources: &[ResourceMount],
        mut command: Command,
    ) -> Result<Self, String> {
        plugin.revalidate()?;
        std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
        let data = std::fs::canonicalize(data).map_err(|e| e.to_string())?;
        command
            .current_dir(&plugin.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut process = Process::spawn(&mut command)?;
        let mut input = process
            .child
            .stdin
            .take()
            .ok_or("NATIVE_STDIN_UNAVAILABLE")?;
        let mut output = process
            .child
            .stdout
            .take()
            .ok_or("NATIVE_STDOUT_UNAVAILABLE")?;
        let (tx, rx) = mpsc::sync_channel(32);
        let sender = tx.clone();
        let reader = std::thread::Builder::new()
            .name("next2-native-reader".into())
            .spawn(move || loop {
                let event = match transport::read_frame(&mut output) {
                    Ok(Some(frame)) => Output::Frame(frame),
                    Ok(None) => Output::Failure("NATIVE_BACKEND_EOF".into()),
                    Err(error) => Output::Failure(error),
                };
                let failed = matches!(event, Output::Failure(_));
                if sender.send(event).is_err() || failed {
                    break;
                }
            })
            .map_err(|e| e.to_string())?;
        let (write_tx, write_rx) = mpsc::sync_channel::<Value>(32);
        let writer = std::thread::Builder::new()
            .name("next2-native-writer".into())
            .spawn(move || {
                while let Ok(frame) = write_rx.recv() {
                    if let Err(error) = transport::write_frame(&mut input, &frame) {
                        let _ = tx.send(Output::Failure(format!("NATIVE_WRITE_FAILED: {error}")));
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
        let session = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let worker = Self {
            process,
            input: Some(write_tx),
            output: Some(rx),
            threads: vec![reader, writer],
            session,
            sequence: 0,
        };
        let deadline = Instant::now()
            + Duration::from_millis(cruciblebox_next2_protocol::BUDGET_HANDSHAKE_TIMEOUT_MS as u64);
        if worker.receive(deadline)? != json!({"kind":"hello","wireVersion":4,"sdkApiVersion":6}) {
            return Err("NATIVE_HANDSHAKE_MISMATCH".into());
        }
        worker.send(json!({"kind":"initialize","wireVersion":4,"session":worker.session,"pluginId":plugin.manifest.id,
            "dataDirectory":data,"resources":resources}))?;
        if worker.receive(deadline)? != json!({"kind":"ready","wireVersion":4}) {
            return Err("NATIVE_HANDSHAKE_MISMATCH".into());
        }
        Ok(worker)
    }
    fn send(&self, frame: Value) -> Result<(), String> {
        cruciblebox_next2_protocol::validate_backend_frame(&frame.to_string())?;
        self.input
            .as_ref()
            .ok_or("NATIVE_STOPPED")?
            .try_send(frame)
            .map_err(|e| format!("NATIVE_QUEUE_UNAVAILABLE: {e}"))
    }
    fn receive(&self, deadline: Instant) -> Result<Value, String> {
        match self
            .output
            .as_ref()
            .ok_or("NATIVE_STOPPED")?
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "NATIVE_TIMEOUT")?
        {
            Output::Frame(frame) => Ok(frame),
            Output::Failure(error) => Err(error),
        }
    }
    pub fn pid(&self) -> u32 {
        self.process.child.id()
    }
    pub fn session(&self) -> &str {
        &self.session
    }
    pub fn stop_handle(&self) -> StopHandle {
        self.process.stop_handle()
    }
    /// `caller_id`, task ID and capability authorization come from host admission.
    pub fn call(
        &mut self,
        caller_id: &str,
        method: &str,
        params: Value,
        task_id: Option<&str>,
        cancel: &AtomicBool,
        mut capability: impl FnMut(&Request) -> Result<Value, RpcError>,
    ) -> Result<Value, RpcError> {
        let result = self.call_inner(caller_id, method, params, task_id, cancel, &mut capability);
        match result {
            Ok(Outcome::Value(value)) => Ok(value),
            Ok(Outcome::DomainError(error)) => Err(error),
            Err(message) => {
                let code = if message == "CANCELLED" {
                    "CANCELLED"
                } else {
                    "NATIVE_TRANSPORT"
                };
                if let Err(stop) = self.process.force_stop() {
                    return Err(RpcError {
                        code: "NATIVE_STOP_FAILED".into(),
                        message: format!("{message}; {stop}"),
                    });
                }
                Err(RpcError {
                    code: code.into(),
                    message,
                })
            }
        }
    }
    fn call_inner(
        &mut self,
        caller_id: &str,
        method: &str,
        params: Value,
        task_id: Option<&str>,
        cancel: &AtomicBool,
        capability: &mut impl FnMut(&Request) -> Result<Value, RpcError>,
    ) -> Result<Outcome, String> {
        self.sequence += 1;
        let id = format!("native-{}", self.sequence);
        let mut frame = json!({"kind":"execute","callerId":caller_id,"request":{"wireVersion":4,"requestId":id,
            "session":self.session,"target":{"kind":"backend"},"method":method,"params":params}});
        if let Some(task_id) = task_id {
            frame["taskId"] = json!(task_id);
        }
        self.send(frame)?;
        let deadline = task_id
            .is_none()
            .then(|| Instant::now() + Duration::from_millis(BUDGET_REQUEST_TIMEOUT_MS as u64));
        let mut cancel_deadline = None;
        loop {
            if cancel.load(Ordering::SeqCst) && cancel_deadline.is_none() {
                if let Some(task_id) = task_id {
                    self.send(json!({"kind":"cancel","wireVersion":4,"taskId":task_id}))?;
                }
                cancel_deadline = Some(
                    Instant::now()
                        + Duration::from_millis(
                            cruciblebox_next2_protocol::BUDGET_STOP_TIMEOUT_MS as u64,
                        ),
                );
            }
            let now = Instant::now();
            if cancel_deadline.is_some_and(|end| now >= end) {
                self.process.force_stop()?;
                return Err("CANCELLED".into());
            }
            if deadline.is_some_and(|deadline| now >= deadline) {
                return Err("NATIVE_TIMEOUT".into());
            }
            let frame = match self
                .output
                .as_ref()
                .ok_or("NATIVE_STOPPED")?
                .recv_timeout(Duration::from_millis(25))
            {
                Ok(Output::Frame(frame)) => frame,
                Ok(Output::Failure(error)) => return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(error) => return Err(format!("NATIVE_DISCONNECTED: {error}")),
            };
            match frame["kind"].as_str() {
                Some("reply") => {
                    let response = cruciblebox_next2_protocol::validate_response(
                        &frame["response"].to_string(),
                        &id,
                    )?;
                    if cancel_deadline.is_some() {
                        return Err("CANCELLED".into());
                    }
                    return match response {
                        cruciblebox_next2_protocol::Response::Success(success) => {
                            Ok(Outcome::Value(success.result))
                        }
                        cruciblebox_next2_protocol::Response::Failure(failure) => {
                            Ok(Outcome::DomainError(failure.error))
                        }
                    };
                }
                Some("capability") => {
                    let request = cruciblebox_next2_protocol::validate_request(
                        &frame["request"].to_string(),
                    )?;
                    if request.session != self.session {
                        return Err("SESSION_DENIED".into());
                    }
                    let response = match capability(&request) {
                        Ok(result) => {
                            json!({"wireVersion":4,"requestId":request.request_id,"ok":true,"result":result})
                        }
                        Err(error) => {
                            json!({"wireVersion":4,"requestId":request.request_id,"ok":false,"error":error})
                        }
                    };
                    self.send(json!({"kind":"capability-result","response":response}))?;
                }
                _ => return Err("UNEXPECTED_NATIVE_FRAME".into()),
            }
        }
    }
    pub fn stop(mut self) -> Result<(), String> {
        self.send(json!({"kind":"stop","wireVersion":4}))?;
        let deadline = Instant::now()
            + Duration::from_millis(cruciblebox_next2_protocol::BUDGET_STOP_TIMEOUT_MS as u64);
        while Instant::now() < deadline {
            if self
                .process
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return self.process.force_stop();
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        self.process.force_stop()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.process.force_stop();
        self.input.take();
        self.output.take();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

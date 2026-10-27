//! Wire 4 backend helpers. Native plugins execute as trusted local code.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
pub const MAX_FRAME_BYTES: usize = 65536;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}
impl Error {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new("IO_ERROR", error.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::new("INVALID_FRAME", error.to_string())
    }
}
impl From<String> for Error {
    fn from(error: String) -> Self {
        Self::new("PLUGIN_ERROR", error)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

fn read(input: &mut impl Read) -> Result<Option<Value>> {
    let mut size = [0u8; 4];
    match input.read(&mut size[..1])? {
        0 => return Ok(None),
        _ => input.read_exact(&mut size[1..])?,
    }
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err(Error::new("FRAME_BUDGET_EXCEEDED", "Invalid frame size"));
    }
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}
fn write(output: &mut impl Write, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::new(
            "FRAME_BUDGET_EXCEEDED",
            "Use file results for large data",
        ));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}
struct Shared {
    output: Mutex<std::io::Stdout>,
    pending: Mutex<HashMap<String, mpsc::SyncSender<Value>>>,
    sequence: AtomicU64,
    init: Value,
}
#[derive(Clone)]
pub struct Client(Arc<Shared>);
impl Client {
    fn send(&self, value: &Value) -> Result<()> {
        write(
            &mut *self
                .0
                .output
                .lock()
                .map_err(|_| Error::new("OUTPUT_UNAVAILABLE", "Protocol output unavailable"))?,
            value,
        )
    }
    pub fn plugin_id(&self) -> &str {
        self.0.init["pluginId"]
            .as_str()
            .expect("validated initialization")
    }
    pub fn data_directory(&self) -> PathBuf {
        self.0.init["dataDirectory"]
            .as_str()
            .expect("validated initialization")
            .into()
    }
    pub fn resource(&self, id: &str) -> Option<PathBuf> {
        self.0.init["resources"]
            .as_array()?
            .iter()
            .find(|item| item["id"] == id)?["directory"]
            .as_str()
            .map(PathBuf::from)
    }
    pub fn platform(&self, method: &str, params: Value) -> Result<Value> {
        self.call(json!({"kind":"platform"}), method, params)
    }
    pub fn service(&self, id: &str, method: &str, params: Value) -> Result<Value> {
        self.call(json!({"kind":"service","id":id}), method, params)
    }
    fn call(&self, target: Value, method: &str, params: Value) -> Result<Value> {
        let id = format!("native-{}", self.0.sequence.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::sync_channel(1);
        self.0
            .pending
            .lock()
            .map_err(|_| Error::new("PROTOCOL_UNAVAILABLE", "Pending requests unavailable"))?
            .insert(id.clone(), tx);
        let result = (|| {
            self.send(&json!({"kind":"capability","request":{"wireVersion":4,"requestId":id,"session":self.0.init["session"],"target":target,"method":method,"params":params}}))?;
            let response = rx
                .recv_timeout(Duration::from_secs(30))
                .map_err(|error| Error::new("HOST_REQUEST_FAILED", error.to_string()))?;
            if response["wireVersion"] != 4 || response["requestId"] != id {
                return Err(Error::new(
                    "RESPONSE_MISMATCH",
                    "Host response did not correlate",
                ));
            }
            if response["ok"] == true {
                Ok(response["result"].clone())
            } else {
                Err(serde_json::from_value(response["error"].clone())?)
            }
        })();
        self.0
            .pending
            .lock()
            .map_err(|_| Error::new("PROTOCOL_UNAVAILABLE", "Pending requests unavailable"))?
            .remove(&id);
        result
    }
}
pub struct Call {
    pub client: Client,
    pub method: String,
    pub params: Value,
    pub caller_id: String,
    pub task_id: Option<String>,
    cancelled: Arc<AtomicBool>,
    sequence: AtomicU64,
}
impl Call {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    pub fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::new("CANCELLED", "Task cancelled"))
        } else {
            Ok(())
        }
    }
    pub fn progress(&self, stage: &str, percent: u32, message: &str) -> Result<()> {
        let task = self.task_id.as_deref().ok_or_else(|| {
            Error::new("TASK_EXECUTION_REQUIRED", "Progress requires a host task")
        })?;
        self.client.platform("tasks.progress", json!({"taskId":task,"sequence":self.sequence.fetch_add(1, Ordering::Relaxed) + 1,"stage":stage,"percent":percent,"message":message}))?;
        Ok(())
    }
}
type Active = Arc<Mutex<Option<(Option<String>, Arc<AtomicBool>)>>>;

/// Factory and handler run serially on the execution thread. The stdin thread
/// continues receiving cancellation and correlated capability responses.
pub fn serve<S: Send + 'static>(
    factory: impl FnOnce(Client) -> Result<S> + Send + 'static,
    mut handler: impl FnMut(&mut S, &Call) -> Result<Value> + Send + 'static,
) -> Result<()> {
    let mut input = std::io::stdin();
    write(
        &mut std::io::stdout(),
        &json!({"kind":"hello","wireVersion":4,"sdkApiVersion":6}),
    )?;
    let init = read(&mut input)?
        .ok_or_else(|| Error::new("HOST_DISCONNECTED", "Missing initialization"))?;
    if init["kind"] != "initialize"
        || init["wireVersion"] != 4
        || init["session"].as_str().is_none()
        || init["pluginId"].as_str().is_none()
        || init["dataDirectory"].as_str().is_none()
        || !init["resources"].is_array()
    {
        return Err(Error::new(
            "INVALID_INITIALIZATION",
            "Invalid host initialization",
        ));
    }
    let client = Client(Arc::new(Shared {
        output: Mutex::new(std::io::stdout()),
        pending: Mutex::new(HashMap::new()),
        sequence: AtomicU64::new(1),
        init,
    }));
    let active: Active = Arc::new(Mutex::new(None));
    let worker_active = active.clone();
    let worker_client = client.clone();
    let (tx, rx) = mpsc::sync_channel::<Value>(1);
    let worker = std::thread::Builder::new().name("plugin-execution".into()).spawn(move || -> Result<()> {
        let mut factory = Some(factory);
        let mut state = None;
        while let Ok(frame) = rx.recv() {
            let cancelled = worker_active.lock().map_err(|_| Error::new("EXECUTION_UNAVAILABLE", "Execution state unavailable"))?.as_ref().expect("execute registered before queueing").1.clone();
            let request = &frame["request"];
            let call = Call { client:worker_client.clone(), method:request["method"].as_str().ok_or_else(|| Error::new("INVALID_EXECUTE", "Missing method"))?.into(), params:request["params"].clone(), caller_id:frame["callerId"].as_str().ok_or_else(|| Error::new("INVALID_EXECUTE", "Missing caller"))?.into(), task_id:frame["taskId"].as_str().map(str::to_owned), cancelled, sequence:AtomicU64::new(0) };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if state.is_none() { state = Some(factory.take().expect("factory runs once")(worker_client.clone())); }
                match state.as_mut().expect("initialized state") {
                    Ok(state) => call.check_cancelled().and_then(|_| handler(state, &call)),
                    Err(error) => Err(error.clone()),
                }
            }));
            let panicked = result.is_err();
            let result = result.unwrap_or_else(|_| Err(Error::new("PLUGIN_PANIC", "Plugin execution panicked; backend will stop")));
            let response = match result { Ok(result) => json!({"wireVersion":4,"requestId":request["requestId"],"ok":true,"result":result}), Err(error) => json!({"wireVersion":4,"requestId":request["requestId"],"ok":false,"error":error}) };
            *worker_active.lock().map_err(|_| Error::new("EXECUTION_UNAVAILABLE", "Execution state unavailable"))? = None;
            if let Err(error) = worker_client.send(&json!({"kind":"reply","response":response})) {
                let _ = worker_client.send(&json!({"kind":"reply","response":{"wireVersion":4,"requestId":request["requestId"],"ok":false,"error":error}}));
                std::process::exit(70);
            }
            if panicked { std::process::exit(70); }
        }
        Ok(())
    })?;
    client.send(&json!({"kind":"ready","wireVersion":4}))?;
    let result = (|| {
        while let Some(frame) = read(&mut input)? {
            match frame["kind"].as_str() {
                Some("execute") => {
                    if frame["request"]["wireVersion"] != 4
                        || frame["request"]["session"] != client.0.init["session"]
                    {
                        return Err(Error::new("SESSION_DENIED", "Execution session mismatch"));
                    }
                    let mut active = active.lock().map_err(|_| {
                        Error::new("EXECUTION_UNAVAILABLE", "Execution state unavailable")
                    })?;
                    if active.is_some() {
                        return Err(Error::new("BUSY", "Execution already active"));
                    }
                    *active = Some((
                        frame["taskId"].as_str().map(str::to_owned),
                        Arc::new(AtomicBool::new(false)),
                    ));
                    tx.try_send(frame)
                        .map_err(|error| Error::new("EXECUTION_UNAVAILABLE", error.to_string()))?;
                }
                Some("capability-result") => {
                    let response = &frame["response"];
                    let id = response["requestId"]
                        .as_str()
                        .ok_or_else(|| Error::new("INVALID_RESPONSE", "Missing request ID"))?;
                    if let Some(sender) = client
                        .0
                        .pending
                        .lock()
                        .map_err(|_| {
                            Error::new("PROTOCOL_UNAVAILABLE", "Pending requests unavailable")
                        })?
                        .remove(id)
                    {
                        let _ = sender.send(response.clone());
                    }
                }
                Some("cancel") => {
                    if let Some((task, cancelled)) = active
                        .lock()
                        .map_err(|_| {
                            Error::new("EXECUTION_UNAVAILABLE", "Execution state unavailable")
                        })?
                        .as_ref()
                    {
                        if task.as_deref() == frame["taskId"].as_str() {
                            cancelled.store(true, Ordering::SeqCst);
                        }
                    }
                }
                Some("stop") => break,
                _ => {
                    return Err(Error::new(
                        "UNEXPECTED_HOST_FRAME",
                        "Unsupported host frame",
                    ))
                }
            }
        }
        Ok(())
    })();
    if let Some((_, cancelled)) = active
        .lock()
        .map_err(|_| Error::new("EXECUTION_UNAVAILABLE", "Execution state unavailable"))?
        .as_ref()
    {
        cancelled.store(true, Ordering::SeqCst);
    }
    client
        .0
        .pending
        .lock()
        .map_err(|_| Error::new("PROTOCOL_UNAVAILABLE", "Pending requests unavailable"))?
        .clear();
    drop(tx);
    let stopped = worker
        .join()
        .map_err(|_| Error::new("PLUGIN_PANIC", "Plugin execution panicked"))?;
    result.and(stopped)
}

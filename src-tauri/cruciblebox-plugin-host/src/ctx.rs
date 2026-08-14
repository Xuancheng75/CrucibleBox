// ctx 注入与 RPC 客户端（1.8.2）
// 对等 PluginProcessEntry.buildContext：logger/database/storage/api 全量方法面。
//
// 帧分发架构（解决 oracle H1/H2）：
// - 后台线程独占 stdin 读帧 → mpsc channel（避免 BufReader 缓冲吞帧 / 死锁）
// - 主循环从队列取 worker 请求帧（优先消费 pending 缓冲）
// - ctx.__hostRequest 同步往返：recv_timeout(30s) 超时；收到不匹配的帧入 pending
//   缓冲（宿主并发推送的 worker 请求不丢失）

use crate::envelope;
use crate::frame;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

const RPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 共享帧队列：后台读线程写入，主循环与 ctx.call 消费。
pub struct FrameQueue {
    rx: Mutex<mpsc::Receiver<Option<Vec<u8>>>>,
    /// 不匹配当前响应 requestId 的帧（宿主并发推送的 worker 请求）暂存于此
    pending: Mutex<VecDeque<Vec<u8>>>,
}

static FRAME_QUEUE: OnceLock<Arc<FrameQueue>> = OnceLock::new();

impl FrameQueue {
    pub fn global() -> &'static Arc<FrameQueue> {
        FRAME_QUEUE.get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            // 后台读线程：独占 stdin
            std::thread::Builder::new()
                .name("frame-reader".into())
                .spawn(move || {
                    let mut stdin = std::io::stdin().lock();
                    loop {
                        match frame::read_frame(&mut stdin) {
                            Ok(Some(bytes)) => {
                                if tx.send(Some(bytes)).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => {
                                let _ = tx.send(None);
                                break;
                            }
                            Err(e) => {
                                eprintln!("[host] frame read error: {e}");
                                let _ = tx.send(None);
                                break;
                            }
                        }
                    }
                })
                .expect("spawn frame reader");
            Arc::new(FrameQueue {
                rx: Mutex::new(rx),
                pending: Mutex::new(VecDeque::new()),
            })
        })
    }

    /// 取下一帧（worker 请求路径：优先 pending，其次 channel 阻塞等待）。
    pub fn take_worker_frame(&self) -> Option<Vec<u8>> {
        if let Some(bytes) = self.pending.lock().unwrap().pop_front() {
            return Some(bytes);
        }
        self.rx.lock().unwrap().recv().ok().flatten()
    }

    /// ctx.call 等待宿主响应：带超时；不匹配 requestId 的帧入 pending。
    fn take_matching_response(&self, request_id: &str) -> Result<Option<Value>, String> {
        let deadline = std::time::Instant::now() + RPC_TIMEOUT;
        loop {
            let bytes = {
                // 先查 pending（可能有宿主此前推送的帧）
                let mut pend = self.pending.lock().unwrap();
                if let Some(bytes) = pend.pop_front() {
                    bytes
                } else {
                    drop(pend);
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        return Err("TIMEOUT: waiting for host response".into());
                    }
                    let rx = self.rx.lock().unwrap();
                    match rx.recv_timeout(remaining) {
                        Ok(Some(bytes)) => bytes,
                        Ok(None) => return Err("host closed pipe".into()),
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            return Err("TIMEOUT: waiting for host response".into())
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return Err("frame reader thread died".into())
                        }
                    }
                }
            };
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|e| format!("bad response json: {e}"))?;
            match envelope::validate_host_response(&value, request_id) {
                Ok((true, result, _)) => return Ok(Some(result)),
                Ok((false, _, err)) => {
                    return Err(err.unwrap_or_else(|| "host request failed".into()));
                }
                Err(e) if e.contains("requestId mismatch") => {
                    // 不是本请求的响应（宿主并发推送的 worker 帧）→ 入 pending
                    self.pending.lock().unwrap().push_back(bytes);
                    continue;
                }
                Err(e) => return Err(format!("response validation: {e}")),
            }
        }
    }
}

/// 向宿主发请求并同步等待匹配响应（核心同步往返原语）。
/// 出站保护（H4 sidecar 侧最小面）：method 白名单 + payload 预算。
pub fn host_call(method: &str, params: Value) -> Result<Value, String> {
    if !envelope::HOST_METHODS.contains(&method) {
        return Err(format!("host method not allowed: {method}"));
    }
    envelope::check_payload_budget(&params).map_err(|e| format!("outbound params: {e}"))?;
    let queue = FrameQueue::global();
    let next_id = NEXT_ID.with(|c| {
        *c.borrow_mut() += 1;
        *c.borrow()
    });
    let request_id = format!("r{}", next_id);
    let token = RPC_TOKEN.with(|t| t.borrow().clone());
    let payload = envelope::make_request(&token, &request_id, method, params);
    let bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    frame::write_frame(&mut std::io::stdout(), &bytes).map_err(|e| e.to_string())?;
    queue
        .take_matching_response(&request_id)?
        .ok_or_else(|| "host request failed".into())
}

thread_local! {
    static RPC_TOKEN: RefCell<String> = const { RefCell::new(String::new()) };
    static NEXT_ID: RefCell<u64> = const { RefCell::new(0) };
}

pub fn init(token: String) {
    RPC_TOKEN.with(|t| *t.borrow_mut() = token);
    let _ = FrameQueue::global();
}

/// Rust 全局 `__hostRequest(method, paramsJson) -> resultJson`，注入到 JS。
pub fn register_host_request(ctx: &rquickjs::Ctx) -> rquickjs::Result<()> {
    let f = rquickjs::Function::new(
        ctx.clone(),
        |method: String, params_json: String| -> Result<String, rquickjs::Error> {
            let params: Value = serde_json::from_str(&params_json).map_err(|e| {
                rquickjs::Error::new_from_js_message("String", "String", e.to_string())
            })?;
            let result = host_call(&method, params).map_err(|e| {
                rquickjs::Error::new_from_js_message("call", "call", e.to_string())
            })?;
            serde_json::to_string(&result)
                .map_err(|e| rquickjs::Error::new_from_js_message("String", "String", e.to_string()))
        },
    )?;
    ctx.globals().set("__hostRequest", f)?;
    Ok(())
}

/// buildContext 的 JS 引导（对等 PluginProcessEntry.buildContext 全量方法面）。
/// 所有宿主能力经 __hostRequest 同步转发；fire-and-forget 方法（logger/notify/
/// shortcut.register/event.emit）忽略响应。
pub const CTX_JS: &str = r#"
function __ff(method, params) {
  try { __hostRequest(method, JSON.stringify(params || {})); } catch (e) { /* fire-and-forget */ }
}
function __rpc(method, params) {
  var raw = __hostRequest(method, JSON.stringify(params || {}));
  return raw === undefined ? null : JSON.parse(raw);
}
function __buildCtx(id, config) {
  var ctxObj = { id: id, config: config || {}, logger: {}, database: {}, storage: {}, api: {} };
  var logger = ctxObj.logger;
  ['info', 'warn', 'error', 'debug'].forEach(function (level) {
    logger[level] = function (message) {
      __ff('log.write', { pluginId: id, level: level, message: String(message) });
    };
  });
  var database = ctxObj.database;
  database.query = function (sql, params) { return __rpc('db.query', { sql: sql, params: params || [] }); };
  database.execute = function (sql, params) { __rpc('db.execute', { sql: sql, params: params || [] }); };
  var storage = ctxObj.storage;
  storage.get = function (key) { return __rpc('storage.get', { pluginId: id, key: key }); };
  storage.set = function (key, value) { __rpc('storage.set', { pluginId: id, key: key, value: value }); };
  storage.delete = function (key) { __rpc('storage.delete', { pluginId: id, key: key }); };
  storage.list = function (prefix) { return __rpc('storage.list', { pluginId: id, prefix: prefix || undefined }); };
  storage.batch = function (mutations) { __rpc('storage.batch', { pluginId: id, mutations: mutations || [] }); };
  var api = ctxObj.api;
  api.notify = function (title, body) { __ff('notification.show', { title: title, body: body || '' }); };
  api.openDialog = function (type) { return __rpc('dialog.open', { type: type }); };
  api.fetch = function (url, opts) { return __rpc('network.fetch', { url: url, opts: opts || {} }); };
  api.readFile = function (path) { return __rpc('file.read', { path: path }); };
  api.writeFile = function (path, data) { __rpc('file.write', { path: path, data: data }); };
  api.registerShortcut = function (keys, handler) {
    __ff('shortcut.register', { keys: keys });
    return function () { __ff('shortcut.unregister', { keys: keys }); };
  };
  api.emitEvent = function (event, data) { __ff('event.emit', { event: event, data: data }); };
  api.onEvent = function (event, handler) {
    var subId = 'sub-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
    __ff('event.subscribe', { event: event, subscriptionId: subId });
    return function () { __ff('event.unsubscribe', { event: event, subscriptionId: subId }); };
  };
  api.invokeTrustedService = function (service, operation, payload) {
    return __rpc('trusted.invoke', { service: service, operation: operation, payload: payload });
  };
  return ctxObj;
}
globalThis.__cbCtx = null;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctx_js_is_valid_syntax() {
        let rt = rquickjs::Runtime::new().unwrap();
        let ctx = rquickjs::Context::full(&rt).unwrap();
        ctx.with(|ctx| {
            ctx.eval::<(), _>(CTX_JS).unwrap();
            let built: rquickjs::Value = ctx
                .eval("__buildCtx('p1', {a:1})")
                .unwrap();
            let obj = built.as_object().unwrap();
            assert_eq!(obj.get::<_, String>("id").unwrap(), "p1");
            let has_storage = !obj.get::<_, rquickjs::Value>("storage").unwrap().is_undefined();
            assert!(has_storage);
        });
    }
}

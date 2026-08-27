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

    /// 取下一帧（worker 请求路径：优先 pending 中的 worker 帧，其次 channel 阻塞等待）。
    /// 孤儿响应帧（请求方已超时放弃）在此丢弃，不进入主循环。
    pub fn take_worker_frame(&self) -> Option<Vec<u8>> {
        loop {
            {
                let mut pend = self.pending.lock().unwrap();
                if let Some(pos) = pend.iter().position(|b| frame_kind(b) == "request") {
                    return pend.remove(pos);
                }
            }
            let bytes = self.rx.lock().unwrap().recv().ok().flatten()?;
            if frame_kind(&bytes) == "request" {
                return Some(bytes);
            }
            // 迟到响应：丢弃
        }
    }

    /// ctx.call 等待宿主响应：带超时；并发到达的 worker 帧入 pending 暂存。
    ///
    /// Bug B/C 根因修复（1.9.11）：旧实现在 pending/channel 中弹到 worker 帧时按
    /// 「requestId 不匹配」push 回 pending 后 continue——同一帧被无限弹出比对，
    /// 主线程自旋死循环（永不到达 recv_timeout），宿主 30s 超时后强杀 sidecar。
    /// 现按 kind 区分：只有响应帧才参与匹配；worker 帧入 pending 一次即止。
    fn take_matching_response(&self, request_id: &str) -> Result<Option<Value>, String> {
        let deadline = std::time::Instant::now() + RPC_TIMEOUT;
        loop {
            // pending 中只找「响应」帧（此前误入的）；worker 帧留在原地给主循环
            {
                let mut pend = self.pending.lock().unwrap();
                if let Some(pos) = pend.iter().position(|b| frame_kind(b) == "response") {
                    let bytes = pend.remove(pos).unwrap();
                    return validate_response_bytes(&bytes, request_id);
                }
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("TIMEOUT: waiting for host response".into());
            }
            let bytes = {
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
            };
            if frame_kind(&bytes) != "response" {
                // 宿主并发推送的 worker 帧：入 pending 一次，继续等响应（不再重弹）
                self.pending.lock().unwrap().push_back(bytes);
                continue;
            }
            // 响应帧但 requestId 不匹配 = 请求方已超时放弃的迟到响应：丢弃，保证前进
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|e| format!("bad response json: {e}"))?;
            match envelope::validate_host_response(&value, request_id) {
                Ok((true, result, _)) => return Ok(Some(result)),
                Ok((false, _, err)) => {
                    return Err(err.unwrap_or_else(|| "host request failed".into()))
                }
                Err(_) => {
                    eprintln!("[host] dropped stale host-response (late arrival)");
                    continue;
                }
            }
        }
    }
}

/// 帧类型判定（坏帧返回空串，由各路径丢弃处理）。
fn frame_kind(bytes: &[u8]) -> String {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| v.get("kind").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

fn validate_response_bytes(bytes: &[u8], request_id: &str) -> Result<Option<Value>, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("bad response json: {e}"))?;
    match envelope::validate_host_response(&value, request_id) {
        Ok((true, result, _)) => Ok(Some(result)),
        Ok((false, _, err)) => Err(err.unwrap_or_else(|| "host request failed".into())),
        Err(e) => Err(format!("response validation: {e}")),
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
            let result = host_call(&method, params)
                .map_err(|e| rquickjs::Error::new_from_js_message("call", "call", e.to_string()))?;
            serde_json::to_string(&result).map_err(|e| {
                rquickjs::Error::new_from_js_message("String", "String", e.to_string())
            })
        },
    )?;
    ctx.globals().set("__hostRequest", f)?;
    Ok(())
}

/// buildContext 的 JS 引导（对等 PluginProcessEntry.buildContext 全量方法面）。
/// 所有宿主能力经 __hostRequest 同步转发；fire-and-forget 方法（logger/notify/
/// shortcut.register/event.emit）忽略响应。
/// v1.9.15：新增 clipboard / getSystemInfo
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
  api.clipboard = {
    read: function () { return __rpc('clipboard.read', {}); },
    write: function (text) { return __rpc('clipboard.write', { text: text }); }
  };
  api.getSystemInfo = function () { return __rpc('system.info', {}); };
  api.registerShortcut = function (keys, handler) {
    __ff('shortcut.register', { keys: keys });
    return function () { __ff('shortcut.unregister', { keys: keys }); };
  };
  api.emitEvent = function (event, data) { __ff('event.emit', { event: event, data: data }); };
  api.onEvent = function (event, handler) {
    if (typeof handler !== 'function') { throw new Error('onEvent requires a handler function'); }
    var subId = 'sub-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
    if (!globalThis.__cbSubscribers) { globalThis.__cbSubscribers = {}; }
    if (!globalThis.__cbSubscribers[event]) { globalThis.__cbSubscribers[event] = {}; }
    globalThis.__cbSubscribers[event][subId] = handler;
    __ff('event.subscribe', { event: event, subscriptionId: subId });
    return function () {
      if (globalThis.__cbSubscribers && globalThis.__cbSubscribers[event]) {
        delete globalThis.__cbSubscribers[event][subId];
      }
      __ff('event.unsubscribe', { event: event, subscriptionId: subId });
    };
  };
  api.dispatchHostEvent = function (event, data) {
    var handlers = globalThis.__cbSubscribers && globalThis.__cbSubscribers[event];
    if (!handlers) { return; }
    Object.keys(handlers).forEach(function (subId) {
      try { handlers[subId](data); } catch (e) { /* handler 异常不中断其余 */ }
    });
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
    use serde_json::json;

    #[test]
    fn ctx_js_is_valid_syntax() {
        let rt = rquickjs::Runtime::new().unwrap();
        let ctx = rquickjs::Context::full(&rt).unwrap();
        ctx.with(|ctx| {
            ctx.eval::<(), _>(CTX_JS).unwrap();
            let built: rquickjs::Value = ctx.eval("__buildCtx('p1', {a:1})").unwrap();
            let obj = built.as_object().unwrap();
            assert_eq!(obj.get::<_, String>("id").unwrap(), "p1");
            let has_storage = !obj
                .get::<_, rquickjs::Value>("storage")
                .unwrap()
                .is_undefined();
            assert!(has_storage);
        });
    }

    fn frame(v: Value) -> Vec<u8> {
        serde_json::to_vec(&v).unwrap()
    }
    fn worker_frame() -> Vec<u8> {
        frame(json!({
            "v": 2, "kind": "request", "requestId": "host-1", "token": "t",
            "method": "plugin.message", "params": {}
        }))
    }
    fn response_frame(rid: &str) -> Vec<u8> {
        frame(json!({
            "v": 2, "kind": "response", "requestId": rid, "ok": true, "result": 42,
            "token": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }))
    }

    /// Bug B/C 根因回归：__hostRequest 等待响应期间并发到达的 worker 帧
    /// 曾被反复弹出比对导致自旋死循环。修复后必须立即返回响应，
    /// worker 帧保留给主循环。
    #[test]
    fn concurrent_worker_frame_does_not_starve_response_wait() {
        let (tx, rx) = mpsc::channel();
        let q = FrameQueue {
            rx: Mutex::new(rx),
            pending: Mutex::new(VecDeque::new()),
        };
        tx.send(Some(worker_frame())).unwrap();
        tx.send(Some(response_frame("r1"))).unwrap();

        let start = std::time::Instant::now();
        let got = q.take_matching_response("r1").unwrap().unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "take_matching_response must not spin on pending worker frames"
        );
        assert_eq!(got, json!(42));

        // worker 帧必须保留给主循环（不丢失）
        let w = q.take_worker_frame().unwrap();
        assert_eq!(frame_kind(&w), "request");
    }

    /// pending 中已有迟到响应时，只消费响应帧；worker 帧不被误吞。
    #[test]
    fn pending_mixed_frames_are_dispatched_by_kind() {
        let (tx, rx) = mpsc::channel();
        let q = FrameQueue {
            rx: Mutex::new(rx),
            pending: Mutex::new(VecDeque::from(vec![worker_frame(), response_frame("r9")])),
        };
        let got = q.take_matching_response("r9").unwrap().unwrap();
        assert_eq!(got, json!(42));
        let w = q.take_worker_frame().unwrap();
        assert_eq!(frame_kind(&w), "request");
        // 队列清空后 take_worker_frame 无帧可取（channel 断开 → None）
        drop(tx);
        assert!(q.take_worker_frame().is_none());
    }

    /// 迟到（requestId 不匹配）的响应帧被丢弃而非回存 pending 自旋。
    #[test]
    fn late_mismatched_response_is_dropped() {
        let (tx, rx) = mpsc::channel();
        let q = FrameQueue {
            rx: Mutex::new(rx),
            pending: Mutex::new(VecDeque::new()),
        };
        tx.send(Some(response_frame("r-old"))).unwrap();
        tx.send(Some(response_frame("r1"))).unwrap();
        let start = std::time::Instant::now();
        let got = q.take_matching_response("r1").unwrap().unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(got, json!(42));
    }
}

// 插件 backend sidecar 宿主侧管理器（1.9.2-a，对等 PluginSandbox + PluginManager 运行时）
// 架构（ora-10）：
// - 每插件一进程 + 长期存活 + 惰性 spawn（首次 plugin_send_message 命中时）
// - 每进程一个阻塞读线程（ChildStdout）+ pending map（requestId → oneshot）
// - stdin 互斥写（Mutex<ChildStdin>）；host 请求分发到独立工作线程（防死锁）
// - worker 请求 30s 超时强杀；崩溃 EOF 检测 + backoff + 5 分钟 3 次隔离
// - 退出时 kill 全部存活进程

use crate::db::Db;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
#[allow(dead_code)] // 崩溃恢复（1.9.2-a 步骤 6 接入后消除）
const BACKOFFS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(30),
];
#[allow(dead_code)]
const CRASH_WINDOW: Duration = Duration::from_secs(5 * 60);
#[allow(dead_code)]
const MAX_CRASHES: u32 = 3;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 事件发射回调类型（event, payload）——main.rs setup 注入 app.emit
type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

pub struct BackendProcess {
    plugin_id: String,
    permissions: crate::permissions::PermissionGuard,
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Mutex<HashMap<String, std::sync::mpsc::Sender<Result<Value, String>>>>,
    next_request_id: AtomicU64,
    /// spawn 时注入 sidecar 的 token（响应帧必须回显；sidecar validate_host_response 要求）
    token: String,
    db: Arc<Mutex<Db>>,
    /// dispose 显式触发时为 true（读线程 EOF 走正常退出，不触发崩溃恢复）
    expected_stop: std::sync::atomic::AtomicBool,
    /// 崩溃上报回调（Manager 注入；读线程 EOF 且非 expected_stop 时调用）
    on_crash: Arc<dyn Fn(&str) + Send + Sync>,
    /// 事件发射回调（Manager 注入；plugin:log / plugin:message 经它广播）
    emitter: Emitter,
}

impl BackendProcess {
    #[allow(clippy::too_many_arguments)]
    fn spawn(
        plugin_id: String,
        permissions: crate::permissions::PermissionGuard,
        db: Arc<Mutex<Db>>,
        sidecar_exe: PathBuf,
        plugin_dir: PathBuf,
        entry_main: String,
        on_crash: Arc<dyn Fn(&str) + Send + Sync>,
        emitter: Emitter,
    ) -> Result<Arc<BackendProcess>, String> {
        // token：32 位随机 [A-Za-z0-9_-]
        let token = crate::rand_token::random_token_alnum(32)?;
        let mut cmd = Command::new(&sidecar_exe);
        cmd.args([
            plugin_dir.to_string_lossy().into_owned(),
            entry_main.clone(),
            "2".into(),
            token.clone(),
        ])
        .current_dir(&plugin_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        // 最小环境（对等 createPluginWorkerEnvironment）：仅 SystemRoot/临时目录
        cmd.env_clear();
        for key in ["SystemRoot", "WINDIR", "TEMP", "TMP", "TZ"] {
            if let Some(v) = std::env::var_os(key) {
                cmd.env(key, v);
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn sidecar failed: {e}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "sidecar stdin not piped".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "sidecar stdout not piped".to_string())?;

        let process = Arc::new(BackendProcess {
            plugin_id: plugin_id.clone(),
            permissions,
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            next_request_id: AtomicU64::new(1),
            token,
            db,
            expected_stop: std::sync::atomic::AtomicBool::new(false),
            on_crash,
            emitter,
        });

        // 读线程：独占 ChildStdout
        let reader = process.clone();
        std::thread::Builder::new()
            .name(format!("sidecar-reader-{plugin_id}"))
            .spawn(move || reader.read_loop(stdout))
            .map_err(|e| format!("spawn reader thread failed: {e}"))?;
        Ok(process)
    }

    fn read_loop(self: Arc<Self>, mut stdout: ChildStdout) {
        loop {
            let event = match read_frame(&mut stdout) {
                Ok(Some(bytes)) => match serde_json::from_slice::<Value>(&bytes) {
                    Ok(v) => v,
                    Err(_) => continue, // 坏帧忽略
                },
                Ok(None) | Err(_) => {
                    // EOF：dispose 路径已设 expected_stop（正常退出）；否则视为崩溃
                    self.finish_pending_eof();
                    let expected = self.expected_stop.load(Ordering::SeqCst);
                    if !expected {
                        let plugin = self.plugin_id.clone();
                        (self.on_crash)(&plugin);
                    }
                    return;
                }
            };
            let kind = event.get("kind").and_then(Value::as_str).unwrap_or("");
            match kind {
                "response" => {
                    let rid = event
                        .get("requestId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let ok = event.get("ok").and_then(Value::as_bool).unwrap_or(false);
                    let result = if ok {
                        Ok(event.get("result").cloned().unwrap_or(Value::Null))
                    } else {
                        Err(event
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("sidecar error")
                            .to_string())
                    };
                    // plugin:message：sidecar 响应帧即插件发往宿主的消息（对等
                    // PluginManager sandbox.on('message') → plugin:message 广播）
                    let message = result.clone().unwrap_or_else(|e| json!(e));
                    (self.emitter)(
                        "plugin:message",
                        json!({ "pluginId": self.plugin_id, "message": message }),
                    );
                    if let Some(tx) = self.pending.lock().unwrap().remove(&rid) {
                        let _ = tx.send(result);
                    }
                }
                "request" => {
                    let request_id = event
                        .get("requestId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let method = event
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let params = event.get("params").cloned().unwrap_or(Value::Null);
                    // host 请求分发到独立工作线程（防死锁：不能阻塞读线程）
                    let owner = self.clone();
                    std::thread::Builder::new()
                        .name(format!("host-method-{method}"))
                        .spawn(move || owner.handle_host_request(request_id, method, params))
                        .ok();
                }
                _ => { /* 忽略未知 kind */ }
            }
        }
    }

    fn finish_pending_eof(&self) {
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for (_, tx) in pending {
            let _ = tx.send(Err("sidecar exited".into()));
        }
    }

    /// 处理 host 方法：权限校验 → 实现分发 → 响应写回 stdin
    fn handle_host_request(&self, request_id: String, method: String, params: Value) {
        // 1) 实现面检查
        if !crate::permissions::is_host_method_implemented(&method) {
            let _ = self.send_host_response(&request_id, Err("NOT_ALLOWED".into()));
            return;
        }
        // 2) 权限校验（宿主权威边界）
        if let Some(perm) = crate::permissions::permission_for_host_method(&method) {
            if let Err(e) = self.permissions.assert(perm) {
                let _ = self.send_host_response(&request_id, Err(e));
                return;
            }
        }
        // 3) 执行
        let result = crate::envelope_host::host_dispatch(
            &self.db,
            &self.plugin_id,
            &method,
            &params,
            &*self.emitter,
        );
        let _ = self.send_host_response(&request_id, result);
    }

    /// 响应写回 stdin（与 worker 请求共用 stdin 锁，帧原子写）
    /// 信封：{v:2, kind:'response', token, requestId, ok, result|error}
    /// token 必须回显 spawn 时注入值（sidecar validate_host_response 校验）。
    fn send_host_response(
        &self,
        request_id: &str,
        result: Result<Value, String>,
    ) -> std::io::Result<()> {
        let payload = match result {
            Ok(v) => json!({
                "v": 2, "kind": "response", "token": self.token,
                "requestId": request_id, "ok": true, "result": v,
            }),
            Err(msg) => json!({
                "v": 2, "kind": "response", "token": self.token,
                "requestId": request_id, "ok": false,
                "error": { "code": "NOT_ALLOWED", "message": msg },
            }),
        };
        let bytes = serde_json::to_vec(&payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let mut stdin = self.stdin.lock().unwrap();
        write_frame(&mut *stdin, &bytes)
    }

    /// 发送 worker 请求（initialize/dispose/plugin.message）并等待匹配响应（30s 超时）
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let request_id = format!(
            "{}-{}",
            self.next_request_id.fetch_add(1, Ordering::SeqCst),
            crate::rand_token::random_token_alnum(8)?
        );
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending.lock().unwrap().insert(request_id.clone(), tx);
        let payload = json!({
            "v": 2, "kind": "request", "requestId": request_id,
            "token": self.token, "method": method, "params": params,
        });
        let bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
        {
            let mut stdin = self.stdin.lock().unwrap();
            write_frame(&mut *stdin, &bytes).map_err(|e| e.to_string())?;
        }
        match rx.recv_timeout(REQUEST_TIMEOUT) {
            Ok(result) => result,
            Err(_) => {
                // 超时：清理 pending + 强杀（对等 requestWorker timeout → terminateChild）
                self.pending.lock().unwrap().remove(&request_id);
                self.kill_now("request timeout");
                Err("sidecar request timed out".into())
            }
        }
    }

    /// 优雅 dispose：lifecycle.dispose → 3s grace → kill
    /// 设置 expected_stop（读线程 EOF 不再触发崩溃恢复）。
    pub fn dispose(&self) {
        self.expected_stop.store(true, Ordering::SeqCst);
        let _ = self.request("lifecycle.dispose", json!({}));
        // 等待退出（grace 3s）
        let mut child = self.child.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }
    }

    fn kill_now(&self, reason: &str) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        eprintln!("[backend] {reason}: killed {}", self.plugin_id);
    }
}

/// 从 reader 读一帧（复用 sidecar frame.rs 语义，宿主侧内联实现避免跨 crate 依赖）
fn read_frame<R: Read>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        let n = reader.read(&mut len_buf[filled..])?;
        if n == 0 {
            if filled == 0 {
                return Ok(None);
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "truncated frame",
            ));
        }
        filled += n;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 8 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        let n = reader.read(&mut payload[filled..])?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "truncated frame",
            ));
        }
        filled += n;
    }
    Ok(Some(payload))
}

/// 写一帧
pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> std::io::Result<()> {
    if payload.len() > 8 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "payload too large",
        ));
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

// ---------------------------------------------------------------------------
// Manager：进程注册表 + 惰性 spawn + 生命周期
// ---------------------------------------------------------------------------

pub struct BackendProcessManager {
    processes: Mutex<HashMap<String, Arc<BackendProcess>>>,
    /// 崩溃历史（plugin_id → 最近崩溃时间戳，跨进程/跨重启计数）
    crash_history: Mutex<HashMap<String, Vec<Instant>>>,
    db: Arc<Mutex<Db>>,
    sidecar_exe: PathBuf,
    /// 事件发射回调（main.rs setup 注入 app.emit；未注入时 no-op）
    emitter: Mutex<Option<Emitter>>,
    /// 自引用（Weak，避免循环）：崩溃回调与重启线程经它访问 Manager
    self_weak: std::sync::OnceLock<std::sync::Weak<BackendProcessManager>>,
}

impl BackendProcessManager {
    pub fn new(db: Arc<Mutex<Db>>) -> Arc<Self> {
        // sidecar exe：dev 态 target/debug/；打包态 tauri externalBin（1.9.2-e 落）
        let sidecar_name = "cruciblebox-plugin-host-x86_64-pc-windows-msvc.exe";
        let sidecar_exe = std::env::current_dir()
            .ok()
            .map(|d| {
                let exe_dir = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(PathBuf::from));
                let cands = [
                    d.join("target/debug/cruciblebox-plugin-host.exe"),
                    d.join("cruciblebox-plugin-host/target/debug/cruciblebox-plugin-host.exe"),
                    d.parent()
                        .map(|p| p.join("target/debug/cruciblebox-plugin-host.exe"))
                        .unwrap_or_default(),
                    exe_dir.clone().unwrap_or_default().join(sidecar_name),
                    exe_dir
                        .unwrap_or_default()
                        .join("resources")
                        .join(sidecar_name),
                ];
                cands.into_iter().find(|p| p.is_file()).unwrap_or_default()
            })
            .unwrap_or_default();
        let mgr = Arc::new(BackendProcessManager {
            processes: Mutex::new(HashMap::new()),
            crash_history: Mutex::new(HashMap::new()),
            db,
            sidecar_exe,
            emitter: Mutex::new(None),
            self_weak: std::sync::OnceLock::new(),
        });
        let _ = mgr.self_weak.set(Arc::downgrade(&mgr));
        mgr
    }

    /// 注入事件发射回调（main.rs setup 调用；app.emit 广播到所有窗口）。
    pub fn set_emitter(&self, emitter: Emitter) {
        *lock(&self.emitter) = Some(emitter);
    }

    /// 事件发射（未注入时 no-op）。
    pub fn emit(&self, event: &str, payload: serde_json::Value) {
        let guard = lock(&self.emitter);
        if let Some(emitter) = guard.as_ref() {
            emitter(event, payload);
        }
    }

    /// 当前发射回调（未注入时 no-op），供 spawn 时注入 BackendProcess。
    fn emitter(&self) -> Emitter {
        lock(&self.emitter)
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Arc::new(|_, _| {}))
    }

    /// 惰性 spawn：返回已激活进程（若不存在则创建）
    /// 注入 on_crash 回调：读线程 EOF（非 dispose）时上报，触发崩溃恢复策略。
    pub fn ensure_activated(
        &self,
        plugin_id: &str,
        record: crate::db::PluginBackendRecord,
    ) -> Result<Arc<BackendProcess>, String> {
        if let Some(p) = self.processes.lock().unwrap().get(plugin_id) {
            return Ok(p.clone());
        }
        if !record.enabled {
            return Err("plugin is disabled".into());
        }
        let guard = crate::permissions::PermissionGuard::from_json(&record.permissions);
        let proc = BackendProcess::spawn(
            plugin_id.to_string(),
            guard,
            self.db.clone(),
            self.sidecar_exe.clone(),
            PathBuf::from(&record.installed_path),
            record.entry_main,
            self.crash_callback(),
            self.emitter(),
        )?;
        // initialize
        proc.request(
            "lifecycle.initialize",
            json!({ "pluginId": plugin_id, "config": {} }),
        )?;
        self.processes
            .lock()
            .unwrap()
            .insert(plugin_id.to_string(), proc.clone());
        Ok(proc)
    }

    /// 崩溃恢复策略：记录崩溃 → backoff 重启或隔离。
    /// 返回 self 的崩溃回调（Arc<dyn Fn(&str)>），读线程 EOF 非 dispose 时触发。
    fn crash_callback(&self) -> Arc<dyn Fn(&str) + Send + Sync> {
        let weak = self
            .self_weak
            .get()
            .expect("BackendProcessManager::new must set self_weak")
            .clone();
        Arc::new(move |pid| {
            if let Some(mgr) = weak.upgrade() {
                mgr.handle_crash(pid);
            }
        })
    }

    /// 崩溃处理：记录 → 判断隔离或 backoff 重启。
    fn handle_crash(&self, plugin_id: &str) {
        // 从进程表移除（读线程已死，进程对象应清理）
        let exited = self.processes.lock().unwrap().remove(plugin_id);
        // 记录崩溃时间
        let now = Instant::now();
        let mut history = self.crash_history.lock().unwrap();
        let entries = history.entry(plugin_id.to_string()).or_default();
        entries.retain(|t| now.duration_since(*t) < CRASH_WINDOW);
        entries.push(now);
        let count = entries.len();
        drop(history);

        if count >= MAX_CRASHES as usize {
            // 5 分钟内 3 次崩溃 → 隔离（持久化 enabled=false）
            eprintln!("[backend] {plugin_id} isolated after {count} crashes in 5m");
            let _ = self.db.lock().unwrap().set_plugin_enabled(plugin_id, false);
            return;
        }

        // backoff 重启（1s/5s/30s）
        let backoff = BACKOFFS[(count - 1).min(BACKOFFS.len() - 1)];
        eprintln!("[backend] {plugin_id} crashed (attempt {count}), restart in {backoff:?}");
        let weak = self
            .self_weak
            .get()
            .expect("BackendProcessManager::new must set self_weak")
            .clone();
        let db = self.db.clone();
        let pid = plugin_id.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(backoff);
            // 重启：重新读 DB 记录并 ensure_activated
            if let Ok(Some(record)) = db.lock().unwrap().plugin_backend_record(&pid) {
                if let Some(mgr) = weak.upgrade() {
                    if let Err(e) = mgr.ensure_activated(&pid, record) {
                        eprintln!("[backend] restart {pid} failed: {e}");
                    }
                }
            }
        });
        let _ = exited;
    }

    #[allow(dead_code)] // 1.9.2-b 插件写路径（deactivate 命令）接入
    pub fn deactivate(&self, plugin_id: &str) {
        if let Some(p) = self.processes.lock().unwrap().remove(plugin_id) {
            p.dispose();
        }
    }

    pub fn kill_all(&self) {
        let procs: Vec<Arc<BackendProcess>> = self
            .processes
            .lock()
            .unwrap()
            .drain()
            .map(|(_, p)| p)
            .collect();
        for p in procs {
            p.dispose();
        }
    }

    #[allow(dead_code)] // 1.9.2-b 插件写路径（deactivate 命令）接入
    pub fn has(&self, plugin_id: &str) -> bool {
        self.processes.lock().unwrap().contains_key(plugin_id)
    }
}

/// 锁辅助（零 unwrap 约束：poisoned 时取内部值继续）
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实 e2e：spawn 宿主侧 sidecar → 真实 gif-editor dist → initialize/message/dispose
    /// 需要 sidecar exe 已构建（cargo build --manifest-path cruciblebox-plugin-host/Cargo.toml）
    /// 与 gif-editor dist 产物（npm run build:plugins）。
    fn sidecar_exe() -> PathBuf {
        // 测试 cwd = src-tauri/；sidecar exe 在 target/debug/
        std::env::current_dir()
            .ok()
            .map(|d| {
                d.join("target")
                    .join("debug")
                    .join("cruciblebox-plugin-host.exe")
            })
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from("target/debug/cruciblebox-plugin-host.exe"))
    }

    fn temp_db() -> Arc<Mutex<Db>> {
        let dir = std::env::temp_dir().join(format!("cb-backend-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openbox.db");
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).unwrap();
        // FK 外键（foreign_keys=ON）：log.write/storage 需 plugins 行存在
        db.conn()
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path)
                 VALUES ('gif-editor', 'gif-editor', '1.0.0', 'GIF Editor', 'dist/main.js', ?1)
                 ON CONFLICT(id) DO NOTHING",
                rusqlite::params![std::env::current_dir()
                    .ok()
                    .and_then(|d| d.parent().map(|p| p.join("plugins/gif-editor")))
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()],
            )
            .unwrap();
        Arc::new(Mutex::new(db))
    }

    fn gif_plugin_dir() -> PathBuf {
        // 测试 cwd = src-tauri/；插件在 ../plugins/gif-editor
        std::env::current_dir()
            .ok()
            .and_then(|d| d.parent().map(|p| p.join("plugins").join("gif-editor")))
            .filter(|p| p.join("dist").join("main.js").exists())
            .unwrap_or_else(|| PathBuf::from("../plugins/gif-editor"))
    }

    #[test]
    fn e2e_initialize_message_dispose() {
        let exe = sidecar_exe();
        let plugin_dir = gif_plugin_dir();
        if !exe.exists() {
            eprintln!("skipping e2e: sidecar exe not built ({})", exe.display());
            return;
        }
        if !plugin_dir.join("dist/main.js").exists() {
            eprintln!("skipping e2e: gif-editor dist not built");
            return;
        }

        let db = temp_db();
        let proc = BackendProcess::spawn(
            "gif-editor".into(),
            crate::permissions::PermissionGuard::parse(&["notification".to_string()]),
            db.clone(),
            exe,
            plugin_dir.clone(),
            "dist/main.js".into(),
            Arc::new(|_pid: &str| {}), // 测试：崩溃回调 no-op
            Arc::new(|_event: &str, _payload: serde_json::Value| {}), // 测试：发射回调 no-op
        )
        .expect("spawn sidecar");

        // initialize（gif-editor activate 会写日志 → host log.write → DB）
        let init = proc.request(
            "lifecycle.initialize",
            json!({"pluginId": "gif-editor", "config": {}}),
        );
        assert!(init.is_ok(), "initialize failed: {:?}", init.err());

        // plugin.message(ping) → 期望 {"ok":true}
        let msg = proc
            .request("plugin.message", json!({"message": {"type": "ping"}}))
            .expect("message failed");
        assert_eq!(msg, json!({"ok": true}), "unexpected onMessage reply");

        // 验证 host log.write 已入库（gif-editor activate 记录日志）
        let log_count: i64 = {
            let db_guard = db.lock().unwrap();
            let conn_guard = db_guard.conn().lock().unwrap();
            conn_guard
                .query_row(
                    "SELECT COUNT(*) FROM plugin_logs WHERE plugin_id = 'gif-editor'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert!(
            log_count >= 1,
            "expected activate log in plugin_logs, got {log_count}"
        );

        // dispose
        let _ = proc.request("lifecycle.dispose", json!({}));
        proc.kill_now("test cleanup");
    }

    #[test]
    fn e2e_diary_backend_initialize_and_message() {
        let exe = sidecar_exe();
        let plugin_dir = std::env::current_dir()
            .ok()
            .and_then(|d| d.parent().map(|p| p.join("plugins").join("diary")))
            .filter(|p| p.join("dist").join("main.js").exists())
            .unwrap_or_else(|| PathBuf::from("../plugins/diary"));
        if !exe.exists() || !plugin_dir.join("dist/main.js").exists() {
            eprintln!("skipping diary e2e: sidecar exe or diary dist not built");
            return;
        }

        let dir = std::env::temp_dir().join(format!("cb-diary-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openbox.db");
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).unwrap();
        db.conn()
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path)
                 VALUES ('diary', 'diary', '1.0.0', 'Diary', 'dist/main.js', ?1)
                 ON CONFLICT(id) DO NOTHING",
                rusqlite::params![plugin_dir.to_string_lossy().into_owned()],
            )
            .unwrap();
        let db = Arc::new(Mutex::new(db));

        let proc = BackendProcess::spawn(
            "diary".into(),
            crate::permissions::PermissionGuard::parse(&[
                "storage:read".to_string(),
                "storage:write".to_string(),
            ]),
            db.clone(),
            exe,
            plugin_dir.clone(),
            "dist/main.js".into(),
            Arc::new(|_pid: &str| {}),
            Arc::new(|_event: &str, _payload: serde_json::Value| {}),
        )
        .expect("spawn diary sidecar");

        let init = proc.request(
            "lifecycle.initialize",
            json!({"pluginId": "diary", "config": {}}),
        );
        assert!(init.is_ok(), "diary initialize failed: {:?}", init.err());

        // getMonthEntries → storage.list（storage:read 权限）→ 应返回 [] 而非错误
        let msg = proc
            .request(
                "plugin.message",
                json!({"message": {"type": "getMonthEntries", "year": 2026, "month": 8}}),
            )
            .expect("diary getMonthEntries request failed");
        let entries = msg.get("entries").cloned().unwrap_or_else(|| json!([]));
        assert!(
            entries.is_array(),
            "getMonthEntries did not return entries array: {msg}"
        );

        let _ = proc.request("lifecycle.dispose", json!({}));
        proc.kill_now("test cleanup");
    }
}

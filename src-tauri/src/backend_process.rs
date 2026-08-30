// 插件 backend sidecar 宿主侧管理器（1.9.2-a，对等 PluginSandbox + PluginManager 运行时）
// 架构（ora-10）：
// - 每插件一进程 + 长期存活 + 惰性 spawn（首次 plugin_send_message 命中时）
// - 每进程一个阻塞读线程（ChildStdout）+ pending map（requestId → oneshot）
// - stdin 互斥写（Mutex<ChildStdin>）；host 请求分发到独立工作线程（防死锁）
// - worker 请求 30s 超时强杀；崩溃 EOF 检测 + backoff + 5 分钟 3 次隔离
// - 退出时 kill 全部存活进程

use crate::db::Db;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DISPOSE_TIMEOUT: Duration = Duration::from_secs(3);
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
                    // EOF：dispose 路径已设 expected_stop（正常退出）；否则视为崩溃。
                    // 退出码诊断：0=主循环正常 break；其他=异常终止（Bug B 排障线）。
                    let status = self
                        .child
                        .lock()
                        .unwrap()
                        .try_wait()
                        .ok()
                        .flatten()
                        .map(|s| format!("{s:?}"))
                        .unwrap_or_else(|| "no-status".into());
                    eprintln!(
                        "[backend] {} pipe closed (expected={}): {status}",
                        self.plugin_id,
                        self.expected_stop.load(Ordering::SeqCst)
                    );
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
        eprintln!(
            "[host-method] enter {} rid={} op={}",
            method,
            request_id,
            params
                .get("payload")
                .and_then(|p| p.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("-")
        );
        // 1) 实现面检查
        if !crate::permissions::is_host_method_implemented(&method) {
            let _ = self.send_host_response(&request_id, Err("NOT_ALLOWED".into()));
            return;
        }
        // 2) 权限校验（宿主权威边界）
        if method == "trusted.invoke" {
            // 宿主固定可信服务（unienv / document-engine 等）共用 trusted.invoke，
            // 接受任一 trusted:* 权限（见 PermissionGuard::assert_trusted_service）。
            if let Err(e) = self.permissions.assert_trusted_service() {
                let _ = self.send_host_response(&request_id, Err(e));
                return;
            }
        } else if let Some(perm) = crate::permissions::permission_for_host_method(&method) {
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
        eprintln!(
            "[host-method] done {method} rid={request_id} ok={}",
            result.is_ok()
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
        self.request_with_timeout(method, params, REQUEST_TIMEOUT)
    }

    fn request_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
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
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => {
                // 超时：清理 pending + 强杀（对等 requestWorker timeout → terminateChild）
                self.pending.lock().unwrap().remove(&request_id);
                self.kill_now("request timeout");
                Err("sidecar request timed out".into())
            }
        }
    }

    /// 优雅 dispose：lifecycle.dispose → 3s grace → kill + wait。
    /// 设置 expected_stop（读线程 EOF 不再触发崩溃恢复）。
    pub fn dispose(&self) -> Result<(), String> {
        self.expected_stop.store(true, Ordering::SeqCst);
        // 1.9.17：停止剪贴板监控线程（如有）
        crate::clipboard_monitor::stop(&self.plugin_id);
        let graceful = self.request_with_timeout("lifecycle.dispose", json!({}), DISPOSE_TIMEOUT);
        if let Err(err) = &graceful {
            eprintln!(
                "[backend] graceful dispose {} failed: {err}; forcing exit",
                self.plugin_id
            );
        }
        // 等待退出（grace 3s）
        let mut child = self.child.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut wait_error = None;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        if let Err(e) = child.kill() {
                            wait_error = Some(format!("failed to kill sidecar: {e}"));
                        }
                        if wait_error.is_none() {
                            if let Err(e) = child.wait() {
                                wait_error = Some(format!("failed to wait sidecar: {e}"));
                            }
                        }
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    wait_error = Some(format!("failed to poll sidecar: {e}"));
                    break;
                }
            }
        }
        drop(child);
        if let Some(error) = wait_error {
            return Err(error);
        }
        Ok(())
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
    /// 插件维护期间禁止激活及崩溃自动重启，避免卸载/升级与恢复线程竞态。
    maintenance: Mutex<HashSet<String>>,
    /// 每个插件的生命周期操作单飞锁。导入、升级、启停、卸载和恢复不得并行。
    lifecycle: Mutex<HashSet<String>>,
    /// 激活单飞锁。只保护同一插件的 spawn/initialize，绝不跨插件串行化。
    activating: Mutex<HashSet<String>>,
    db: Arc<Mutex<Db>>,
    sidecar_exe: PathBuf,
    /// 事件发射回调（main.rs setup 注入 app.emit；未注入时 no-op）
    emitter: Mutex<Option<Emitter>>,
    /// 自引用（Weak，避免循环）：崩溃回调与重启线程经它访问 Manager
    self_weak: std::sync::OnceLock<std::sync::Weak<BackendProcessManager>>,
}

impl BackendProcessManager {
    pub fn new(db: Arc<Mutex<Db>>) -> Arc<Self> {
        let sidecar_exe = Self::resolve_sidecar_exe(
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from)),
            std::env::current_dir().ok(),
        );
        if !sidecar_exe.is_file() {
            // Bug B/C 根因诊断线：打包态解析失败时此前静默落到空路径，
            // 首条消息只会看到 "spawn sidecar failed: program path has no file name"。
            eprintln!(
                "[backend] sidecar exe not found; resolved={}",
                sidecar_exe.display()
            );
        }
        let mgr = Arc::new(BackendProcessManager {
            processes: Mutex::new(HashMap::new()),
            crash_history: Mutex::new(HashMap::new()),
            maintenance: Mutex::new(HashSet::new()),
            lifecycle: Mutex::new(HashSet::new()),
            activating: Mutex::new(HashSet::new()),
            db,
            sidecar_exe,
            emitter: Mutex::new(None),
            self_weak: std::sync::OnceLock::new(),
        });
        let _ = mgr.self_weak.set(Arc::downgrade(&mgr));
        mgr
    }

    /// 解析插件 backend sidecar 路径。dev 态在 cargo 工作目录下找 target/debug；
    /// 打包态 tauri externalBin 会把 `cruciblebox-plugin-host-<triple>.exe`
    /// 剥掉 triple 后放到安装根（NSIS 布局：exe 同级；resources 布局兜底）。
    /// 找不到返回空 PathBuf（spawn 报错前先有 eprintln 诊断）。
    fn resolve_sidecar_exe(exe_dir: Option<PathBuf>, cwd: Option<PathBuf>) -> PathBuf {
        const SIDECAR_TRIPLE: &str = "cruciblebox-plugin-host-x86_64-pc-windows-msvc.exe";
        const SIDECAR_PLAIN: &str = "cruciblebox-plugin-host.exe";
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(d) = &cwd {
            candidates.push(d.join("target/debug").join(SIDECAR_PLAIN));
            candidates.push(
                d.join("cruciblebox-plugin-host/target/debug")
                    .join(SIDECAR_PLAIN),
            );
            if let Some(parent) = d.parent() {
                candidates.push(parent.join("target/debug").join(SIDECAR_PLAIN));
                // cargo tauri dev 从 src-tauri 启动：cwd=src-tauri，release 态本地验证用
                candidates.push(parent.join("target/release").join(SIDECAR_PLAIN));
            }
        }
        if let Some(d) = &exe_dir {
            // 打包态主候选：externalBin 安装名（triple 已被 bundler 剥除）
            candidates.push(d.join(SIDECAR_PLAIN));
            candidates.push(d.join("resources").join(SIDECAR_PLAIN));
            // 兼容：手工 stage 未剥 triple 的布局
            candidates.push(d.join(SIDECAR_TRIPLE));
            candidates.push(d.join("resources").join(SIDECAR_TRIPLE));
        }
        candidates
            .into_iter()
            .find(|p| p.is_file())
            .unwrap_or_default()
    }

    /// 注入事件发射回调（main.rs setup 调用；app.emit 广播到所有窗口）。
    pub fn set_emitter(&self, emitter: Emitter) {
        *lock(&self.emitter) = Some(emitter);
    }

    /// 启动时恢复需要宿主侧监控的已启用插件（目前为 clipboard 权限插件）。
    /// 先复制 DB 记录再逐个激活，绝不把 DB 锁带入 sidecar 生命周期调用。
    pub fn activate_enabled_with_permission(&self, permission: &str) {
        let records = match self.db.lock().unwrap().enabled_plugin_backend_records() {
            Ok(records) => records,
            Err(error) => {
                eprintln!("[backend] failed to read enabled plugins at startup: {error}");
                return;
            }
        };
        for (plugin_id, record) in records {
            let guard = crate::permissions::PermissionGuard::from_json(&record.permissions);
            if !guard.has(permission) {
                continue;
            }
            if let Err(error) = self.ensure_activated(&plugin_id, record) {
                eprintln!("[backend] startup activation failed for {plugin_id}: {error}");
            }
        }
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
        // Do not hold the maintenance registry lock while spawning a process
        // or waiting for lifecycle.initialize.  A slow plugin must not block
        // maintenance checks for unrelated plugins.
        if self.maintenance.lock().unwrap().contains(plugin_id) {
            return Err("plugin is in maintenance".into());
        }
        if let Some(p) = self.processes.lock().unwrap().get(plugin_id) {
            return Ok(p.clone());
        }
        if !record.enabled {
            return Err("plugin is disabled".into());
        }
        {
            let mut activating = self.activating.lock().unwrap();
            if !activating.insert(plugin_id.to_string()) {
                return Err("plugin activation already in progress".into());
            }
        }
        let guard = crate::permissions::PermissionGuard::from_json(&record.permissions);
        let result = (|| {
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
            if let Err(error) = proc.request(
                "lifecycle.initialize",
                json!({ "pluginId": plugin_id, "config": {} }),
            ) {
                let _ = proc.dispose();
                return Err(error);
            }
            self.processes
                .lock()
                .unwrap()
                .insert(plugin_id.to_string(), proc.clone());
            // 1.9.17：clipboard 权限插件自动启动宿主侧剪贴板监控
            if proc.permissions.has(crate::permissions::CLIPBOARD) {
                let _ = crate::clipboard_monitor::start(plugin_id, self.emitter());
            }
            Ok(proc)
        })();
        self.activating.lock().unwrap().remove(plugin_id);
        result
    }

    /// 标记插件进入维护窗口。调用方必须在完成操作后调用 end_maintenance。
    pub fn begin_maintenance(&self, plugin_id: &str) -> Result<(), String> {
        let mut maintenance = self.maintenance.lock().unwrap();
        if !maintenance.insert(plugin_id.to_string()) {
            return Err("plugin maintenance already in progress".into());
        }
        Ok(())
    }

    pub fn end_maintenance(&self, plugin_id: &str) {
        self.maintenance.lock().unwrap().remove(plugin_id);
    }

    /// 进入维护窗口并返回自动释放的 guard。
    pub fn enter_maintenance(&self, plugin_id: &str) -> Result<PluginMaintenanceGuard<'_>, String> {
        self.begin_maintenance(plugin_id)?;
        Ok(PluginMaintenanceGuard {
            manager: self,
            plugin_id: plugin_id.to_string(),
        })
    }

    /// 开始一个插件生命周期操作。返回的 guard 在所有返回路径上自动释放，
    /// 避免快速导入/卸载失败后留下永久 busy 状态。
    pub fn begin_lifecycle_operation(
        &self,
        plugin_ref: &str,
    ) -> Result<PluginLifecycleGuard<'_>, String> {
        // 旧版数据库允许 id 与 manifest name 不一致。生命周期调用方有时拿
        // id、有时只能拿 name；同时登记两个别名，避免升级/卸载与启停互相穿透。
        let keys = self.lifecycle_identity_keys(plugin_ref);
        let mut lifecycle = self.lifecycle.lock().unwrap();
        if keys.iter().any(|key| lifecycle.contains(key)) {
            return Err(format!(
                "{plugin_ref}: plugin lifecycle operation already in progress"
            ));
        }
        lifecycle.extend(keys.iter().cloned());
        Ok(PluginLifecycleGuard {
            manager: self,
            keys,
        })
    }

    fn lifecycle_identity_keys(&self, plugin_ref: &str) -> Vec<String> {
        let mut keys = vec![plugin_ref.to_string()];
        // Keep one Db guard for both lookups. Chaining two `lock(&self.db)` calls
        // in `or_else` can extend the first temporary guard until the whole
        // expression ends and deadlock on a name-based lookup.
        let db = lock(&self.db);
        let row = db
            .plugin_find_by_id(plugin_ref)
            .ok()
            .flatten()
            .or_else(|| db.plugin_find_by_name(plugin_ref).ok().flatten());
        drop(db);
        if let Some(row) = row {
            if !keys.iter().any(|key| key == &row.id) {
                keys.push(row.id);
            }
            if !keys.iter().any(|key| key == &row.name) {
                keys.push(row.name);
            }
        }
        keys
    }

    fn is_lifecycle_busy(&self, plugin_ref: &str) -> bool {
        let keys = self.lifecycle_identity_keys(plugin_ref);
        self.lifecycle
            .lock()
            .unwrap()
            .iter()
            .any(|key| keys.iter().any(|candidate| candidate == key))
    }

    fn is_in_maintenance(&self, plugin_id: &str) -> bool {
        self.maintenance.lock().unwrap().contains(plugin_id)
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
        if self.is_in_maintenance(plugin_id) || self.is_lifecycle_busy(plugin_id) {
            let _ = exited;
            return;
        }
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
            // 先把记录拷贝出来，确保调用 lifecycle busy 检查时没有持有 DB 锁。
            let record = db
                .lock()
                .unwrap()
                .plugin_backend_record(&pid)
                .ok()
                .flatten();
            if let Some(record) = record {
                if let Some(mgr) = weak.upgrade() {
                    if mgr.is_in_maintenance(&pid) || mgr.is_lifecycle_busy(&pid) {
                        return;
                    }
                    if let Err(e) = mgr.ensure_activated(&pid, record) {
                        eprintln!("[backend] restart {pid} failed: {e}");
                    }
                }
            }
        });
        let _ = exited;
    }

    #[allow(dead_code)] // 1.9.2-b 插件写路径（deactivate 命令）接入
    pub fn deactivate(&self, plugin_id: &str) -> Result<(), String> {
        if let Some(p) = self.processes.lock().unwrap().remove(plugin_id) {
            p.dispose()?;
        }
        Ok(())
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
            let _ = p.dispose();
        }
    }

    #[allow(dead_code)] // 1.9.2-b 插件写路径（deactivate 命令）接入
    pub fn has(&self, plugin_id: &str) -> bool {
        self.processes.lock().unwrap().contains_key(plugin_id)
    }
}

/// RAII 生命周期锁。必须在 `begin_lifecycle_operation` 返回后一直持有到
/// DB、目录、backend 和 renderer session 全部完成变更。
pub struct PluginLifecycleGuard<'a> {
    manager: &'a BackendProcessManager,
    keys: Vec<String>,
}

impl Drop for PluginLifecycleGuard<'_> {
    fn drop(&mut self) {
        let mut lifecycle = self.manager.lifecycle.lock().unwrap();
        for key in &self.keys {
            lifecycle.remove(key);
        }
    }
}

/// RAII 维护窗口 guard。升级、卸载和停用遇到错误时也会释放维护标记。
pub struct PluginMaintenanceGuard<'a> {
    manager: &'a BackendProcessManager,
    plugin_id: String,
}

impl Drop for PluginMaintenanceGuard<'_> {
    fn drop(&mut self) {
        self.manager.end_maintenance(&self.plugin_id);
    }
}

/// 锁辅助（零 unwrap 约束：poisoned 时取内部值继续）
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_DB_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
        let sequence = TEMP_DB_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("cb-backend-test-{}-{sequence}", std::process::id()));
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

    fn touch(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"mz").unwrap();
    }

    #[test]
    fn resolve_sidecar_prefers_packaged_external_bin_name() {
        // Bug B/C 根因回归：NSIS 打包态 externalBin 安装名不带 target triple，
        // 旧实现只找带 triple 的名字 → 打包态永远 spawn 失败。
        let root = std::env::temp_dir().join(format!("cb-sidecar-pkg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let exe_dir = root.join("app");
        let plain = exe_dir.join("cruciblebox-plugin-host.exe");
        let triple = exe_dir.join("cruciblebox-plugin-host-x86_64-pc-windows-msvc.exe");
        // 两种布局并存时优先剥 triple 的安装名
        touch(&plain);
        touch(&triple);
        let got = BackendProcessManager::resolve_sidecar_exe(Some(exe_dir.clone()), None);
        assert_eq!(got, plain);
        // 仅 resources 布局也能找到
        std::fs::remove_file(&plain).unwrap();
        let res = exe_dir
            .join("resources")
            .join("cruciblebox-plugin-host.exe");
        touch(&res);
        let got = BackendProcessManager::resolve_sidecar_exe(Some(exe_dir.clone()), None);
        assert_eq!(got, res);
        // 仅手工 stage（未剥 triple）布局兜底
        std::fs::remove_file(&res).unwrap();
        let got = BackendProcessManager::resolve_sidecar_exe(Some(exe_dir.clone()), None);
        assert_eq!(got, triple);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_sidecar_dev_layout_and_miss() {
        let root = std::env::temp_dir().join(format!("cb-sidecar-dev-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cwd = root.join("src-tauri");
        let dev = cwd
            .join("target")
            .join("debug")
            .join("cruciblebox-plugin-host.exe");
        touch(&dev);
        let got = BackendProcessManager::resolve_sidecar_exe(None, Some(cwd.clone()));
        assert_eq!(got, dev);
        // 全部落空 → 空 PathBuf（调用方 eprintln 诊断 + spawn 报错）
        let miss = BackendProcessManager::resolve_sidecar_exe(
            Some(root.join("nowhere")),
            Some(root.join("empty-cwd")),
        );
        assert!(!miss.is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lifecycle_operations_are_single_flight_and_release_on_drop() {
        let db = temp_db();
        let manager = BackendProcessManager::new(db);
        let first = manager
            .begin_lifecycle_operation("demo")
            .expect("first lifecycle operation should start");
        let error = match manager.begin_lifecycle_operation("demo") {
            Ok(_) => panic!("same plugin must be single-flight"),
            Err(error) => error,
        };
        assert!(error.contains("already in progress"));
        drop(first);
        manager
            .begin_lifecycle_operation("demo")
            .expect("guard drop must release lifecycle operation");
    }

    #[test]
    fn lifecycle_operation_covers_legacy_id_and_name_aliases() {
        let db = temp_db();
        lock(&db)
            .plugin_create(&crate::db::PluginRow {
                id: "legacy-id".into(),
                name: "demo".into(),
                version: "1.0.0".into(),
                display_name: "Demo".into(),
                description: String::new(),
                author: String::new(),
                icon: String::new(),
                entry_main: "dist/main.js".into(),
                entry_renderer: "dist/renderer.js".into(),
                permissions: "[]".into(),
                config_schema: "{}".into(),
                config_data: "{}".into(),
                enabled: false,
                installed_path: "C:/plugins/demo".into(),
                installed_at: String::new(),
                updated_at: String::new(),
                sort_order: 0,
            })
            .unwrap();
        let manager = BackendProcessManager::new(db);
        let guard = manager
            .begin_lifecycle_operation("demo")
            .expect("name should acquire lifecycle operation");
        let error = match manager.begin_lifecycle_operation("legacy-id") {
            Ok(_) => panic!("legacy id must share the same lifecycle operation"),
            Err(error) => error,
        };
        assert!(error.contains("already in progress"));
        drop(guard);
    }

    /// Bug B 复现：真实 sidecar + 真实 unienv 插件，重放 renderer 请求序列。
    /// 真机观测：detect×5 通过后 listVersions 使 sidecar 崩溃并被隔离。
    #[test]
    fn e2e_unienv_renderer_sequence() {
        let exe = sidecar_exe();
        let repo_plugin = std::env::current_dir()
            .ok()
            .and_then(|d| d.parent().map(|p| p.join("plugins").join("unienv")))
            .filter(|p| p.join("dist").join("main.js").exists());
        let installed_plugin = std::env::var_os("APPDATA").map(|a| {
            std::path::PathBuf::from(a)
                .join("cruciblebox")
                .join("plugins")
                .join("unienv")
        });
        let Some(plugin_dir) =
            repo_plugin.or(installed_plugin.filter(|p| p.join("dist").join("main.js").exists()))
        else {
            eprintln!("skipping unienv e2e: no plugin dist available");
            return;
        };
        if !exe.exists() {
            eprintln!("skipping unienv e2e: sidecar exe not built");
            return;
        }

        let dir = std::env::temp_dir().join(format!("cb-unienv-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openbox.db");
        let db = Db::open(&path).unwrap();
        {
            let guard = db.conn().lock().unwrap();
            guard
                .execute(
                    "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path, config_data)
                     VALUES ('unienv', 'unienv', '0.5.7', 'UniEnv', 'dist/main.js', ?1, ?2)
                     ON CONFLICT(id) DO NOTHING",
                    rusqlite::params![
                        plugin_dir.to_string_lossy().into_owned(),
                        // onlineVersions=off：单测确定性（不联网）
                        serde_json::json!({
                            "installRoot": "C:\\UniEnv",
                            "downloadMirror": "direct",
                            "customCombos": "[]",
                            "onlineVersions": "off"
                        })
                        .to_string()
                    ],
                )
                .unwrap();
        }

        let proc = BackendProcess::spawn(
            "unienv".into(),
            crate::permissions::PermissionGuard::parse(&["trusted:unienv".to_string()]),
            Arc::new(Mutex::new(db)),
            exe,
            plugin_dir.clone(),
            "dist/main.js".into(),
            Arc::new(|pid: &str| panic!("sidecar crashed during unienv sequence: {pid}")),
            Arc::new(|_event: &str, _payload: serde_json::Value| {}),
        )
        .expect("spawn unienv sidecar");

        let init = proc.request(
            "lifecycle.initialize",
            json!({ "pluginId": "unienv", "config": {} }),
        );
        assert!(init.is_ok(), "unienv initialize failed: {:?}", init.err());

        let send = |proc: &Arc<BackendProcess>, label: &str, msg: Value| {
            eprintln!("[seq] sending {label}");
            let r = proc.request("plugin.message", json!({ "message": msg }));
            match &r {
                Ok(v) => {
                    let s = v.to_string();
                    eprintln!(
                        "[seq] {label} -> {}",
                        if s.len() > 120 {
                            format!("{}…", &s[..120])
                        } else {
                            s
                        }
                    );
                }
                Err(e) => eprintln!("[seq] {label} -> ERR {e}"),
            }
            assert!(r.is_ok(), "{label} failed: {:?}", r.err());
            r.unwrap()
        };

        let tools = send(&proc, "listTools", json!({ "type": "listTools" }));
        assert!(
            tools.as_array().map(|a| a.len() == 7).unwrap_or(false),
            "listTools: {tools}"
        );
        send(&proc, "listCombos", json!({ "type": "listCombos" }));
        for tool in ["python", "node", "git", "go", "java"] {
            send(&proc, "detect", json!({ "type": "detect", "tool": tool }));
        }
        send(
            &proc,
            "detect-node-2",
            json!({ "type": "detect", "tool": "node" }),
        );
        let versions = send(
            &proc,
            "listVersions",
            json!({ "type": "listVersions", "tool": "node" }),
        );
        assert!(
            versions.as_array().map(|a| !a.is_empty()).unwrap_or(false),
            "listVersions(node) returned non-array/empty: {versions}"
        );

        // 真机场景复现：空闲数秒后继续请求（真机在 detect 后空闲期自发 EOF）
        std::thread::sleep(std::time::Duration::from_secs(3));
        for round in 0..2 {
            send(
                &proc,
                "detect-again",
                json!({ "type": "detect", "tool": "node" }),
            );
            let v = send(
                &proc,
                "listVersions-again",
                json!({ "type": "listVersions", "tool": "node" }),
            );
            assert!(v.as_array().map(|a| !a.is_empty()).unwrap_or(false));
            std::thread::sleep(std::time::Duration::from_secs(2 * (round + 1)));
        }
        // 进程必须仍然存活（未发生自发退出）
        let alive = proc
            .child
            .lock()
            .unwrap()
            .try_wait()
            .ok()
            .flatten()
            .is_none();
        assert!(alive, "sidecar exited spontaneously during idle period");

        let _ = proc.request("lifecycle.dispose", json!({}));
        proc.kill_now("test cleanup");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Document Engine renderer smoke: the real plugin backend must traverse
    /// trusted.invoke and return valid status, jobs and model envelopes.
    /// This catches an installed-path/permission/sidecar mismatch before the
    /// UI can turn it into several unrelated "读取失败" messages.
    #[test]
    fn e2e_document_engine_renderer_sequence() {
        let exe = sidecar_exe();
        let plugin_dir = std::env::current_dir()
            .ok()
            .and_then(|d| {
                d.parent()
                    .map(|p| p.join("plugins").join("document-engine"))
            })
            .filter(|p| p.join("dist").join("main.js").exists());
        let Some(plugin_dir) = plugin_dir else {
            eprintln!("skipping document-engine e2e: no plugin dist available");
            return;
        };
        if !exe.exists() {
            eprintln!("skipping document-engine e2e: sidecar exe not built");
            return;
        }

        let dir =
            std::env::temp_dir().join(format!("cb-document-engine-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openbox.db");
        let db = Db::open(&path).unwrap();
        {
            let guard = db.conn().lock().unwrap();
            guard
                .execute(
                    "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path, permissions)
                     VALUES ('document-engine', 'document-engine', '0.1.0', 'Document Engine', 'dist/main.js', ?1, ?2)
                     ON CONFLICT(id) DO NOTHING",
                    rusqlite::params![
                        plugin_dir.to_string_lossy().into_owned(),
                        serde_json::json!(["trusted:document-engine"]).to_string()
                    ],
                )
                .unwrap();
        }

        let proc = BackendProcess::spawn(
            "document-engine".into(),
            crate::permissions::PermissionGuard::parse(&["trusted:document-engine".to_string()]),
            Arc::new(Mutex::new(db)),
            exe,
            plugin_dir.clone(),
            "dist/main.js".into(),
            Arc::new(|pid: &str| panic!("sidecar crashed during document-engine sequence: {pid}")),
            Arc::new(|_event: &str, _payload: serde_json::Value| {}),
        )
        .expect("spawn document-engine sidecar");

        proc.request(
            "lifecycle.initialize",
            json!({ "pluginId": "document-engine", "config": {} }),
        )
        .expect("document-engine initialize");

        let send = |message: Value| {
            proc.request("plugin.message", json!({ "message": message }))
                .expect("document-engine message")
        };
        let status = send(json!({ "type": "getStatus" }));
        assert!(
            status["status"].is_object(),
            "invalid status envelope: {status}"
        );
        let jobs = send(json!({ "type": "document.jobs.list" }));
        assert!(jobs["tasks"].is_array(), "invalid jobs envelope: {jobs}");
        let models = send(json!({ "type": "document.models.list" }));
        assert!(
            models["models"].is_array(),
            "invalid models envelope: {models}"
        );

        let _ = proc.request("lifecycle.dispose", json!({}));
        proc.kill_now("test cleanup");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

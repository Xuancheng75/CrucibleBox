// Next backend capability router. Legacy db.query/db.execute are intentionally unsupported.
// 由 BackendProcess 读线程收到 sidecar host 请求后，在工作线程调用本分发器。
// v1.9.15：扩展实现面——network.fetch / notification.show / file.read / file.write /
// clipboard.read / clipboard.write / system.info

use crate::db::Db;
use crate::unienv_task::TaskContext;
#[cfg(test)]
use crate::unienv_task::TaskManager;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// 执行 host 方法。返回 Ok(result) 或 Err(message)。
/// 前置：调用方已做 is_host_method_implemented + PermissionGuard 校验（backend_process.rs）。
/// emitter：事件发射回调（log.write 入库后广播 plugin:log）。
pub fn host_dispatch(
    db: &Arc<Mutex<Db>>,
    services: &crate::host_services::Services,
    plugin_id: &str,
    method: &str,
    params: &Value,
    emitter: &(dyn Fn(&str, serde_json::Value) + Send + Sync),
) -> Result<Value, String> {
    match method {
        "plugin.root" => {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            let path = db
                .plugin_find_by_id(plugin_id)?
                .ok_or("plugin not found")?
                .installed_path;
            Ok(json!({ "path": path }))
        }
        "storage.get" => {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            let key = str_param(params, "key")?;
            let v = db.storage_get(plugin_id, &key).map_err(|e| e.to_string())?;
            Ok(v.map(|raw| parse_stored(&raw)).unwrap_or(Value::Null))
        }
        "storage.set" => {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            let key = str_param(params, "key")?;
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            let serialized = serde_json::to_string(&value).map_err(|e| e.to_string())?;
            db.storage_set(plugin_id, &key, &serialized)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "storage.delete" => {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            let key = str_param(params, "key")?;
            db.storage_delete(plugin_id, &key)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "storage.list" => {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            let prefix = params.get("prefix").and_then(Value::as_str).unwrap_or("");
            let rows = db
                .storage_list(plugin_id, prefix)
                .map_err(|e| e.to_string())?;
            let items: Vec<Value> = rows
                .into_iter()
                .map(|(k, v)| json!({ "key": k, "value": parse_stored(&v) }))
                .collect();
            Ok(Value::Array(items))
        }
        "storage.batch" => {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            let mutations = params
                .get("mutations")
                .and_then(Value::as_array)
                .ok_or_else(|| "storage.batch requires mutations array".to_string())?;
            let converted = mutations
                .iter()
                .map(|m| {
                    let is_set = m.get("type").and_then(Value::as_str) == Some("set");
                    let key = m
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let value = m
                        .get("value")
                        .cloned()
                        .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "null".into()));
                    (is_set, key, value)
                })
                .collect::<Vec<_>>();
            db.storage_batch(plugin_id, &converted)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "log.write" => {
            let db = db.lock().unwrap_or_else(|p| p.into_inner());
            let level = params
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("info");
            let message = str_param(params, "message")?;
            db.log_write(plugin_id, level, &message)
                .map_err(|e| e.to_string())?;
            // plugin:log：入库后广播（对等 PluginLogService emitLog → plugin:log）
            emitter(
                "plugin:log",
                json!({ "pluginId": plugin_id, "level": level, "message": message }),
            );
            Ok(Value::Null)
        }
        "event.emit" => {
            let event = str_param(params, "event")?;
            let data = params.get("data").cloned().unwrap_or(Value::Null);
            emitter(
                "plugin:event",
                json!({ "pluginId": plugin_id, "event": event, "data": data }),
            );
            Ok(Value::Null)
        }
        "event.subscribe" | "event.unsubscribe" => Ok(Value::Null),
        "trusted.invoke" => {
            let service = params
                .get("service")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let operation = params
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let payload = params.get("payload");
            // 统一分发器：按 service 参数路由到对应的宿主固定可信服务。
            // UniEnv 保留旧签名（service 透传）；Document Engine 自身即目标服务。
            match service.as_str() {
                "unienv" => {
                    // 在线版本发现有明确的网络等待上限，但不应让该等待占用
                    // 宿主的外层数据库互斥锁。配置只读取一次，随后在无锁路径
                    // 执行受限 HTTP 请求，其它插件的设置/日志仍可并行访问。
                    let online_check = operation == "message"
                        && payload
                            .and_then(|value| value.get("type"))
                            .and_then(Value::as_str)
                            == Some("checkOnlineVersions");
                    if online_check {
                        let config = {
                            let db = db.lock().unwrap_or_else(|p| p.into_inner());
                            crate::unienv_service::load_config_for_host(&db, plugin_id)
                        };
                        return crate::unienv_service::dispatch_online_versions(
                            &config,
                            payload
                                .ok_or_else(|| "message operation requires payload".to_string())?,
                        );
                    }
                    let repository = db.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    crate::unienv_service::dispatch(
                        &services.unienv,
                        &repository,
                        plugin_id,
                        &service,
                        &operation,
                        payload,
                    )
                }
                "document-engine" => {
                    let remote_model = operation == "message"
                        && payload
                            .and_then(|value| value.get("type"))
                            .and_then(Value::as_str)
                            .is_some_and(|kind| {
                                kind == "document.models.update"
                                    || (kind == "document.models.install"
                                        && payload
                                            .and_then(|value| value.get("url"))
                                            .and_then(Value::as_str)
                                            .is_some())
                            });
                    if remote_model {
                        let config = {
                            let db = db.lock().unwrap_or_else(|p| p.into_inner());
                            crate::document_engine_service::load_config_for_host(
                                &services.document,
                                &db,
                                plugin_id,
                            )
                        };
                        return crate::document_engine_service::dispatch_remote_model(
                            &config,
                            payload
                                .ok_or_else(|| "message operation requires payload".to_string())?,
                        );
                    }
                    let repository = db.lock().unwrap_or_else(|p| p.into_inner()).clone();
                    crate::document_engine_service::dispatch(
                        &services.document,
                        &repository,
                        plugin_id,
                        &operation,
                        payload,
                    )
                }
                "archive-extractor" => crate::archive_service::dispatch(
                    &services.archive,
                    plugin_id,
                    &operation,
                    payload,
                ),
                _ => Err(format!("unknown trusted service: {service}")),
            }
        }
        "notification.show" => {
            let title = str_param(params, "title")?;
            let body = params
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            emitter(
                "plugin:notification",
                json!({ "pluginId": plugin_id, "title": title, "body": body }),
            );
            Ok(json!({ "shown": true }))
        }
        "dialog.open" => {
            let app = services
                .platform
                .app_handle
                .as_ref()
                .ok_or_else(|| "application handle is not ready".to_string())?;
            let kind = str_param(params, "type")?;
            let selected = match kind.as_str() {
                "file" => app.dialog().file().blocking_pick_file(),
                "folder" => app.dialog().file().blocking_pick_folder(),
                _ => return Err("dialog type must be file or folder".into()),
            };
            Ok(selected
                .map(|path| Value::String(path.to_string()))
                .unwrap_or(Value::Null))
        }
        "network.fetch" => {
            let url = str_param(params, "url")?;
            let opts = params
                .get("options")
                .or_else(|| params.get("opts"))
                .cloned()
                .unwrap_or(Value::Null);
            let method = opts
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_uppercase();
            let (agent, _) = crate::network_policy::current().agent_for_url(&url, 30, 30)?;
            let mut req = match method.as_str() {
                "GET" => agent.get(&url),
                "POST" => agent.post(&url),
                "PUT" => agent.put(&url),
                "DELETE" => agent.delete(&url),
                "HEAD" => agent.head(&url),
                "PATCH" => agent.request("PATCH", &url),
                _ => return Err(format!("unsupported HTTP method: {method}")),
            };
            if let Some(headers) = opts.get("headers").and_then(Value::as_object) {
                for (k, v) in headers {
                    if let Some(vs) = v.as_str() {
                        req = req.set(k, vs);
                    }
                }
            }
            let response = if matches!(method.as_str(), "GET" | "HEAD" | "DELETE") {
                req.call()
            } else {
                let body = opts
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                req.send_string(&body)
            };
            let resp = match response {
                Ok(resp) | Err(ureq::Error::Status(_, resp)) => resp,
                Err(error) => return Err(error.to_string()),
            };
            let status = resp.status();
            let status_text = resp.status_text().to_string();
            let mut resp_headers = serde_json::Map::new();
            for name in resp.headers_names() {
                if let Some(val) = resp.header(&name) {
                    resp_headers.insert(name, Value::String(val.to_string()));
                }
            }
            let mut body_bytes = Vec::new();
            resp.into_reader()
                .take(50 * 1024 * 1024)
                .read_to_end(&mut body_bytes)
                .map_err(|e| e.to_string())?;
            let body_str = String::from_utf8_lossy(&body_bytes).into_owned();
            Ok(json!({
                "ok": (200..300).contains(&status),
                "status": status,
                "statusText": status_text,
                "headers": resp_headers,
                "body": body_str
            }))
        }
        "process.run" => run_plugin_process(params),
        "process.start" => {
            let request = params.clone();
            let task_id = services.process_tasks.start(
                &format!("plugin-process:{plugin_id}"),
                Box::new(move |ctx| run_plugin_process_task(ctx, &request)),
            )?;
            Ok(json!({ "taskId": task_id }))
        }
        "process.getTask" | "process.cancel" => {
            let task_id = str_param(params, "taskId")?;
            let snapshot = services
                .process_tasks
                .get(&task_id)
                .ok_or_else(|| "任务不存在".to_string())?;
            if snapshot["resourceKey"] != format!("plugin-process:{plugin_id}") {
                return Err("任务不属于当前插件".into());
            }
            if method == "process.cancel" {
                Ok(json!({ "cancelled": services.process_tasks.cancel(&task_id) }))
            } else {
                Ok(snapshot)
            }
        }
        "file.read" => {
            let path = str_param(params, "path")?;
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
            Ok(json!({ "content": encoded, "encoding": "base64" }))
        }
        "file.write" => {
            let path = str_param(params, "path")?;
            let encoded = str_param(params, "base64")?;
            let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                .map_err(|error| format!("invalid base64 payload: {error}"))?;
            if let Some(parent) = std::path::Path::new(&path).parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, data).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "shortcut.register" => {
            let keys = str_param(params, "keys")?;
            services
                .platform
                .app_handle
                .as_ref()
                .ok_or_else(|| "application handle is not ready".to_string())?
                .global_shortcut()
                .register(keys.as_str())
                .map_err(|error| format!("shortcut register failed: {error}"))?;
            Ok(Value::Null)
        }
        "shortcut.unregister" => {
            let keys = str_param(params, "keys")?;
            services
                .platform
                .app_handle
                .as_ref()
                .ok_or_else(|| "application handle is not ready".to_string())?
                .global_shortcut()
                .unregister(keys.as_str())
                .map_err(|error| format!("shortcut unregister failed: {error}"))?;
            Ok(Value::Null)
        }
        "clipboard.read" => {
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
            let text = clipboard
                .get_text()
                .map_err(|e| format!("clipboard read failed: {e}"))?;
            Ok(json!({ "text": text }))
        }
        "clipboard.write" => {
            let text = str_param(params, "text")?;
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
            clipboard
                .set_text(&text)
                .map_err(|e| format!("clipboard write failed: {e}"))?;
            Ok(json!({ "ok": true }))
        }
        "system.info" => {
            let cache = &services.platform.system_info_cache;
            if let Some((_at, value)) = cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .filter(|(at, _)| at.elapsed() < Duration::from_secs(2))
            {
                return Ok(value.clone());
            }
            use sysinfo::System;
            let mut sys = System::new_all();
            sys.refresh_all();
            let cpu_brand = sys
                .cpus()
                .first()
                .map(|c| c.brand().to_string())
                .unwrap_or_default();
            let cpu_usage = sys.global_cpu_usage();
            let total_mem = sys.total_memory();
            let available_mem = sys.available_memory();
            let mut disks = Vec::new();
            for d in sysinfo::Disks::new_with_refreshed_list().iter() {
                disks.push(json!({
                    "name": d.mount_point().to_string_lossy(),
                    "total": d.total_space(),
                    "available": d.available_space()
                }));
            }
            let mut networks = Vec::new();
            for (name, data) in sysinfo::Networks::new_with_refreshed_list().iter() {
                let ip = data
                    .ip_networks()
                    .first()
                    .map(|n| n.addr.to_string())
                    .unwrap_or_default();
                let mac = data.mac_address().to_string();
                networks.push(json!({ "name": name, "ip": ip, "mac": mac }));
            }
            let value = json!({
                "os": {
                    "name": System::name().unwrap_or_default(),
                    "version": System::os_version().unwrap_or_default(),
                    "hostname": System::host_name().unwrap_or_default()
                },
                "cpu": {
                    "brand": cpu_brand,
                    "cores": sys.cpus().len(),
                    "physicalCores": sys.physical_core_count().unwrap_or(0),
                    "usage": cpu_usage
                },
                "memory": {
                    "total": total_mem,
                    "available": available_mem,
                    "usage": if total_mem > 0 {
                        ((total_mem - available_mem) as f64 / total_mem as f64) * 100.0
                    } else {
                        0.0
                    }
                },
                "disks": disks,
                "network": networks
            });
            *cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                Some((Instant::now(), value.clone()));
            Ok(value)
        }
        _ => Err("host method not implemented".into()),
    }
}

fn read_process_output<R: Read + Send + 'static>(mut stream: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut stored = Vec::new();
        let mut buffer = [0_u8; 8192];
        while let Ok(count) = stream.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let remaining = (1024 * 1024_usize).saturating_sub(stored.len());
            stored.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        String::from_utf8_lossy(&stored).into_owned()
    })
}

fn terminate_process_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .creation_flags(0x0800_0000)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn run_plugin_process(params: &Value) -> Result<Value, String> {
    use std::process::{Command, Stdio};
    let program = str_param(params, "program")?;
    let args = params
        .get("args")
        .and_then(Value::as_array)
        .ok_or_else(|| "process args must be an array".to_string())?;
    let mut command = Command::new(program);
    for arg in args {
        command.arg(
            arg.as_str()
                .ok_or_else(|| "process args must be strings".to_string())?,
        );
    }
    if let Some(cwd) = params.get("cwd").and_then(Value::as_str) {
        command.current_dir(cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let timeout_ms = params
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(20_000)
        .min(20_000);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动本地程序：{error}"))?;
    let stdout = read_process_output(
        child
            .stdout
            .take()
            .ok_or_else(|| "无法读取程序输出".to_string())?,
    );
    let stderr = read_process_output(
        child
            .stderr
            .take()
            .ok_or_else(|| "无法读取程序错误输出".to_string())?,
    );
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let (exit_code, timed_out) = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break (status.code(), false);
        }
        if Instant::now() >= deadline {
            terminate_process_tree(&mut child);
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    Ok(json!({
        "exitCode": exit_code,
        "stdout": stdout.join().unwrap_or_default(),
        "stderr": stderr.join().unwrap_or_default(),
        "timedOut": timed_out
    }))
}

fn run_plugin_process_task(ctx: &TaskContext, params: &Value) -> Result<Value, String> {
    use std::process::{Command, Stdio};
    const OUTPUT_ARG: &str = "__CRUCIBLEBOX_OUTPUT__";
    let program = str_param(params, "program")?;
    let args = params
        .get("args")
        .and_then(Value::as_array)
        .ok_or_else(|| "process args must be an array".to_string())?;
    if params.get("outputValidation").and_then(Value::as_str) == Some("media") {
        let probe_name = if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        };
        let probe = Path::new(&program).with_file_name(probe_name);
        if !Path::new(&program).is_file() {
            return Err("找不到 FFmpeg 程序，请重新选择 ffmpeg.exe".into());
        }
        if !probe.is_file() {
            return Err("所选 FFmpeg 目录缺少 ffprobe.exe，请选择包含两个程序的目录".into());
        }
    }
    let output_transaction = params
        .get("outputTarget")
        .and_then(Value::as_str)
        .map(|target| {
            let target = Path::new(target);
            if !target.is_absolute() {
                return Err("输出路径必须为绝对路径".to_string());
            }
            if target.exists() {
                let canonical_target = target
                    .canonicalize()
                    .map_err(|error| format!("检查输出路径失败：{error}"))?;
                for input in params
                    .get("inputPaths")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let input = input
                        .as_str()
                        .ok_or_else(|| "输入路径必须是字符串".to_string())?;
                    let canonical_input = Path::new(input)
                        .canonicalize()
                        .map_err(|error| format!("检查输入路径失败：{error}"))?;
                    if canonical_input == canonical_target {
                        return Err("输入和输出不能是同一个文件".into());
                    }
                }
            }
            crate::output_transaction::OutputTransaction::new(target, false)
        })
        .transpose()?;
    let mut command = Command::new(&program);
    let mut output_arg_count = 0;
    for arg in args {
        let arg = arg
            .as_str()
            .ok_or_else(|| "process args must be strings".to_string())?;
        if arg == OUTPUT_ARG {
            output_arg_count += 1;
            let transaction = output_transaction
                .as_ref()
                .ok_or_else(|| "输出占位符缺少输出事务".to_string())?;
            command.arg(transaction.stage_path());
        } else {
            command.arg(arg);
        }
    }
    if output_transaction.is_some() && output_arg_count != 1 {
        return Err("输出事务必须包含一个输出占位符".into());
    }
    if let Some(cwd) = params.get("cwd").and_then(Value::as_str) {
        command.current_dir(cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let timeout_ms = params
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(600_000)
        .min(1_800_000);
    ctx.update_progress("running", 5, "本地程序运行中…");
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动本地程序：{error}"))?;
    let stdout = read_process_output(
        child
            .stdout
            .take()
            .ok_or_else(|| "无法读取程序输出".to_string())?,
    );
    let stderr = read_process_output(
        child
            .stderr
            .take()
            .ok_or_else(|| "无法读取程序错误输出".to_string())?,
    );
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let (exit_code, timed_out) = loop {
        if ctx.is_cancelled() {
            terminate_process_tree(&mut child);
            return Err("操作已取消".into());
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break (status.code(), false);
        }
        if Instant::now() >= deadline {
            terminate_process_tree(&mut child);
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if let Some(transaction) = output_transaction {
        if timed_out {
            return Err("本地程序运行超时，暂存输出已清理".into());
        }
        if exit_code != Some(0) {
            return Err(format!(
                "本地程序执行失败（退出码 {}）：{}",
                exit_code.map_or_else(|| "未知".into(), |code| code.to_string()),
                stderr.chars().take(320).collect::<String>()
            ));
        }
        if ctx.is_cancelled() {
            return Err("操作已取消".into());
        }
        ctx.update_progress("validating", 95, "正在验证并保存输出…");
        let validation = params
            .get("outputValidation")
            .and_then(Value::as_str)
            .unwrap_or("nonempty");
        let size = std::fs::metadata(transaction.stage_path())
            .map_err(|error| format!("读取暂存输出失败：{error}"))?
            .len();
        if size == 0 {
            return Err("处理结果为空，未保存输出文件".into());
        }
        match validation {
            "media" => validate_media_output(&program, transaction.stage_path(), ctx)?,
            "nonempty" => {}
            _ => return Err("不支持的输出验证方式".into()),
        }
        let published = transaction.publish_durable(ctx.runtime_context(), true, |_| Ok(()))?;
        Ok(json!({
            "exitCode": exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "timedOut": timed_out,
            "outputPath": published.to_string_lossy()
        }))
    } else {
        ctx.update_progress("done", 100, "本地程序已结束");
        Ok(
            json!({ "exitCode": exit_code, "stdout": stdout, "stderr": stderr, "timedOut": timed_out }),
        )
    }
}

fn validate_media_output(program: &str, stage: &Path, ctx: &TaskContext) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let probe_name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let probe = Path::new(program).with_file_name(probe_name);
    let mut child = Command::new(&probe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(stage)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动 ffprobe 验证输出：{error}"))?;
    let stdout = read_process_output(child.stdout.take().ok_or("无法读取 ffprobe 输出")?);
    let stderr = read_process_output(child.stderr.take().ok_or("无法读取 ffprobe 错误")?);
    let deadline = Instant::now() + Duration::from_secs(8);
    let status = loop {
        if ctx.is_cancelled() {
            terminate_process_tree(&mut child);
            return Err("操作已取消".into());
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            terminate_process_tree(&mut child);
            return Err("ffprobe 验证超时，输出未保存".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "音视频输出验证失败：{}",
            stderr.chars().take(320).collect::<String>()
        ));
    }
    if !stdout
        .lines()
        .any(|line| matches!(line.trim(), "audio" | "video"))
    {
        return Err("输出文件未包含可读取的音频或视频流".into());
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod process_output_tests {
    use super::*;
    use std::fs;

    fn wait_for_task(params: Value) -> Value {
        let tasks = Arc::new(TaskManager::default());
        let task_id = tasks
            .start(
                "process-output-test",
                Box::new(move |ctx| run_plugin_process_task(ctx, &params)),
            )
            .unwrap();
        for _ in 0..100 {
            let snapshot = tasks.get(&task_id).unwrap();
            if matches!(
                snapshot["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("process output task did not finish");
    }

    #[test]
    fn staged_process_output_preserves_existing_target() {
        let dir = std::env::temp_dir().join(format!(
            "cb-process-output-{}",
            crate::rand_token::random_token_alnum(10).unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("结果.txt");
        let script = dir.join("write-output.ps1");
        fs::write(
            &script,
            "param([string]$outputPath)\n[IO.File]::WriteAllText($outputPath, 'new')\n",
        )
        .unwrap();
        fs::write(&target, b"old").unwrap();
        let snapshot = wait_for_task(json!({
            "program": "powershell.exe",
            "args": [
                "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
                script.to_string_lossy(),
                "__CRUCIBLEBOX_OUTPUT__"
            ],
            "outputTarget": target.to_string_lossy(),
            "outputValidation": "nonempty",
            "inputPaths": [],
            "timeoutMs": 10_000
        }));
        assert_eq!(snapshot["status"], "succeeded", "{snapshot}");
        assert_eq!(fs::read(&target).unwrap(), b"old");
        // A durable publication can be terminal before the executor attaches stdout metadata.
        // Its committed path is already authoritative and must be usable immediately.
        let output = snapshot["result"]["outputPath"]
            .as_str()
            .or_else(|| snapshot["result"]["path"].as_str())
            .unwrap();
        assert_ne!(output, target.to_string_lossy());
        assert_eq!(fs::read(output).unwrap(), b"new");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn staged_process_output_rejects_input_overwrite() {
        let dir = std::env::temp_dir().join(format!(
            "cb-process-output-{}",
            crate::rand_token::random_token_alnum(10).unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("源.txt");
        fs::write(&target, b"original").unwrap();
        let snapshot = wait_for_task(json!({
            "program": "powershell.exe",
            "args": ["-NoProfile", "-Command", "exit 0", "__CRUCIBLEBOX_OUTPUT__"],
            "outputTarget": target.to_string_lossy(),
            "inputPaths": [target.to_string_lossy()]
        }));
        assert_eq!(snapshot["status"], "failed", "{snapshot}");
        assert_eq!(fs::read(&target).unwrap(), b"original");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_validation_removes_staged_process_output() {
        let dir = std::env::temp_dir().join(format!(
            "cb-process-output-{}",
            crate::rand_token::random_token_alnum(10).unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("report.txt");
        let script = dir.join("write-output.ps1");
        fs::write(
            &script,
            "param([string]$outputPath)\n[IO.File]::WriteAllText($outputPath, 'new')\n",
        )
        .unwrap();
        fs::write(&target, b"old").unwrap();
        let snapshot = wait_for_task(json!({
            "program": "powershell.exe",
            "args": [
                "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
                script.to_string_lossy(), "__CRUCIBLEBOX_OUTPUT__"
            ],
            "outputTarget": target.to_string_lossy(),
            "outputValidation": "unsupported",
            "timeoutMs": 10_000
        }));
        assert_eq!(snapshot["status"], "failed", "{snapshot}");
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn media_task_rejects_missing_probe_before_starting() {
        let dir = std::env::temp_dir().join(format!(
            "cb-process-output-{}",
            crate::rand_token::random_token_alnum(10).unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        let program = dir.join("ffmpeg.exe");
        fs::write(&program, b"placeholder").unwrap();
        let target = dir.join("video.mp4");
        let snapshot = wait_for_task(json!({
            "program": program.to_string_lossy(),
            "args": ["__CRUCIBLEBOX_OUTPUT__"],
            "outputTarget": target.to_string_lossy(),
            "outputValidation": "media"
        }));
        assert_eq!(snapshot["status"], "failed", "{snapshot}");
        assert!(snapshot["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ffprobe.exe"));
        assert!(!target.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "requires CRUCIBLEBOX_ACCEPTANCE_FFMPEG with a paired ffprobe.exe"]
    fn real_ffmpeg_output_is_validated_and_published() {
        let program = std::env::var("CRUCIBLEBOX_ACCEPTANCE_FFMPEG").unwrap();
        let dir = std::env::temp_dir().join(format!(
            "cb-ffmpeg-acceptance-{}",
            crate::rand_token::random_token_alnum(10).unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("source.mp4");
        let target = dir.join("clip.mp4");
        let generated = std::process::Command::new(&program)
            .args(["-hide_banner", "-nostdin", "-f", "lavfi", "-i"])
            .arg("testsrc2=duration=1:size=128x96:rate=10")
            .args(["-c:v", "libopenh264", "-b:v", "500k"])
            .arg(&input)
            .output()
            .unwrap();
        assert!(generated.status.success());
        let snapshot = wait_for_task(json!({
            "program": program,
            "args": [
                "-hide_banner", "-nostdin", "-y", "-i", input.to_string_lossy(),
                "-t", "0.5", "-c:v", "libopenh264", "-b:v", "500k",
                "__CRUCIBLEBOX_OUTPUT__"
            ],
            "outputTarget": target.to_string_lossy(),
            "outputValidation": "media",
            "inputPaths": [input.to_string_lossy()],
            "timeoutMs": 10_000
        }));
        assert_eq!(snapshot["status"], "succeeded", "{snapshot}");
        // A durable publication can be terminal before the executor attaches stdout metadata.
        // Its committed path is already authoritative and must be usable immediately.
        let output = snapshot["result"]["outputPath"]
            .as_str()
            .or_else(|| snapshot["result"]["path"].as_str())
            .unwrap();
        assert!(Path::new(output).is_file());
        assert!(fs::metadata(output).unwrap().len() > 0);
        assert_eq!(snapshot["result"]["exitCode"], 0);
        fs::remove_dir_all(dir).unwrap();
    }
}

// ---------------------------------------------------------------------------
// helpers（对等 PluginProcessEntry 的参数读取语义）
// ---------------------------------------------------------------------------

fn str_param(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing string param: {key}"))
}

fn parse_stored(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

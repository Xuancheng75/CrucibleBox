// host 方法分发器（1.9.2-a，对等 PluginManager.ts 的 db.query/execute + storage/log 转发）
// 由 BackendProcess 读线程收到 sidecar host 请求后，在工作线程调用本分发器。
// v1.9.15：扩展实现面——network.fetch / notification.show / file.read / file.write /
// clipboard.read / clipboard.write / system.info

use crate::db::Db;
use serde_json::{json, Value};
use std::io::Read;
use std::sync::{Arc, Mutex};

/// 执行 host 方法。返回 Ok(result) 或 Err(message)。
/// 前置：调用方已做 is_host_method_implemented + PermissionGuard 校验（backend_process.rs）。
/// emitter：事件发射回调（log.write 入库后广播 plugin:log）。
pub fn host_dispatch(
    db: &Arc<Mutex<Db>>,
    plugin_id: &str,
    method: &str,
    params: &Value,
    emitter: &(dyn Fn(&str, serde_json::Value) + Send + Sync),
) -> Result<Value, String> {
    let db = db.lock().unwrap_or_else(|p| p.into_inner());
    match method {
        "db.query" => {
            let sql = str_param(params, "sql")?;
            let p = array_param(params, "params")?;
            let rows = query_rows(&db, &sql, &p).map_err(|e| e.to_string())?;
            Ok(rows)
        }
        "db.execute" => {
            let sql = str_param(params, "sql")?;
            let p = array_param(params, "params")?;
            db.execute_sql(&sql, &p).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "storage.get" => {
            let key = str_param(params, "key")?;
            let v = db.storage_get(plugin_id, &key).map_err(|e| e.to_string())?;
            Ok(v.map(|raw| parse_stored(&raw)).unwrap_or(Value::Null))
        }
        "storage.set" => {
            let key = str_param(params, "key")?;
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            let serialized = serde_json::to_string(&value).map_err(|e| e.to_string())?;
            db.storage_set(plugin_id, &key, &serialized)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "storage.delete" => {
            let key = str_param(params, "key")?;
            db.storage_delete(plugin_id, &key)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "storage.list" => {
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
        // event.*：1.9.2-a 最小面——subscribe/unsubscribe 记录接受（no-op 通过），
        // emit 由宿主事件总线后续（1.9.2-c）；当前返回 Null 保证不 NOT_ALLOWED。
        "event.emit" | "event.subscribe" | "event.unsubscribe" => Ok(Value::Null),
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
                    crate::unienv_service::dispatch(&db, plugin_id, &service, &operation, payload)
                }
                "document-engine" => {
                    crate::document_engine_service::dispatch(&db, plugin_id, &operation, payload)
                }
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
        "network.fetch" => {
            let url = str_param(params, "url")?;
            let opts = params.get("opts").cloned().unwrap_or(Value::Null);
            let method = opts
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_uppercase();
            let timeout = std::time::Duration::from_secs(30);
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(timeout)
                .timeout_read(timeout)
                .build();
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
            let resp = if matches!(method.as_str(), "GET" | "HEAD" | "DELETE") {
                req.call().map_err(|e| e.to_string())?
            } else {
                let body = opts
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                req.send_string(&body).map_err(|e| e.to_string())?
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
                "status": status,
                "statusText": status_text,
                "headers": resp_headers,
                "body": body_str
            }))
        }
        "file.read" => {
            let path = str_param(params, "path")?;
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
            Ok(json!({ "content": encoded, "encoding": "base64" }))
        }
        "file.write" => {
            let path = str_param(params, "path")?;
            let data = params.get("data").and_then(Value::as_str).unwrap_or("");
            if let Some(parent) = std::path::Path::new(&path).parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, data.as_bytes()).map_err(|e| e.to_string())?;
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
            Ok(json!({
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
            }))
        }
        _ => Err("host method not implemented".into()),
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

fn array_param(params: &Value, key: &str) -> Result<Vec<Value>, String> {
    match params.get(key) {
        Some(Value::Array(items)) => Ok(items.clone()),
        None | Some(Value::Null) => Ok(Vec::new()),
        _ => Err(format!("param must be array: {key}")),
    }
}

/// SQL 参数转换（对等 commands.rs 旧 db_execute 的 JSON→SQLite 映射）
fn to_sql_value(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as SqlValue;
    match v {
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Number(n) => n
            .as_i64()
            .map(SqlValue::Integer)
            .or_else(|| n.as_f64().map(SqlValue::Real))
            .unwrap_or(SqlValue::Null),
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Null => SqlValue::Null,
        _ => SqlValue::Text(v.to_string()),
    }
}

fn query_rows(db: &Db, sql: &str, params: &[Value]) -> rusqlite::Result<Value> {
    let guard = db.conn().lock().unwrap();
    let mut stmt = guard.prepare(sql)?;
    let sql_values: Vec<rusqlite::types::Value> = params.iter().map(to_sql_value).collect();
    let col_names: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(sql_values.iter()), |row| {
        let mut map = serde_json::Map::new();
        for (idx, name) in col_names.iter().enumerate() {
            let v = value_ref_to_json(&row.get_ref(idx)?)?;
            map.insert(name.clone(), v);
        }
        Ok(map)
    })?;
    let out: Vec<Value> = rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(Value::Object)
        .collect();
    Ok(Value::Array(out))
}

fn value_ref_to_json(v: &rusqlite::types::ValueRef) -> rusqlite::Result<Value> {
    use rusqlite::types::ValueRef;
    Ok(match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Number((*i).into()),
        ValueRef::Real(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(s) => Value::String(String::from_utf8_lossy(s).into_owned()),
        ValueRef::Blob(b) => json!({
            "type": "Buffer",
            "data": b.iter().copied().map(u64::from).collect::<Vec<_>>(),
        }),
    })
}

fn parse_stored(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

/// 供 db.rs 的 execute_sql 使用（在 db.rs impl 中调用）
impl Db {
    /// 执行写 SQL（对等 dbExecute：返回受影响行数）
    pub fn execute_sql(&self, sql: &str, params: &[Value]) -> rusqlite::Result<usize> {
        let guard = self.conn().lock().unwrap();
        let sql_values: Vec<rusqlite::types::Value> = params.iter().map(to_sql_value).collect();
        guard.execute(sql, rusqlite::params_from_iter(sql_values.iter()))
    }
}

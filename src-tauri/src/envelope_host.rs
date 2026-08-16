// host 方法分发器（1.9.2-a，对等 PluginManager.ts 的 db.query/execute + storage/log 转发）
// 由 BackendProcess 读线程收到 sidecar host 请求后，在工作线程调用本分发器。

use crate::db::Db;
use serde_json::{json, Value};
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

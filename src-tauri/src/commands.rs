// 1.8.1 核心 IPC 命令集（Tauri commands）
// 对等 electron/ipc/*.ts + settings/theme/plugin 读路径。
// 并发模型：所有 DB 命令用 #[tauri::command(async)] 跑在线程池（阻塞安全），
// 经 tauri::State<Mutex<Db>> 单连接串行化（与 better-sqlite3 单连接语义对等）。
// 安全模型（对等 electron/ipc/ipcGuard.ts assertTrustedSender + settings 白名单）：
// - 所有命令校验调用窗口为主窗口 main frame（label=main）
// - settings_set 仅允许白名单 key（当前仅 'theme'；对等 settings.ipc.ts）

use crate::db::Db;
use rusqlite::types::Value as SqlValue;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{State, WebviewWindow};

/// 渲染进程可写 settings key 白名单（对等 electron/ipc/settings.ipc.ts）
const ALLOWED_SETTINGS_KEYS: &[&str] = &["theme"];

fn lock<'a>(db: &'a Mutex<Db>) -> std::sync::MutexGuard<'a, Db> {
    db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 校验调用方为主窗口 main frame（对等 assertTrustedSender 的窗口级约束）。
/// 插件 webview / 未知窗口一律拒绝。
fn is_main_window(window: &WebviewWindow) -> bool {
    window.label() == "main"
}

// ---------------------------------------------------------------------------
// settings（对等 SettingsRepository + settings.ipc.ts）
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn settings_get(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
    key: String,
) -> Result<Option<String>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    let v: Option<String> = guard
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [&key],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(v)
}

#[tauri::command(async)]
pub fn settings_set(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
    key: String,
    value: String,
) -> Result<bool, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    if !ALLOWED_SETTINGS_KEYS.contains(&key.as_str()) {
        return Err(format!("setting key not allowed: {key}"));
    }
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    guard
        .execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )
        .map_err(|e| e.to_string())?;
    // 对等 TS：set 成功恒返回 true（含同值更新）
    Ok(true)
}

#[tauri::command(async)]
pub fn settings_get_all(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
) -> Result<Vec<(String, String)>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    let mut stmt = guard
        .prepare("SELECT key, value FROM settings")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// app（对等 app.ipc.ts）
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn app_get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn app_get_platform() -> String {
    if cfg!(windows) {
        "win32".into()
    } else if cfg!(target_os = "macos") {
        "darwin".into()
    } else {
        "linux".into()
    }
}

// ---------------------------------------------------------------------------
// plugins（对等 plugin.ipc.ts 读路径：list/get）
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMetaDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub icon: String,
    pub entry_main: String,
    pub entry_renderer: String,
    pub permissions: Vec<String>,
    pub config_schema: serde_json::Value,
    pub config_data: serde_json::Value,
    pub enabled: bool,
    pub installed_path: String,
    pub installed_at: String,
    pub updated_at: String,
    pub sort_order: i64,
}

fn json_or_empty(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({}))
}

fn row_to_meta(row: &rusqlite::Row) -> rusqlite::Result<PluginMetaDto> {
    Ok(PluginMetaDto {
        id: row.get("id")?,
        name: row.get("name")?,
        version: row.get("version")?,
        display_name: row.get("display_name")?,
        description: row.get("description").unwrap_or_default(),
        author: row.get("author").unwrap_or_default(),
        icon: row.get("icon").unwrap_or_default(),
        entry_main: row.get("entry_main")?,
        entry_renderer: row.get("entry_renderer").unwrap_or_default(),
        permissions: serde_json::from_str(&row.get::<_, String>("permissions").unwrap_or_default())
            .unwrap_or_default(),
        config_schema: json_or_empty(&row.get::<_, String>("config_schema").unwrap_or_default()),
        config_data: json_or_empty(&row.get::<_, String>("config_data").unwrap_or_default()),
        enabled: row.get::<_, i64>("enabled").unwrap_or(1) == 1,
        installed_path: row.get("installed_path")?,
        installed_at: row.get("installed_at").unwrap_or_default(),
        updated_at: row.get("updated_at").unwrap_or_default(),
        sort_order: row.get("sort_order").unwrap_or(0),
    })
}

const PLUGIN_COLUMNS: &str = "id, name, version, display_name, description, author, icon, \
     entry_main, entry_renderer, permissions, config_schema, config_data, enabled, \
     installed_path, installed_at, updated_at, sort_order";

#[tauri::command(async)]
pub fn plugin_list(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
) -> Result<Vec<PluginMetaDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    // 对等 plugin.repository.ts 排序：sort_order ASC, installed_at DESC
    let mut stmt = guard
        .prepare(&format!(
            "SELECT {} FROM plugins ORDER BY sort_order ASC, installed_at DESC, id ASC",
            PLUGIN_COLUMNS
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_meta)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command(async)]
pub fn plugin_get(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
    id: String,
) -> Result<Option<PluginMetaDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    let sql = format!("SELECT {} FROM plugins WHERE id = ?1", PLUGIN_COLUMNS);
    let mut stmt = guard.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map([id], row_to_meta)
        .map_err(|e| e.to_string())?;
    let first = rows.next().transpose().map_err(|e| e.to_string())?;
    Ok(first)
}

// ---------------------------------------------------------------------------
// plugin renderer session（1.8.3，对等 plugin.ipc.ts create/dispose-renderer-session）
// ---------------------------------------------------------------------------

/// 创建 renderer 会话。校验插件启用 + manifest 一致性后签发 session。
#[tauri::command(async)]
pub fn create_renderer_session(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
    protocol: State<'_, std::sync::Arc<crate::plugin_protocol::ProtocolContext>>,
    id: String,
) -> Result<serde_json::Value, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    // 读插件记录（enabled 校验 + 字段透传）
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    let row = guard
        .query_row(
            "SELECT name, entry_renderer, permissions, renderer_api_version, installed_path FROM plugins WHERE id = ?1 AND enabled = 1",
            [&id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            },
        )
        .map_err(|e| format!("plugin not found or disabled: {e}"))?;
    let (name, entry_renderer, permissions_json, api_version, installed_path) = row;
    if entry_renderer.is_empty() {
        return Err("plugin has no renderer entry".into());
    }
    if api_version != 1 && api_version != 2 {
        return Err("unsupported rendererApiVersion".into());
    }
    let permissions: Vec<String> =
        serde_json::from_str(&permissions_json).unwrap_or_default();

    // runtimePath：打包态 out/plugin-frame/runtime.js；dev 态 src-tauri 相对路径
    let runtime_path = std::env::current_dir()
        .map(|d| d.join("out").join("plugin-frame").join("runtime.js"))
        .unwrap_or_else(|_| PathBuf::from("out/plugin-frame/runtime.js"));

    let session = {
        let mut reg = protocol.registry.lock().unwrap();
        reg.create(crate::plugin_session::CreateSessionInput {
            plugin_id: id,
            plugin_name: name,
            plugin_directory: installed_path,
            renderer_entry: entry_renderer,
            runtime_path: runtime_path.to_string_lossy().into_owned(),
            renderer_api_version: api_version as u8,
            permissions,
            owner_webview_label: "main".into(),
        })
        .map_err(|e| e)?
    };
    Ok(crate::plugin_protocol::session_dto(&session))
}

/// 释放 renderer 会话（对等 disposeRendererSession）。
#[tauri::command(async)]
pub fn dispose_renderer_session(
    window: WebviewWindow,
    protocol: State<'_, std::sync::Arc<crate::plugin_protocol::ProtocolContext>>,
    token: String,
) -> Result<bool, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    let removed = protocol.registry.lock().unwrap().dispose(&token);
    Ok(removed)
}

// ---------------------------------------------------------------------------
// db status（供前端诊断/基准）
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn db_status(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
) -> Result<crate::db::DbStatus, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    db.status().map_err(|e| e.to_string())
}

/// 插件侧 db.query/db.execute 在 1.8.2 sidecar 落地前的最小直通。
/// 安全说明：token 为编译期常量（非安全边界，见 oracle M1）；1.8.2 由 sidecar 携带
/// 随机令牌并经权限守卫替换。渲染进程不可获得该常量。
#[tauri::command(async)]
pub fn db_execute(
    window: WebviewWindow,
    db: State<'_, Mutex<Db>>,
    token: String,
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<usize, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    if token != crate::app::INTERNAL_DB_TOKEN {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    let values: Vec<SqlValue> = params
        .iter()
        .map(|v| match v {
            serde_json::Value::String(s) => SqlValue::Text(s.clone()),
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(SqlValue::Integer)
                .or_else(|| n.as_f64().map(SqlValue::Real))
                .unwrap_or(SqlValue::Null),
            serde_json::Value::Bool(b) => SqlValue::Integer(*b as i64),
            serde_json::Value::Null => SqlValue::Null,
            _ => SqlValue::Text(v.to_string()),
        })
        .collect();
    let guard = db.conn().lock().unwrap();
    let n = guard
        .execute(&sql, rusqlite::params_from_iter(values.iter()))
        .map_err(|e| e.to_string())?;
    Ok(n)
}

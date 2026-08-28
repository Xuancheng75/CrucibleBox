// 1.8.1 核心 IPC 命令集（Tauri commands）
// 对等 electron/ipc/*.ts + settings/theme/plugin 读路径。
// 并发模型：所有 DB 命令用 #[tauri::command(async)] 跑在线程池（阻塞安全），
// 经 tauri::State<Mutex<Db>> 单连接串行化（与 better-sqlite3 单连接语义对等）。
// 安全模型（对等 electron/ipc/ipcGuard.ts assertTrustedSender + settings 白名单）：
// - 所有命令校验调用窗口为主窗口 main frame（label=main）
// - settings_set 仅允许白名单 key（当前仅 'theme'；对等 settings.ipc.ts）

use crate::db::Db;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{State, WebviewWindow};

/// 渲染进程可写 settings key 白名单（对等 electron/ipc/settings.ipc.ts）
const ALLOWED_SETTINGS_KEYS: &[&str] = &["theme"];

fn lock<'a>(db: &'a Arc<Mutex<Db>>) -> std::sync::MutexGuard<'a, Db> {
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
    db: State<'_, Arc<Mutex<Db>>>,
    key: String,
) -> Result<Option<String>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    let v: Option<String> = guard
        .query_row("SELECT value FROM settings WHERE key = ?1", [&key], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    Ok(v)
}

#[tauri::command(async)]
pub fn settings_set(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
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
    db: State<'_, Arc<Mutex<Db>>>,
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
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
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
    db: State<'_, Arc<Mutex<Db>>>,
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
    let rows = stmt.query_map([], row_to_meta).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command(async)]
pub fn plugin_get(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
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
// plugin 写路径（1.9.2-b，对等 plugin.ipc.ts enable/disable/reorder/updateConfig/
// getLogs/clearLogs/uninstall + PluginManager 等价）
// ---------------------------------------------------------------------------

/// 启用插件：持久化 enabled + 惰性激活 backend（对等 activatePlugin）
#[tauri::command(async)]
pub fn plugin_enable(
    window: WebviewWindow,
    backend: State<'_, Arc<crate::backend_process::BackendProcessManager>>,
    install: State<'_, Arc<crate::install::InstallManager>>,
    db: State<'_, Arc<Mutex<Db>>>,
    id: String,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let record = lock(&db)
        .plugin_backend_record(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("plugin not found: {id}"))?;
    if install.is_blocked(&record.name) {
        return Err("plugin is blocked after interrupted transaction".into());
    }
    lock(&db)
        .set_plugin_enabled(&id, true)
        .map_err(|e| e.to_string())?;
    // 惰性激活 backend（若插件有 backend）
    let activation_record = lock(&db)
        .plugin_backend_record(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("plugin not found: {id}"))?;
    if let Err(error) = backend.ensure_activated(&id, activation_record) {
        let _ = lock(&db).set_plugin_enabled(&id, false);
        return Err(format!("failed to activate plugin: {error}"));
    }
    backend.emit(
        "plugin:status-change",
        serde_json::json!({ "pluginId": id, "status": "active" }),
    );
    Ok(serde_json::json!({ "success": true }))
}

/// 禁用插件：停用 backend + 持久化 enabled=false（对等 deactivatePlugin）
#[tauri::command(async)]
pub fn plugin_disable(
    window: WebviewWindow,
    backend: State<'_, Arc<crate::backend_process::BackendProcessManager>>,
    install: State<'_, Arc<crate::install::InstallManager>>,
    db: State<'_, Arc<Mutex<Db>>>,
    id: String,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let record = lock(&db)
        .plugin_backend_record(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("plugin not found: {id}"))?;
    if install.is_blocked(&record.name) {
        return Err("plugin is blocked after interrupted transaction".into());
    }
    backend.begin_maintenance(&id)?;
    let deactivate_result = backend.deactivate(&id);
    backend.end_maintenance(&id);
    deactivate_result?;
    lock(&db)
        .set_plugin_enabled(&id, false)
        .map_err(|e| e.to_string())?;
    backend.emit(
        "plugin:status-change",
        serde_json::json!({ "pluginId": id, "status": "inactive" }),
    );
    Ok(serde_json::json!({ "success": true }))
}

/// 重排插件（事务化完整排列校验，对等 reorderPlugins）
#[tauri::command(async)]
pub fn plugin_reorder(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    ordered_ids: Vec<String>,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).plugin_reorder(&ordered_ids)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 更新插件配置（对等 updateConfig）
#[tauri::command(async)]
pub fn plugin_update_config(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: String,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let serialized = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    lock(&db)
        .plugin_update_config(&id, &serialized)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

/// 查询插件日志（对等 getLogs）
#[tauri::command(async)]
pub fn plugin_get_logs(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    plugin_id: Option<String>,
    level: Option<String>,
    limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let logs =
        lock(&db).plugin_logs(plugin_id.as_deref(), level.as_deref(), limit.unwrap_or(200))?;
    Ok(serde_json::json!({ "success": true, "data": logs }))
}

/// 清空插件日志（对等 clearLogs）
#[tauri::command(async)]
pub fn plugin_clear_logs(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    plugin_id: Option<String>,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db)
        .plugin_clear_logs(plugin_id.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

/// 卸载插件：释放 renderer/backend，再以 journal + quarantine 事务删除 DB 与目录。
#[tauri::command(async)]
pub fn plugin_uninstall(
    window: WebviewWindow,
    install: State<'_, Arc<crate::install::InstallManager>>,
    protocol: State<'_, std::sync::Arc<crate::plugin_protocol::ProtocolContext>>,
    id: String,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let removed_sessions = protocol.registry.lock().unwrap().dispose_plugin(&id);
    let mut result = install.uninstall(&id)?;
    if let Some(object) = result
        .get_mut("data")
        .and_then(|value| value.as_object_mut())
    {
        object.insert(
            "removedSessions".into(),
            serde_json::json!(removed_sessions),
        );
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// plugin install 链（1.9.3，对等 PluginInstaller preview/commit/discard + 导入路径登记）
// ---------------------------------------------------------------------------

/// 安装来源 DTO（前端传 { type: "zip"|"directory", path }）
#[derive(Deserialize)]
pub struct InstallSourceDto {
    #[serde(rename = "type")]
    pub source_type: String,
    pub path: String,
}

/// 安装预览：校验来源 + manifest + 升级策略，返回 installToken（对等 previewInstall）。
/// #[tauri::command(async)] 使命令在 async runtime 线程池执行（阻塞安全）。
#[tauri::command(async)]
pub fn plugin_install_preview(
    window: WebviewWindow,
    install: State<'_, Arc<crate::install::InstallManager>>,
    source: InstallSourceDto,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let source = crate::install::InstallSource {
        source_type: source.source_type,
        path: source.path,
    };
    install.preview(source)
}

/// 安装提交：消费 installToken 执行安装/升级（对等 commitInstall）。
#[tauri::command(async)]
pub fn plugin_install_commit(
    window: WebviewWindow,
    install: State<'_, Arc<crate::install::InstallManager>>,
    token: String,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    install.commit(token)
}

/// 安装放弃：删除 token + 回滚事务 + 清理 stage（对等 discardInstall）。
#[tauri::command(async)]
pub fn plugin_install_discard(
    window: WebviewWindow,
    install: State<'_, Arc<crate::install::InstallManager>>,
    token: String,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    install.discard(token)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 登记可信导入路径（对等 PluginInstaller 的 trustedPaths 登记；容量 50）。
#[tauri::command]
pub fn plugin_register_import_path(
    window: WebviewWindow,
    install: State<'_, Arc<crate::install::InstallManager>>,
    path: String,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    install.remember_trusted_path(PathBuf::from(path));
    Ok(serde_json::json!({ "success": true }))
}

// ---------------------------------------------------------------------------
// plugin renderer session（1.8.3，对等 plugin.ipc.ts create/dispose-renderer-session）
// ---------------------------------------------------------------------------

/// 创建 renderer 会话。校验插件启用 + manifest 一致性后签发 session。
/// color_scheme：宿主当前主题模式（"dark"/"light"），用于 index.html 首帧内联背景
/// （Bug E：消除深色主题下 runtime.js 加载前的白屏闪烁）。
#[tauri::command(async)]
pub fn create_renderer_session(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    protocol: State<'_, std::sync::Arc<crate::plugin_protocol::ProtocolContext>>,
    id: String,
    color_scheme: Option<String>,
) -> Result<serde_json::Value, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    let (initial_background, scheme) = match color_scheme.as_deref() {
        Some("light") => ("#ffffff", "light"),
        _ => ("#0a0c10", "dark"),
    };
    // 读插件记录（enabled 校验 + 字段透传）
    let db = lock(&db);
    let guard = db.conn().lock().unwrap();
    let row = guard
        .query_row(
            "SELECT name, entry_renderer, permissions, installed_path FROM plugins WHERE id = ?1 AND enabled = 1",
            [&id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|e| format!("plugin not found or disabled: {e}"))?;
    let (name, entry_renderer, permissions_json, installed_path) = row;
    if entry_renderer.is_empty() {
        return Err("plugin has no renderer entry".into());
    }
    // v1 schema 无 renderer_api_version 列：从插件根目录重读 plugin.json（对等
    // PluginManager 的 manifest 校验语义），并校验 manifest 与 DB 记录一致。
    let manifest = crate::manifest::read_manifest(std::path::Path::new(&installed_path))
        .map_err(|e| format!("failed to read plugin manifest: {e}"))?;
    if manifest.name != name {
        return Err("plugin manifest name mismatch".into());
    }
    if manifest.renderer != entry_renderer {
        return Err("plugin manifest renderer entry mismatch".into());
    }
    let api_version = manifest.renderer_api_version.unwrap_or(1);
    if api_version != 1 && api_version != 2 {
        return Err("unsupported rendererApiVersion".into());
    }
    let permissions: Vec<String> = serde_json::from_str(&permissions_json).unwrap_or_default();

    // runtimePath：dev 态仓库 out/；打包态 exe 目录/resources/out/（tauri.conf resources）。
    let runtime_path = {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from));
        let candidates = [
            std::env::current_dir()
                .ok()
                .map(|d| d.join("out").join("plugin-frame").join("runtime.js")),
            exe_dir
                .clone()
                .map(|d| d.join("out/plugin-frame/runtime.js")),
            exe_dir.map(|d| d.join("resources/out/plugin-frame/runtime.js")),
            Some(PathBuf::from("out/plugin-frame/runtime.js")),
            std::env::current_dir()
                .ok()
                .and_then(|d| d.parent().map(|p| p.join("out/plugin-frame/runtime.js"))),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from("out/plugin-frame/runtime.js"))
    };

    let session = {
        let mut reg = protocol.registry.lock().unwrap();
        reg.create(crate::plugin_session::CreateSessionInput {
            initial_background: initial_background.into(),
            color_scheme: scheme.into(),
            plugin_id: id,
            plugin_name: name,
            plugin_directory: installed_path,
            renderer_entry: entry_renderer,
            runtime_path: runtime_path.to_string_lossy().into_owned(),
            renderer_api_version: api_version,
            permissions,
            owner_webview_label: "main".into(),
        })?
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

/// 插件宿主 → backend 消息转发（对等 plugin:send-message）。
/// 1.9.2-a：惰性 spawn sidecar（首次调用时），消息路由到 backend 的 onMessage。
/// 路由约定（1.9.11 起）：
/// - 前端 bridge 直传 pluginId（宿主 webview 是可信调用方；renderer 无法直接
///   invoke 命令，伪造面不存在）。旧版传 session token 的路径保留兼容：
///   64-hex id 先经 registry 反查 plugin_id。
/// - token 反查失败时返回明确的 SESSION_EXPIRED 错误（此前会落到 DB 查询报
///   "plugin not found: <hex>"，前端只能看到笼统 INTERNAL_ERROR —— Bug C 排障主因）。
#[tauri::command(async)]
pub fn plugin_send_message(
    window: WebviewWindow,
    backend: State<'_, Arc<crate::backend_process::BackendProcessManager>>,
    protocol: State<'_, Arc<crate::plugin_protocol::ProtocolContext>>,
    db: State<'_, Arc<Mutex<Db>>>,
    id: String,
    message: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    // 解析 plugin_id：64-hex 视为 session token 反查；否则按 plugin id 直传
    let mut plugin_id = id.clone();
    if id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        let access = protocol.registry.lock().unwrap().get(&id, "main");
        match (&access.ok, &access.session) {
            (true, Some(session)) => plugin_id = session.plugin_id.clone(),
            _ => {
                let reason = access
                    .reason
                    .map(|r| format!("{r:?}"))
                    .unwrap_or_else(|| "unknown".into());
                eprintln!(
                    "[plugin_send_message] session lookup failed for token prefix {}…: {reason}",
                    &id[..8.min(id.len())]
                );
                return Err(format!("SESSION_EXPIRED: renderer session no longer valid ({reason}); reopen the plugin"));
            }
        }
    }
    eprintln!(
        "[plugin_send_message] route plugin_id={plugin_id} type={}",
        message.get("type").and_then(|v| v.as_str()).unwrap_or("?")
    );
    // 惰性 spawn + 路由
    let record = {
        let db = lock(&db);
        db.plugin_backend_record(&plugin_id)
            .map_err(|e| format!("plugin lookup failed: {e}"))?
            .ok_or_else(|| format!("plugin not found: {plugin_id}"))?
    };
    let proc = backend.ensure_activated(&plugin_id, record)?;
    let result = proc.request("plugin.message", serde_json::json!({ "message": message }))?;
    Ok(result)
}

// ---------------------------------------------------------------------------
// db status（供前端诊断/基准）
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn db_status(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<crate::db::DbStatus, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let db = lock(&db);
    db.status().map_err(|e| e.to_string())
}

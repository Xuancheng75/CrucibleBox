// 1.8.1 核心 IPC 命令集（Tauri commands）
// 对等 electron/ipc/*.ts + settings/theme/plugin 读路径。
// 并发模型：所有 DB 命令用 #[tauri::command(async)] 跑在线程池（阻塞安全），
// 经 tauri::State<Mutex<Db>> 单连接串行化（与 better-sqlite3 单连接语义对等）。
// 安全模型（对等 electron/ipc/ipcGuard.ts assertTrustedSender + settings 白名单）：
// - 所有命令校验调用窗口为主窗口 main frame（label=main）
// - settings_set 仅允许白名单 key（当前仅 'theme'；对等 settings.ipc.ts）

use crate::db::Db;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State, Webview, WebviewWindow};
use tauri_plugin_updater::UpdaterExt;

/// 渲染进程可写 settings key 白名单（对等 electron/ipc/settings.ipc.ts）
const ALLOWED_SETTINGS_KEYS: &[&str] = &["theme", "updateChannel"];

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateMetadata {
    pub rid: tauri::ResourceId,
    pub current_version: String,
    pub version: String,
    pub date: Option<String>,
    pub body: Option<String>,
    pub raw_json: serde_json::Value,
}

/// 2.0 update-channel entry point.  The JS updater API only accepts headers
/// and timeout options, so endpoint selection must happen in the trusted host.
#[tauri::command]
pub async fn app_check_update(
    webview: Webview,
    channel: String,
    timeout_ms: Option<u64>,
) -> Result<Option<AppUpdateMetadata>, String> {
    if webview.label() != "main" {
        return Err("unauthorized".into());
    }
    let endpoint = match channel.as_str() {
        "stable" => {
            "https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-stable/latest.json"
        }
        "beta" => {
            "https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-beta/latest.json"
        }
        _ => return Err("unsupported update channel".into()),
    };
    let endpoint = tauri::Url::parse(endpoint).map_err(|error| error.to_string())?;
    let mut builder = webview
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?;
    if let Some(timeout_ms) = timeout_ms {
        builder = builder.timeout(std::time::Duration::from_millis(timeout_ms));
    }
    let updater = builder.build().map_err(|error| error.to_string())?;
    let update = updater.check().await.map_err(|error| error.to_string())?;
    Ok(update.map(|update| AppUpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date: update.date.map(|date| date.to_string()),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    }))
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
        install.recover_if_blocked(&record.name)?;
    }
    let _lifecycle = backend.begin_lifecycle_operation(&id)?;
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
        install.recover_if_blocked(&record.name)?;
    }
    let _lifecycle = backend.begin_lifecycle_operation(&id)?;
    let _maintenance = backend.enter_maintenance(&id)?;
    let deactivate_result = backend.deactivate(&id);
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

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketplaceCatalog {
    schema_version: u32,
    plugins: Vec<MarketplaceCatalogPlugin>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketplaceCatalogResponse {
    #[serde(flatten)]
    catalog: MarketplaceCatalog,
    source: String,
    stale: bool,
    fetched_at: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct MarketplaceCatalogPlugin {
    id: String,
    version: String,
    artifact: String,
    sha256: String,
    size: u64,
    url: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    min_host_version: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    highlights: Vec<String>,
}

const MARKETPLACE_MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;
const MARKETPLACE_DOWNLOAD_ATTEMPTS: usize = 3;
const MARKETPLACE_CATALOG_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

struct MarketplaceCatalogCache {
    channel: String,
    catalog: MarketplaceCatalog,
    source: String,
    fetched_at: u64,
    checked_at: Instant,
}

static MARKETPLACE_CATALOG_CACHE: OnceLock<Mutex<Option<MarketplaceCatalogCache>>> =
    OnceLock::new();

fn marketplace_catalog_cache() -> &'static Mutex<Option<MarketplaceCatalogCache>> {
    MARKETPLACE_CATALOG_CACHE.get_or_init(|| Mutex::new(None))
}

fn marketplace_agent(connect_secs: u64, read_secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        // Respect HTTP(S)_PROXY/NO_PROXY when a user or enterprise network
        // exposes GitHub through an explicit proxy.  Without this feature the
        // embedded client always connects directly, unlike the browser.
        .try_proxy_from_env(true)
        .timeout_connect(std::time::Duration::from_secs(connect_secs))
        .timeout_read(std::time::Duration::from_secs(read_secs))
        .build()
}

fn marketplace_catalog_urls(channel: &str) -> Vec<&'static str> {
    if channel == "beta" {
        vec![
            "https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-beta/plugins.json",
            "https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-stable/plugins.json",
        ]
    } else {
        vec!["https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-stable/plugins.json"]
    }
}

fn marketplace_channel(requested: Option<&str>) -> Result<String, String> {
    match requested {
        Some("stable") => Ok("stable".into()),
        Some("beta") => Ok("beta".into()),
        Some(_) => Err("unsupported marketplace channel".into()),
        None if env!("CARGO_PKG_VERSION").contains("-beta.")
            || env!("CARGO_PKG_VERSION").contains("-rc.") =>
        {
            Ok("beta".into())
        }
        None => Ok("stable".into()),
    }
}

fn marketplace_catalog_response(
    catalog: MarketplaceCatalog,
    source: String,
    stale: bool,
    fetched_at: u64,
) -> MarketplaceCatalogResponse {
    MarketplaceCatalogResponse {
        catalog,
        source,
        stale,
        fetched_at,
    }
}

fn fetch_marketplace_catalog(
    force_refresh: bool,
    requested_channel: Option<&str>,
) -> Result<MarketplaceCatalogResponse, String> {
    let channel = marketplace_channel(requested_channel)?;
    if !force_refresh {
        let cache = marketplace_catalog_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cache.as_ref().filter(|cached| {
            cached.channel == channel && cached.checked_at.elapsed() < MARKETPLACE_CATALOG_CACHE_TTL
        }) {
            return Ok(marketplace_catalog_response(
                cached.catalog.clone(),
                cached.source.clone(),
                false,
                cached.fetched_at,
            ));
        }
    }

    let agent = marketplace_agent(8, 20);
    let mut catalog_errors = Vec::new();
    for catalog_url in marketplace_catalog_urls(&channel) {
        for attempt in 1..=MARKETPLACE_DOWNLOAD_ATTEMPTS {
            let response = match agent
                .get(catalog_url)
                .set("Accept", "application/json")
                .set("Cache-Control", "no-cache")
                .set(
                    "User-Agent",
                    concat!("CrucibleBox/", env!("CARGO_PKG_VERSION")),
                )
                .call()
            {
                Ok(response) => response,
                Err(error) => {
                    catalog_errors.push(format!("{catalog_url}（第 {attempt} 次）：{error}"));
                    continue;
                }
            };
            let mut catalog_text = String::new();
            if let Err(error) = response
                .into_reader()
                .take(MARKETPLACE_MAX_CATALOG_BYTES + 1)
                .read_to_string(&mut catalog_text)
            {
                catalog_errors.push(format!("{catalog_url}（第 {attempt} 次）：{error}"));
                continue;
            }
            if catalog_text.len() as u64 > MARKETPLACE_MAX_CATALOG_BYTES {
                catalog_errors.push(format!("{catalog_url}：官方插件目录超过安全大小限制"));
                continue;
            }
            match serde_json::from_str::<MarketplaceCatalog>(&catalog_text) {
                Ok(value) if value.schema_version == 1 => {
                    let fetched_at = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_secs())
                        .unwrap_or_default();
                    marketplace_catalog_cache()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .replace(MarketplaceCatalogCache {
                            channel: channel.clone(),
                            catalog: value.clone(),
                            source: catalog_url.to_string(),
                            fetched_at,
                            checked_at: Instant::now(),
                        });
                    return Ok(marketplace_catalog_response(
                        value,
                        catalog_url.to_string(),
                        false,
                        fetched_at,
                    ));
                }
                Ok(_) => catalog_errors.push(format!("{catalog_url}：官方目录版本不受支持")),
                Err(error) => catalog_errors.push(format!("{catalog_url}：解析失败：{error}")),
            }
        }
    }

    let cache = marketplace_catalog_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.as_ref().filter(|cached| cached.channel == channel) {
        return Ok(marketplace_catalog_response(
            cached.catalog.clone(),
            cached.source.clone(),
            true,
            cached.fetched_at,
        ));
    }
    Err(format!(
        "读取官方插件目录失败：{}",
        catalog_errors.join("；")
    ))
}

/// Return the remote first-party catalog for the marketplace page.  The
/// frontend keeps its bundled catalog as a fast/offline fallback; this command
/// only enriches it with the latest version and artifact metadata.
#[tauri::command(async)]
pub fn marketplace_catalog(
    window: WebviewWindow,
    force_refresh: Option<bool>,
    channel: Option<String>,
) -> Result<Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let catalog = fetch_marketplace_catalog(force_refresh.unwrap_or(false), channel.as_deref())?;
    serde_json::to_value(catalog).map_err(|error| format!("序列化插件目录失败: {error}"))
}

/// Download a first-party plugin bundle from the signed release catalog.
/// Installation still goes through plugin_install_preview/commit, so this
/// command only materializes a digest-verified ZIP in a bounded temp folder.
#[tauri::command(async)]
pub fn marketplace_download_plugin(
    window: WebviewWindow,
    id: String,
    channel: Option<String>,
) -> Result<String, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    if id.is_empty()
        || id.len() > 100
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("invalid plugin id".into());
    }
    const MAX_PLUGIN_BYTES: u64 = 256 * 1024 * 1024;
    let agent = marketplace_agent(15, 45);
    let catalog = fetch_marketplace_catalog(false, channel.as_deref())?.catalog;
    if catalog.schema_version != 1 {
        return Err("unsupported marketplace catalog schema".into());
    }
    let plugin = catalog
        .plugins
        .into_iter()
        .find(|plugin| plugin.id == id)
        .ok_or_else(|| "官方目录中没有该插件".to_string())?;
    if plugin.size == 0 || plugin.size > MAX_PLUGIN_BYTES {
        return Err("插件包大小超出安全限制".into());
    }
    let expected_artifact = format!("{}-{}.zip", plugin.id, plugin.version);
    if plugin.artifact != expected_artifact
        || !plugin
            .url
            .starts_with("https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-v")
    {
        return Err("官方目录包含不受信任的下载地址".into());
    }
    let root = std::env::temp_dir().join("cruciblebox-marketplace");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let target = root.join(&plugin.artifact);
    let partial = root.join(format!(".{}.part", plugin.artifact));

    if let Ok((size, digest)) = sha256_file(&target) {
        if size == plugin.size && digest.eq_ignore_ascii_case(&plugin.sha256) {
            emit_marketplace_progress(
                &window,
                &plugin.artifact,
                plugin.size,
                plugin.size,
                "cached",
            );
            return Ok(target.to_string_lossy().into_owned());
        }
    }
    if let Ok((size, digest)) = sha256_file(&partial) {
        if size == plugin.size && digest.eq_ignore_ascii_case(&plugin.sha256) {
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
            std::fs::rename(&partial, &target)
                .map_err(|error| format!("无法复用已完成的插件包：{error}"))?;
            emit_marketplace_progress(
                &window,
                &plugin.artifact,
                plugin.size,
                plugin.size,
                "cached",
            );
            return Ok(target.to_string_lossy().into_owned());
        }
    }

    let mut last_download_error = String::from("下载插件失败");
    let mut download_completed = false;
    for attempt in 1..=MARKETPLACE_DOWNLOAD_ATTEMPTS {
        let partial_size = std::fs::metadata(&partial)
            .ok()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        if partial_size > plugin.size {
            let _ = std::fs::remove_file(&partial);
        }
        let partial_size = std::fs::metadata(&partial)
            .ok()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        let mut request = agent.get(&plugin.url);
        if partial_size > 0 {
            request = request.set("Range", &format!("bytes={partial_size}-"));
        }
        let response = match request
            .set("Accept", "application/octet-stream")
            .set(
                "User-Agent",
                concat!("CrucibleBox/", env!("CARGO_PKG_VERSION")),
            )
            .call()
        {
            Ok(response) => response,
            Err(error) => {
                last_download_error = format!("下载插件失败（第 {attempt} 次）：{error}");
                continue;
            }
        };
        if !response.get_url().starts_with("https://") {
            last_download_error = "下载响应不是 HTTPS 地址".into();
            continue;
        }
        let resumed = partial_size > 0
            && response.status() == 206
            && response
                .header("Content-Range")
                .is_some_and(|value| value.starts_with(&format!("bytes {partial_size}-")));
        let offset = if resumed { partial_size } else { 0 };
        if partial_size > 0 && !resumed {
            let _ = std::fs::remove_file(&partial);
        }
        if response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|size| {
                size > plugin.size.saturating_sub(offset) || size > MAX_PLUGIN_BYTES
            })
        {
            last_download_error = "下载响应超过目录声明大小".into();
            continue;
        }
        let mut reader = response.into_reader();
        let mut file = match if resumed {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial)
        } else {
            std::fs::File::create(&partial)
        } {
            Ok(file) => file,
            Err(error) => return Err(format!("无法创建下载临时文件：{error}")),
        };
        let mut hasher = Sha256::new();
        let mut total = offset;
        if resumed {
            let mut existing = match std::fs::File::open(&partial) {
                Ok(file) => file,
                Err(error) => return Err(format!("无法读取断点文件：{error}")),
            };
            let mut existing_buffer = [0_u8; 64 * 1024];
            loop {
                let read = existing
                    .read(&mut existing_buffer)
                    .map_err(|error| format!("无法读取断点文件：{error}"))?;
                if read == 0 {
                    break;
                }
                hasher.update(&existing_buffer[..read]);
            }
        }
        let mut buffer = [0_u8; 64 * 1024];
        let mut read_error = None;
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(read) => read,
                Err(error) => {
                    read_error = Some(error.to_string());
                    break;
                }
            };
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > plugin.size || total > MAX_PLUGIN_BYTES {
                read_error = Some("下载内容超过目录声明大小".into());
                break;
            }
            hasher.update(&buffer[..read]);
            if let Err(error) = file.write_all(&buffer[..read]) {
                read_error = Some(error.to_string());
                break;
            }
            emit_marketplace_progress(&window, &plugin.artifact, total, plugin.size, "downloading");
        }
        if let Some(error) = read_error {
            last_download_error = format!("下载插件失败（第 {attempt} 次）：{error}");
            continue;
        }
        if let Err(error) = file.sync_all() {
            last_download_error = format!("保存插件包失败：{error}");
            continue;
        }
        if total != plugin.size
            || !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&plugin.sha256)
        {
            last_download_error = "插件包完整性校验失败".into();
            let _ = std::fs::remove_file(&partial);
            continue;
        }
        download_completed = true;
        break;
    }
    if !download_completed {
        return Err(last_download_error);
    }
    if target.exists() {
        std::fs::remove_file(&target).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

fn sha256_file(path: &std::path::Path) -> Result<(u64, String), String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

fn emit_marketplace_progress(
    window: &WebviewWindow,
    artifact: &str,
    downloaded: u64,
    total: u64,
    stage: &str,
) {
    let _ = window.emit(
        "marketplace:progress",
        serde_json::json!({
            "artifact": artifact,
            "downloaded": downloaded,
            "total": total,
            "stage": stage
        }),
    );
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

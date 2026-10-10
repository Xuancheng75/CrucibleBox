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
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State, Webview, WebviewWindow};
use tauri_plugin_updater::UpdaterExt;

/// 渲染进程可写 settings key 白名单（对等 electron/ipc/settings.ipc.ts）
#[tauri::command]
pub fn window_apply_theme(
    window: WebviewWindow,
    dark: bool,
    caption: u32,
    text: u32,
) -> Result<(), String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    if caption > 0xFFFFFF || text > 0xFFFFFF {
        return Err("invalid title bar color".into());
    }
    window
        .set_theme(Some(if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }))
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
        };
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0;
        for (attribute, color) in [(DWMWA_CAPTION_COLOR, caption), (DWMWA_TEXT_COLOR, text)] {
            // Windows 10 supports light/dark mode but not Windows 11 caption colors.
            let result = unsafe {
                DwmSetWindowAttribute(
                    hwnd as _,
                    attribute as u32,
                    (&color as *const u32).cast(),
                    std::mem::size_of::<u32>() as u32,
                )
            };
            if result < 0 && result != 0x80070057u32 as i32 {
                return Err(format!("title bar color failed: {result:#x}"));
            }
        }
    }
    Ok(())
}

const ALLOWED_SETTINGS_KEYS: &[&str] = &[
    "theme",
    "updateChannel",
    "downloadProxyMode",
    "downloadProxyUrl",
];

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
    let db_state = db.inner().clone();
    let db = lock(&db_state);
    db.setting_get(&key).map_err(|e| e.to_string())
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
    if key == "downloadProxyMode" {
        crate::network_policy::validate(
            &value,
            crate::network_policy::current().proxy_url.as_deref(),
        )?;
    }
    if key == "downloadProxyUrl" {
        crate::network_policy::validate(
            &crate::network_policy::current().mode,
            Some(value.as_str()),
        )?;
    }
    let db_state = db.inner().clone();
    let db = lock(&db_state);
    db.setting_set(&key, &value).map_err(|e| e.to_string())?;
    drop(db);
    crate::network_policy::reload(&db_state);
    // 对等 TS：set 成功恒返回 true（含同值更新）
    Ok(true)
}

#[tauri::command(async)]
pub fn network_diagnose(
    window: WebviewWindow,
) -> Result<crate::network_policy::NetworkDiagnostic, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    Ok(crate::network_policy::diagnose())
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
    db.settings_all().map_err(|e| e.to_string())
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

#[tauri::command]
pub fn app_fault_history(window: WebviewWindow) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let (path, text) = crate::diagnostics::read()?;
    Ok(serde_json::json!({ "path": path, "text": text }))
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
    pub network_route: String,
}

/// 2.0 update-channel entry point.  The JS updater API only accepts headers
/// and timeout options, so endpoint selection must happen in the trusted host.
#[tauri::command]
pub async fn app_check_update(
    webview: Webview,
    db: State<'_, Arc<Mutex<Db>>>,
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
        .endpoints(vec![endpoint.clone()])
        .map_err(|error| error.to_string())?;
    let network_policy = marketplace_network_policy(&db);
    let network_route = network_policy.route();
    let resolved_route = network_policy.resolve(endpoint.as_str())?;
    if let Some(proxy_url) = resolved_route.proxy_endpoint.as_deref() {
        let proxy =
            tauri::Url::parse(proxy_url).map_err(|error| format!("手动代理地址无效：{error}"))?;
        builder = builder.proxy(proxy);
    } else {
        builder = builder.no_proxy();
    }
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
        network_route: network_route.clone(),
        rid: webview.resources_table().add(update),
    }))
}

// ---------------------------------------------------------------------------
// plugins（对等 plugin.ipc.ts 读路径：list/get）
// ---------------------------------------------------------------------------

pub use cruciblebox_repository::management::PluginMetaDto;

#[tauri::command(async)]
pub fn plugin_list(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<PluginMetaDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).metadata_list()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandContributionDto {
    plugin_id: String,
    id: String,
    title: String,
    keywords: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFileHandlerDto {
    plugin_id: String,
    extensions: Vec<String>,
}

#[tauri::command(async)]
pub fn plugin_file_handlers_list(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<PluginFileHandlerDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let installations = lock(&db).enabled_plugin_installations()?;
    let mut handlers = Vec::new();
    for (plugin_id, installed_path) in installations {
        let manifest_path = std::path::Path::new(&installed_path).join("plugin.json");
        let Ok(manifest) = crate::manifest::read_manifest(&manifest_path) else {
            continue;
        };
        if manifest.manifest_version != Some(4) {
            continue;
        }
        let Some(items) = manifest
            .contributes
            .get("fileHandlers")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        let extensions = items
            .iter()
            .flat_map(|item| {
                item.get("extensions")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter_map(serde_json::Value::as_str)
            .map(|value| value.trim_start_matches('.').to_ascii_lowercase())
            .filter(|value| {
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
            .collect::<Vec<_>>();
        if !extensions.is_empty() {
            handlers.push(PluginFileHandlerDto {
                plugin_id,
                extensions,
            });
        }
    }
    Ok(handlers)
}

#[tauri::command(async)]
pub fn plugin_command_contributions_list(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<PluginCommandContributionDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let installations = lock(&db).enabled_plugin_installations()?;
    let mut commands = Vec::new();
    for (plugin_id, installed_path) in installations {
        let manifest_path = std::path::Path::new(&installed_path).join("plugin.json");
        let Ok(manifest) = crate::manifest::read_manifest(&manifest_path) else {
            continue;
        };
        if manifest.manifest_version != Some(4) {
            continue;
        }
        let Some(items) = manifest
            .contributes
            .get("commands")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for item in items {
            let (Some(id), Some(title)) = (
                item.get("id").and_then(serde_json::Value::as_str),
                item.get("title").and_then(serde_json::Value::as_str),
            ) else {
                continue;
            };
            if id.is_empty() || title.is_empty() {
                continue;
            }
            let keywords = item
                .get("keywords")
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            commands.push(PluginCommandContributionDto {
                plugin_id: plugin_id.clone(),
                id: id.to_owned(),
                title: title.to_owned(),
                keywords,
            });
        }
    }
    Ok(commands)
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
    lock(&db).metadata_get(id)
}

pub use cruciblebox_repository::management::UserPluginTagDto;

#[tauri::command(async)]
pub fn plugin_tags_list(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<UserPluginTagDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).tags_list()
}

#[tauri::command(async)]
pub fn plugin_tags_create(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    name: String,
) -> Result<i64, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).tag_create(name)
}

#[tauri::command(async)]
pub fn plugin_tags_rename(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
    name: String,
) -> Result<(), String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).tag_rename(id, name)
}

#[tauri::command(async)]
pub fn plugin_tags_delete(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
) -> Result<(), String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).tag_delete(id)
}

#[tauri::command(async)]
pub fn plugin_tags_assign(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    plugin_ids: Vec<String>,
    tag_ids: Vec<i64>,
) -> Result<(), String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).tags_assign(plugin_ids, tag_ids)
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
    next_backend: State<'_, Arc<crate::next_backend::Manager>>,
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
    let manifest =
        crate::manifest::read_manifest(std::path::Path::new(&activation_record.installed_path));
    let next = manifest
        .as_ref()
        .ok()
        .filter(|manifest| manifest.manifest_version == Some(5));
    let result = match next {
        Some(manifest) if manifest.backend == Some(false) => Ok(()),
        Some(_) => next_backend.activate(&id),
        None => Err(match manifest {
            Err(error) => format!("插件包读取失败：{error}"),
            Ok(_) => "此插件是旧架构版本，请在插件市场升级到 Next 版本。原插件包和用户数据已保留。"
                .into(),
        }),
    };
    if let Err(error) = result {
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
    next_backend: State<'_, Arc<crate::next_backend::Manager>>,
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
    let _next_maintenance = next_backend.begin_maintenance(&id)?;
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
    backend: State<'_, Arc<crate::backend_process::BackendProcessManager>>,
    id: String,
    config: serde_json::Value,
    db: State<'_, Arc<Mutex<Db>>>,
    next_backend: State<'_, Arc<crate::next_backend::Manager>>,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let record = lock(&db)
        .plugin_find_by_id(&id)?
        .ok_or("plugin not found")?;
    let manifest = crate::manifest::read_manifest(std::path::Path::new(&record.installed_path))?;
    if manifest.manifest_version != Some(5) {
        return Err("LEGACY_RUNTIME_RETIRED".into());
    }
    let _lifecycle = backend.begin_lifecycle_operation(&id)?;
    let _maintenance = next_backend.begin_maintenance(&id)?;
    lock(&db)
        .plugin_update_config(
            &id,
            &serde_json::to_string(&config).map_err(|e| e.to_string())?,
        )
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
    pub catalog: MarketplaceCatalog,
    pub source: String,
    pub stale: bool,
    pub fetched_at: u64,
    pub network_route: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct MarketplaceCatalogPlugin {
    pub id: String,
    pub version: String,
    pub artifact: String,
    pub size: u64,
    pub url: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub min_host_version: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub highlights: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

pub use cruciblebox_repository::management::MarketplaceSourceDto;

pub use cruciblebox_repository::management::PluginMarketplaceOriginDto;

#[tauri::command(async)]
pub fn marketplace_origins_list(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<PluginMarketplaceOriginDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).marketplace_origins()
}

fn marketplace_source(db: &Arc<Mutex<Db>>, id: i64) -> Result<MarketplaceSourceDto, String> {
    lock(db).marketplace_source(id)
}

#[tauri::command(async)]
pub fn marketplace_sources_list(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<MarketplaceSourceDto>, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).marketplace_sources()
}

#[tauri::command(async)]
pub fn marketplace_sources_add(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    name: String,
    url: String,
) -> Result<i64, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).marketplace_source_add(name, url)
}

#[tauri::command(async)]
pub fn marketplace_sources_set_enabled(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).marketplace_source_enable(id, enabled)
}

#[tauri::command(async)]
pub fn marketplace_sources_delete(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
) -> Result<(), String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    lock(&db).marketplace_source_delete(id)
}

const MARKETPLACE_MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;
const MARKETPLACE_DOWNLOAD_ATTEMPTS: usize = 3;
const MARKETPLACE_RETRY_BASE_DELAY: Duration = Duration::from_millis(750);
const MARKETPLACE_CATALOG_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

struct MarketplaceCatalogCache {
    channel: String,
    network_route: String,
    catalog: MarketplaceCatalog,
    source: String,
    fetched_at: u64,
    checked_at: Instant,
}

fn read_catalog_cache(
    db: &Arc<Mutex<Db>>,
    source_key: &str,
    channel: &str,
) -> Option<(MarketplaceCatalog, String, u64)> {
    let db = db.lock().ok()?;
    let (json, url, fetched_at) = db
        .marketplace_cache_read(source_key, channel)
        .ok()
        .flatten()?;
    let catalog: MarketplaceCatalog = serde_json::from_str(&json).ok()?;
    matches!(catalog.schema_version, 1 | 2).then_some((catalog, url, fetched_at.max(0) as u64))
}

fn write_catalog_cache(
    db: &Arc<Mutex<Db>>,
    source_key: &str,
    channel: &str,
    catalog: &MarketplaceCatalog,
    url: &str,
    fetched_at: u64,
) -> Result<(), String> {
    let json = serde_json::to_string(catalog).map_err(|error| error.to_string())?;
    let db = db.lock().map_err(|error| error.to_string())?;
    db.marketplace_cache_write(source_key, channel, &json, url, fetched_at)
}

static MARKETPLACE_CATALOG_CACHE: OnceLock<Mutex<Option<MarketplaceCatalogCache>>> =
    OnceLock::new();

fn marketplace_catalog_cache() -> &'static Mutex<Option<MarketplaceCatalogCache>> {
    MARKETPLACE_CATALOG_CACHE.get_or_init(|| Mutex::new(None))
}

#[tauri::command]
pub fn marketplace_cancel_task(window: WebviewWindow, task_id: String) -> Result<bool, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let runtime = window
        .try_state::<Arc<crate::task_runtime::TaskRuntime>>()
        .ok_or("task runtime unavailable")?;
    Ok(runtime.cancel("marketplace", &task_id))
}

type MarketplaceNetworkPolicy = crate::network_policy::NetworkPolicy;

fn marketplace_network_policy(db: &Arc<Mutex<Db>>) -> MarketplaceNetworkPolicy {
    crate::network_policy::reload(db);
    crate::network_policy::current()
}

fn marketplace_agent(
    url: &str,
    connect_secs: u64,
    read_secs: u64,
    policy: &MarketplaceNetworkPolicy,
) -> Result<ureq::Agent, String> {
    policy
        .agent_for_url(url, connect_secs, read_secs)
        .map(|(agent, _)| agent)
}

fn marketplace_catalog_urls(channel: &str) -> Vec<String> {
    let rolling_tag = if channel == "beta" {
        "tauri-beta"
    } else {
        "tauri-stable"
    };
    vec![format!(
        "https://github.com/Xuancheng75/CrucibleBox/releases/download/{rolling_tag}/plugins.json"
    )]
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
    network_route: String,
) -> MarketplaceCatalogResponse {
    MarketplaceCatalogResponse {
        catalog,
        source,
        stale,
        fetched_at,
        network_route,
    }
}

fn fetch_marketplace_catalog(
    db: &Arc<Mutex<Db>>,
    force_refresh: bool,
    requested_channel: Option<&str>,
    policy: &MarketplaceNetworkPolicy,
) -> Result<MarketplaceCatalogResponse, String> {
    let channel = marketplace_channel(requested_channel)?;
    if !force_refresh {
        if let Some((catalog, source, fetched_at)) = read_catalog_cache(db, "official", &channel) {
            let age = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs().saturating_sub(fetched_at))
                .unwrap_or(u64::MAX);
            if age < MARKETPLACE_CATALOG_CACHE_TTL.as_secs() {
                return Ok(marketplace_catalog_response(
                    catalog,
                    source,
                    false,
                    fetched_at,
                    policy.route(),
                ));
            }
        }
        let cache = marketplace_catalog_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cache.as_ref().filter(|cached| {
            cached.channel == channel
                && cached.network_route == policy.cache_key()
                && cached.checked_at.elapsed() < MARKETPLACE_CATALOG_CACHE_TTL
        }) {
            return Ok(marketplace_catalog_response(
                cached.catalog.clone(),
                cached.source.clone(),
                false,
                cached.fetched_at,
                policy.route(),
            ));
        }
    }

    let mut catalog_errors = Vec::new();
    let mut skip_ureq = false;

    #[cfg(windows)]
    if policy.mode != "manual" && !policy.uses_manual_proxy() {
        for catalog_url in marketplace_catalog_urls(&channel) {
            match crate::marketplace_transport::get_text(
                &catalog_url,
                MARKETPLACE_MAX_CATALOG_BYTES,
                policy.uses_system_proxy(),
            ) {
                Ok(catalog_text) => match serde_json::from_str::<MarketplaceCatalog>(&catalog_text)
                {
                    Ok(value) if matches!(value.schema_version, 1 | 2) => {
                        let fetched_at = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|duration| duration.as_secs())
                            .unwrap_or_default();
                        write_catalog_cache(
                            db,
                            "official",
                            &channel,
                            &value,
                            &catalog_url,
                            fetched_at,
                        )?;
                        marketplace_catalog_cache()
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .replace(MarketplaceCatalogCache {
                                channel: channel.clone(),
                                network_route: policy.cache_key(),
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
                            policy.route(),
                        ));
                    }
                    Ok(_) => catalog_errors.push(format!("{catalog_url}：官方目录版本不受支持")),
                    Err(error) => catalog_errors.push(format!("{catalog_url}：解析失败：{error}")),
                },
                Err(error) => {
                    let permanent = error.contains("响应状态异常：404");
                    catalog_errors.push(format!("{catalog_url}（WinHTTP）：{error}"));
                    if permanent {
                        skip_ureq = true;
                        break;
                    }
                }
            }
        }
    }

    if !skip_ureq {
        for catalog_url in marketplace_catalog_urls(&channel) {
            let agent = match marketplace_agent(&catalog_url, 8, 30, policy) {
                Ok(agent) => agent,
                Err(error) => {
                    catalog_errors.push(format!("{catalog_url}：路由解析失败：{error}"));
                    continue;
                }
            };
            for attempt in 1..=MARKETPLACE_DOWNLOAD_ATTEMPTS {
                let response = match agent
                    .get(&catalog_url)
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
                        let permanent = matches!(error, ureq::Error::Status(404, _));
                        catalog_errors.push(format!("{catalog_url}（第 {attempt} 次）：{error}"));
                        if permanent {
                            break;
                        }
                        if attempt < MARKETPLACE_DOWNLOAD_ATTEMPTS {
                            std::thread::sleep(MARKETPLACE_RETRY_BASE_DELAY * attempt as u32);
                        }
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
                    Ok(value) if matches!(value.schema_version, 1 | 2) => {
                        let fetched_at = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|duration| duration.as_secs())
                            .unwrap_or_default();
                        write_catalog_cache(
                            db,
                            "official",
                            &channel,
                            &value,
                            &catalog_url,
                            fetched_at,
                        )?;
                        marketplace_catalog_cache()
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .replace(MarketplaceCatalogCache {
                                channel: channel.clone(),
                                network_route: policy.cache_key(),
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
                            policy.route(),
                        ));
                    }
                    Ok(_) => catalog_errors.push(format!("{catalog_url}：官方目录版本不受支持")),
                    Err(error) => catalog_errors.push(format!("{catalog_url}：解析失败：{error}")),
                }
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
            policy.route(),
        ));
    }
    if let Some((catalog, source, fetched_at)) = read_catalog_cache(db, "official", &channel) {
        return Ok(marketplace_catalog_response(
            catalog,
            source,
            true,
            fetched_at,
            policy.route(),
        ));
    }
    Err(format!(
        "读取官方插件目录失败：{}",
        catalog_errors.join("；")
    ))
}

fn fetch_custom_marketplace_catalog(
    db: &Arc<Mutex<Db>>,
    url: &str,
    policy: &MarketplaceNetworkPolicy,
) -> Result<MarketplaceCatalogResponse, String> {
    let agent = match marketplace_agent(url, 8, 30, policy) {
        Ok(agent) => agent,
        Err(error) => {
            if let Some((catalog, source, fetched_at)) = read_catalog_cache(db, url, "custom") {
                return Ok(marketplace_catalog_response(
                    catalog,
                    source,
                    true,
                    fetched_at,
                    policy.route(),
                ));
            }
            return Err(error);
        }
    };
    let response = agent.get(url).set("Accept", "application/json").call();
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            if let Some((catalog, source, fetched_at)) = read_catalog_cache(db, url, "custom") {
                return Ok(marketplace_catalog_response(
                    catalog,
                    source,
                    true,
                    fetched_at,
                    policy.route(),
                ));
            }
            return Err(format!("读取第三方插件目录失败：{error}"));
        }
    };
    if !response.get_url().starts_with("https://") {
        return Err("插件目录响应必须使用 HTTPS".into());
    }
    let mut body = String::new();
    response
        .into_reader()
        .take(MARKETPLACE_MAX_CATALOG_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())?;
    if body.len() as u64 > MARKETPLACE_MAX_CATALOG_BYTES {
        return Err("插件目录过大".into());
    }
    let catalog: MarketplaceCatalog =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    if !matches!(catalog.schema_version, 1 | 2) {
        return Err("不支持此插件目录版本".into());
    }
    let fetched_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    write_catalog_cache(db, url, "custom", &catalog, url, fetched_at)?;
    Ok(marketplace_catalog_response(
        catalog,
        url.into(),
        false,
        fetched_at,
        policy.route(),
    ))
}

/// Return the remote first-party catalog for the marketplace page.  The
/// frontend keeps its bundled catalog as a fast/offline fallback; this command
/// only enriches it with the latest version and artifact metadata.
#[tauri::command(async)]
pub fn marketplace_catalog(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    force_refresh: Option<bool>,
    channel: Option<String>,
    source_id: Option<i64>,
) -> Result<Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let policy = marketplace_network_policy(&db);
    let catalog = if let Some(id) = source_id {
        let source = marketplace_source(&db, id)?;
        if !source.enabled {
            return Err("此插件目录已停用".into());
        }
        fetch_custom_marketplace_catalog(&db, &source.url, &policy)?
    } else {
        fetch_marketplace_catalog(
            &db,
            force_refresh.unwrap_or(false),
            channel.as_deref(),
            &policy,
        )?
    };
    serde_json::to_value(catalog).map_err(|error| format!("序列化插件目录失败: {error}"))
}

/// Download a first-party plugin bundle and hand the resulting ZIP to the
/// normal installation flow.
#[tauri::command(async)]
pub fn marketplace_download_plugin(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: String,
    channel: Option<String>,
    priority: Option<String>,
    task_id: Option<String>,
    source_id: Option<i64>,
) -> Result<String, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let runtime = window
        .try_state::<Arc<crate::task_runtime::TaskRuntime>>()
        .ok_or("task runtime unavailable")?
        .inner()
        .clone();
    let result = runtime
        .run_sync("marketplace", "download", task_id.as_deref(), |ctx| {
            let path = marketplace_download_execute(
                window.clone(),
                db.inner().clone(),
                id,
                channel,
                priority,
                Some(ctx.task_id()),
                source_id,
                ctx,
            )?;
            Ok(serde_json::json!({"path":path}))
        })
        .map_err(|error| {
            if error.contains("操作已取消") {
                "CANCELLED".to_owned()
            } else {
                error
            }
        })?;
    result["path"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "download result missing".into())
}
#[allow(clippy::too_many_arguments)]
fn marketplace_download_execute(
    window: WebviewWindow,
    db: Arc<Mutex<Db>>,
    id: String,
    channel: Option<String>,
    priority: Option<String>,
    task_id: Option<String>,
    source_id: Option<i64>,
    ctx: &crate::task_runtime::Context,
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
    let policy = marketplace_network_policy(&db);
    let catalog = if let Some(id) = source_id {
        let source = marketplace_source(&db, id)?;
        if !source.enabled {
            return Err("此插件目录已停用".into());
        }
        fetch_custom_marketplace_catalog(&db, &source.url, &policy)?.catalog
    } else {
        fetch_marketplace_catalog(&db, false, channel.as_deref(), &policy)?.catalog
    };
    if !matches!(catalog.schema_version, 1 | 2) {
        return Err("unsupported marketplace catalog schema".into());
    }
    let plugin = catalog
        .plugins
        .into_iter()
        .find(|plugin| plugin.id == id)
        .ok_or_else(|| "当前目录中没有该插件".to_string())?;
    if plugin.size == 0 || plugin.size > MAX_PLUGIN_BYTES {
        return Err("插件包大小超出安全限制".into());
    }
    let expected_artifact = format!("{}-{}.zip", plugin.id, plugin.version);
    if plugin.artifact != expected_artifact || !plugin.url.starts_with("https://") {
        return Err("目录中的插件下载地址或文件名无效".into());
    }
    let agent = marketplace_agent(&plugin.url, 15, 90, &policy)?;
    let root = std::env::temp_dir().join("cruciblebox-marketplace").join(
        source_id
            .map(|id| format!("source-{id}"))
            .unwrap_or_else(|| "official".into()),
    );
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let target = root.join(&plugin.artifact);
    let partial = root.join(format!(".{}.part", plugin.artifact));
    if ctx.is_cancelled() {
        let _ = std::fs::remove_file(&partial);
        return Err("CANCELLED".into());
    }

    if let Ok(metadata) = std::fs::metadata(&target) {
        if metadata.len() == plugin.size {
            emit_marketplace_progress(
                &window,
                &db,
                &plugin.artifact,
                plugin.size,
                plugin.size,
                "cached",
                task_id.as_deref(),
            );
            return Ok(target.to_string_lossy().into_owned());
        }
    }
    if let Ok(metadata) = std::fs::metadata(&partial) {
        if metadata.len() == plugin.size {
            publish_marketplace_stage(ctx, &partial, &target, plugin.size)?;
            emit_marketplace_progress(
                &window,
                &db,
                &plugin.artifact,
                plugin.size,
                plugin.size,
                "cached",
                task_id.as_deref(),
            );
            return Ok(target.to_string_lossy().into_owned());
        }
    }

    let mut last_download_error = String::from("下载插件失败");
    let mut download_completed = false;
    for attempt in 1..=MARKETPLACE_DOWNLOAD_ATTEMPTS {
        if ctx.is_cancelled() {
            let _ = std::fs::remove_file(&partial);
            return Err("CANCELLED".into());
        }
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

        // Prefer Windows BITS for a fresh transfer. The BITS job is explicitly
        // direct and owns transport retry/resume. Existing partial files stay
        // on the compatibility path so the byte-range verifier remains
        // available for recovery.
        if partial_size == 0
            && task_id.is_none()
            && policy.mode != "manual"
            && !policy.uses_manual_proxy()
        {
            let bits_result = crate::marketplace_download::download_with_bits(
                &plugin.url,
                &partial,
                &plugin.artifact,
                plugin.size,
                priority.as_deref() != Some("normal"),
                policy.uses_system_proxy(),
                |downloaded, total| {
                    emit_marketplace_progress(
                        &window,
                        &db,
                        &plugin.artifact,
                        downloaded,
                        total,
                        "downloading",
                        task_id.as_deref(),
                    );
                },
            );
            match bits_result {
                Ok(()) => {
                    if std::fs::metadata(&partial)
                        .ok()
                        .is_some_and(|metadata| metadata.len() == plugin.size)
                    {
                        download_completed = true;
                        break;
                    }
                    let _ = std::fs::remove_file(&partial);
                    last_download_error = "BITS 下载结果大小与目录记录不一致".into();
                    continue;
                }
                Err(error) => {
                    last_download_error = format!("BITS 下载不可用，将切换 Range 下载：{error}");
                }
            }
        }
        let mut request = agent.get(&plugin.url);
        if partial_size > 0 {
            request = request.set("Range", &format!("bytes={partial_size}-"));
        }
        let response = match request
            .set("Accept", "application/octet-stream")
            .set("Accept-Encoding", "identity")
            .set("Connection", "keep-alive")
            .set(
                "User-Agent",
                concat!("CrucibleBox/", env!("CARGO_PKG_VERSION")),
            )
            .call()
        {
            Ok(response) => response,
            Err(error) => {
                let permanent = matches!(error, ureq::Error::Status(404, _));
                last_download_error =
                    format!("官方 Release 下载失败（自动代理，第 {attempt} 次）：{error}");
                if permanent {
                    break;
                }
                if attempt < MARKETPLACE_DOWNLOAD_ATTEMPTS {
                    std::thread::sleep(MARKETPLACE_RETRY_BASE_DELAY * attempt as u32);
                }
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
        let mut total = offset;
        let mut buffer = [0_u8; 64 * 1024];
        let mut read_error = None;
        let mut last_progress_bytes = total;
        let mut last_progress_at = Instant::now();
        loop {
            if ctx.is_cancelled() {
                let _ = std::fs::remove_file(&partial);
                return Err("CANCELLED".into());
            }
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
            if let Err(error) = file.write_all(&buffer[..read]) {
                read_error = Some(error.to_string());
                break;
            }
            if total.saturating_sub(last_progress_bytes) >= 256 * 1024
                || last_progress_at.elapsed() >= Duration::from_millis(250)
                || total == plugin.size
            {
                emit_marketplace_progress(
                    &window,
                    &db,
                    &plugin.artifact,
                    total,
                    plugin.size,
                    "downloading",
                    task_id.as_deref(),
                );
                last_progress_bytes = total;
                last_progress_at = Instant::now();
            }
        }
        if let Some(error) = read_error {
            last_download_error = format!("下载插件失败（第 {attempt} 次）：{error}");
            if attempt < MARKETPLACE_DOWNLOAD_ATTEMPTS {
                std::thread::sleep(MARKETPLACE_RETRY_BASE_DELAY * attempt as u32);
            }
            continue;
        }
        if let Err(error) = file.sync_all() {
            last_download_error = format!("保存插件包失败：{error}");
            continue;
        }
        if total != plugin.size {
            last_download_error = "插件包大小与目录记录不一致".into();
            let _ = std::fs::remove_file(&partial);
            continue;
        }
        download_completed = true;
        break;
    }
    if !download_completed {
        return Err(last_download_error);
    }
    publish_marketplace_stage(ctx, &partial, &target, plugin.size)?;
    emit_marketplace_progress(
        &window,
        &db,
        &plugin.artifact,
        plugin.size,
        plugin.size,
        "downloaded",
        task_id.as_deref(),
    );
    Ok(target.to_string_lossy().into_owned())
}

fn publish_marketplace_stage(
    ctx: &crate::task_runtime::Context,
    partial: &std::path::Path,
    target: &std::path::Path,
    size: u64,
) -> Result<(), String> {
    ctx.check_cancelled()?;
    let transaction =
        crate::output_transaction::OutputTransaction::adopt_stage(target, partial, true)?;
    transaction.publish_durable(ctx, true, |stage| {
        if std::fs::metadata(stage).map_err(|e| e.to_string())?.len() != size {
            return Err("插件包大小与目录记录不一致".into());
        }
        Ok(())
    })?;
    Ok(())
}

fn emit_marketplace_progress(
    window: &WebviewWindow,
    _db: &Arc<Mutex<Db>>,
    artifact: &str,
    downloaded: u64,
    total: u64,
    stage: &str,
    task_id: Option<&str>,
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
    if let Some(task_id) = task_id {
        let percent = if total == 0 {
            0
        } else {
            downloaded.saturating_mul(70).saturating_div(total).min(70)
        };
        let detail = match stage {
            "cached" => "使用已下载文件",
            "downloaded" => "下载完成，准备安装",
            _ => "正在下载插件",
        };
        if stage != "downloaded" && stage != "cached" {
            if let Some(runtime) = window.try_state::<Arc<crate::task_runtime::TaskRuntime>>() {
                runtime.report_progress("marketplace", task_id, stage, percent as u32, detail);
            }
        }
    }
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
    db: State<'_, Arc<Mutex<Db>>>,
    token: String,
    marketplace_source_id: Option<i64>,
    task_id: Option<String>,
    runtime: State<'_, Arc<crate::task_runtime::TaskRuntime>>,
) -> Result<serde_json::Value, String> {
    if !is_main_window(&window) {
        return Err("unauthorized".into());
    }
    let result = install.commit_with_runtime(&runtime, token, task_id.as_deref())?;
    if let (Some(source_id), Some(plugin_id)) = (
        marketplace_source_id,
        result
            .get("data")
            .and_then(|data| data.get("id"))
            .and_then(Value::as_str),
    ) {
        let source_url = if source_id == 0 {
            "official".to_string()
        } else {
            marketplace_source(&db, source_id)?.url
        };
        let db = lock(&db);
        db.marketplace_origin_set(plugin_id, source_id, &source_url)
            .map_err(|error| format!("插件安装成功，但记录目录来源失败：{error}"))?;
    }
    Ok(result)
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
    let _ = (&db, &protocol, &id, &color_scheme);
    Err(
        "LEGACY_RUNTIME_RETIRED: plugin package and user data retained; install a Next plugin"
            .into(),
    )
}

/// 释放 renderer 会话（对等 disposeRendererSession）。
#[tauri::command(async)]
pub fn dispose_renderer_session(
    window: WebviewWindow,
    protocol: State<'_, std::sync::Arc<crate::plugin_protocol::ProtocolContext>>,
    gateway: State<'_, Mutex<crate::next_renderer::Gateway>>,
    token: String,
) -> Result<bool, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    let removed = protocol.registry.lock().unwrap().dispose(&token);
    gateway
        .lock()
        .map_err(|_| "INTERNAL_ERROR")?
        .forget_session(&token);
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
    let _ = (&backend, &protocol, &db, &id, &message);
    Err(
        "LEGACY_RUNTIME_RETIRED: plugin package and user data retained; install a Next plugin"
            .into(),
    )
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

#[cfg(test)]
mod tests {
    use super::{
        marketplace_catalog_urls, read_catalog_cache, write_catalog_cache, MarketplaceCatalog,
    };
    use crate::db::Db;
    use std::sync::{Arc, Mutex};

    #[test]
    fn catalog_cache_survives_db_reopen_and_separates_channels() {
        let path = std::env::temp_dir().join(format!(
            "cruciblebox-catalog-test-{}.db",
            crate::rand_token::random_token_hex().unwrap()
        ));
        let db = Arc::new(Mutex::new(Db::open(&path).unwrap()));
        let catalog = MarketplaceCatalog {
            schema_version: 2,
            plugins: Vec::new(),
        };
        write_catalog_cache(
            &db,
            "official",
            "beta",
            &catalog,
            "https://example.test/beta",
            42,
        )
        .unwrap();
        drop(db);
        let db = Arc::new(Mutex::new(Db::open(&path).unwrap()));
        assert!(read_catalog_cache(&db, "official", "stable").is_none());
        assert!(read_catalog_cache(&db, "third-party", "beta").is_none());
        let (loaded, source, fetched) = read_catalog_cache(&db, "official", "beta").unwrap();
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(source, "https://example.test/beta");
        assert_eq!(fetched, 42);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stable_marketplace_catalog_uses_its_channel() {
        let urls = marketplace_catalog_urls("stable");
        assert_eq!(urls.len(), 1);
        assert_eq!(
            urls[0],
            "https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-stable/plugins.json"
        );
    }

    #[test]
    fn beta_marketplace_catalog_is_channel_isolated() {
        let urls = marketplace_catalog_urls("beta");
        assert_eq!(urls, vec!["https://github.com/Xuancheng75/CrucibleBox/releases/download/tauri-beta/plugins.json"]);
    }
}

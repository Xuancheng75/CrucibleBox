// UniEnv trusted host service（1.9.9 阶段 A / 1.9.11 阶段 B）
// 对等 Electron 线 plugin-system/trusted-services/unienv/（已冻结）。本实现运行在宿主
// Rust 进程：sidecar 内插件经 api.invokeTrustedService('unienv', ...) → __hostRequest
// "trusted.invoke" → envelope_host::host_dispatch → 本模块。
//
// 阶段 B 范围（对齐冻结线 trusted-service.ts）：
//   只读：listTools / listVersions / listCombos(含自定义) / detect
//   任务：install / installCombo（installation 单飞、进度快照、取消、getTask 轮询）
//   内联：uninstall / switchVersion（互斥守卫，快速 FS 操作）
//   恢复：activate 时清理中断 staging；失败 fail-closed 拒绝后续写操作
// 配置来自插件 config_data（installRoot/downloadMirror/customCombos），每次请求现读。

use crate::db::Db;
use crate::unienv_install;
use crate::unienv_task::{TaskContext, TaskManager, INSTALLATION_RESOURCE};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_INSTALL_PATH_LENGTH: usize = 240;
const MAX_CUSTOM_COMBOS: usize = 20;
const MAX_COMBO_ITEMS: usize = 10;

fn tasks() -> &'static Arc<TaskManager> {
    static TASKS: OnceLock<Arc<TaskManager>> = OnceLock::new();
    TASKS.get_or_init(|| Arc::new(TaskManager::default()))
}

static RECOVERY_DONE: AtomicBool = AtomicBool::new(false);
static STARTUP_ERROR: OnceLock<String> = OnceLock::new();
static INLINE_MUTATION: Mutex<Option<String>> = Mutex::new(None);

fn err(code: &str, message: String) -> Value {
    json!({ "error": message, "code": code })
}

// ---------------------------------------------------------------------------
// 配置（对等 parseUniEnvConfig + canonicalizeInstallRoot）
// ---------------------------------------------------------------------------

pub struct ComboPack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub items: Vec<(String, String)>,
}

pub struct UniEnvConfig {
    pub install_root: PathBuf,
    pub download_mirror: String,
    pub custom_combos: Vec<ComboPack>,
    /// 联网检查语言新版本（默认开；不可达时静默回退内置目录）
    pub online_versions: bool,
}

fn has_control_characters(value: &str) -> bool {
    value.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
}

fn assert_safe_segment(segment: &str, label: &str) -> Result<(), String> {
    if segment.is_empty() || segment == ".." {
        return Err(format!("{label} contains an unsafe traversal segment"));
    }
    if segment == "." {
        return Ok(());
    }
    let invalid = |c: char| {
        matches!(
            c,
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '`' | '$' | '&' | '^' | '%' | '!' | ';'
        )
    };
    if has_control_characters(segment)
        || segment.chars().any(invalid)
        || segment.contains('/')
        || segment.contains('\\')
        || segment.ends_with('.')
        || segment.ends_with(' ')
    {
        return Err(format!(
            "{label} contains characters that are unsafe on Windows"
        ));
    }
    let lower = segment.to_ascii_lowercase();
    let reserved = ["con", "prn", "aux", "nul"];
    let reserved_device = lower.len() == 4
        && (lower.starts_with("com") || lower.starts_with("lpt"))
        && lower.as_bytes()[3].is_ascii_digit()
        && lower.as_bytes()[3] != b'0';
    if reserved.contains(&lower.as_str()) || reserved_device {
        return Err(format!("{label} uses a reserved Windows device name"));
    }
    Ok(())
}

/// 规范化安装根目录（对齐 path-policy.ts canonicalizeInstallRoot 的安全子集）：
/// 必须是形如 C:\UniEnv 的绝对盘符路径；拒绝 UNC/设备命名空间/盘根/危险字符。
pub fn canonicalize_install_root(value: &str) -> Result<PathBuf, String> {
    fn drive_absolute(v: &str) -> bool {
        let b = v.as_bytes();
        b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/')
    }
    if value.is_empty() || value.len() != value.trim_start().len() {
        return Err("Install root must be a non-empty string without leading whitespace".into());
    }
    if value.len() > MAX_INSTALL_PATH_LENGTH {
        return Err(format!(
            "Install root exceeds {MAX_INSTALL_PATH_LENGTH} characters"
        ));
    }
    if has_control_characters(value) {
        return Err("Install root contains control characters".into());
    }
    let unc_or_device = value.starts_with("\\\\")
        || value.starts_with("//")
        || value.to_ascii_lowercase().starts_with("/??/")
        || value.to_ascii_lowercase().starts_with("/device/");
    if unc_or_device {
        return Err("UNC and Windows device namespace paths are not supported".into());
    }
    if !drive_absolute(value) {
        return Err("Install root must be an absolute drive path such as C:\\UniEnv".into());
    }

    let portable = value.replace('/', "\\");
    for (index, segment) in portable[3..].split('\\').enumerate() {
        if segment.is_empty() {
            continue;
        }
        assert_safe_segment(segment, &format!("Install root segment {}", index + 1))
            .map_err(|e| format!("invalid-path: {e}"))?;
    }

    // 简易 normalize：合并分隔符与 . / ..（字符集已白名单化，逐段处理足够）
    let mut segments: Vec<String> = Vec::new();
    for segment in portable[3..].split('\\') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other.to_string()),
        }
    }
    let mut normalized = format!("{}:\\", portable[..1].to_ascii_uppercase());
    normalized.push_str(&segments.join("\\"));
    while normalized.len() > 3 && normalized.ends_with('\\') {
        normalized.pop();
    }
    if normalized.len() <= 3 {
        return Err("A drive root cannot be used as the installation root".into());
    }
    if normalized.len() > MAX_INSTALL_PATH_LENGTH {
        return Err(format!(
            "Canonical install root exceeds {MAX_INSTALL_PATH_LENGTH} characters"
        ));
    }
    Ok(PathBuf::from(normalized))
}

fn supported_version_list(tool: &str) -> Option<Vec<String>> {
    supported_versions_raw()
        .get(tool)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
}

pub fn is_supported_tool(tool: &str) -> bool {
    supported_version_list(tool).is_some()
}

pub fn is_supported_version(tool: &str, version: &str) -> bool {
    supported_version_list(tool)
        .map(|list| list.iter().any(|v| v == version))
        .unwrap_or(false)
}

/// 版本目录安全拼接（对齐 safeJoinVersionDirectory）：工具必须受支持，
/// 结果必须落在安装根之下且长度受限。`allow_dynamic` 用于在线安装的
/// 非内置版本（仍执行段级安全检查，仅跳过目录白名单）。
pub fn safe_join_version_dir_impl(
    install_root: &Path,
    tool: &str,
    version: &str,
    allow_dynamic: bool,
) -> Result<PathBuf, String> {
    if !is_supported_tool(tool) {
        return Err(format!("unknown-tool: Unsupported tool: {tool}"));
    }
    if !allow_dynamic && !is_supported_version(tool, version) {
        return Err(format!(
            "unknown-version: Unsupported {tool} version: {version}"
        ));
    }
    assert_safe_segment(tool, "Tool id").map_err(|e| format!("invalid-segment: {e}"))?;
    assert_safe_segment(version, "Version").map_err(|e| format!("invalid-segment: {e}"))?;
    let target = install_root.join(tool).join(version);
    let root_str = install_root.to_string_lossy();
    let target_str = target.to_string_lossy();
    if !target_str.starts_with(root_str.as_ref()) {
        return Err(
            "outside-root: Version directory is not a descendant of the installation root".into(),
        );
    }
    if target_str.len() > MAX_INSTALL_PATH_LENGTH {
        return Err(format!(
            "path-limit: Version directory exceeds {MAX_INSTALL_PATH_LENGTH} characters"
        ));
    }
    Ok(target)
}

pub fn safe_join_version_dir(
    install_root: &Path,
    tool: &str,
    version: &str,
) -> Result<PathBuf, String> {
    safe_join_version_dir_impl(install_root, tool, version, false)
}

/// 动态（在线）版本：跳过目录白名单，保留路径安全校验
pub fn safe_join_version_dir_dynamic(
    install_root: &Path,
    tool: &str,
    version: &str,
) -> Result<PathBuf, String> {
    safe_join_version_dir_impl(install_root, tool, version, true)
}

fn parse_custom_combos(value: Option<&Value>) -> Result<Vec<ComboPack>, String> {
    let Some(list) = value else {
        return Ok(Vec::new());
    };
    let arr = list
        .as_array()
        .ok_or("config.customCombos: must be an array")?;
    if arr.len() > MAX_CUSTOM_COMBOS {
        return Err(format!(
            "config.customCombos: exceeds max {MAX_CUSTOM_COMBOS} combos"
        ));
    }
    let mut out = Vec::new();
    for combo in arr {
        let obj = combo
            .as_object()
            .ok_or("config.customCombos[]: must be object")?;
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .ok_or("config.customCombos.id: must be string")?;
        let id_ok = !id.is_empty()
            && id.len() <= 64
            && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && id.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
            });
        if !id_ok {
            return Err(format!("config.customCombos.{id}: invalid combo id"));
        }
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .ok_or("config.customCombos.name: must be string")?;
        if name.is_empty() || name.len() > 80 {
            return Err(format!("config.customCombos.{id}: invalid name length"));
        }
        let description = obj.get("description").and_then(Value::as_str).unwrap_or("");
        if description.len() > 512 {
            return Err(format!("config.customCombos.{id}: description too long"));
        }
        let items_raw = obj
            .get("items")
            .and_then(Value::as_array)
            .ok_or("config.customCombos.items: must be array")?;
        if items_raw.is_empty() || items_raw.len() > MAX_COMBO_ITEMS {
            return Err(format!("config.customCombos.{id}: invalid item count"));
        }
        let mut items = Vec::new();
        for item in items_raw {
            let tool = item
                .get("toolId")
                .and_then(Value::as_str)
                .ok_or(format!("config.customCombos.{id}.toolId: must be string"))?;
            let version = item
                .get("version")
                .and_then(Value::as_str)
                .ok_or(format!("config.customCombos.{id}.version: must be string"))?;
            if !is_supported_tool(tool) {
                return Err(format!("config.customCombos.{id}: unknown tool {tool}"));
            }
            if !is_supported_version(tool, version) {
                return Err(format!(
                    "config.customCombos.{id}: unknown {tool} version {version}"
                ));
            }
            items.push((tool.to_string(), version.to_string()));
        }
        out.push(ComboPack {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            items,
        });
    }
    Ok(out)
}

/// 从插件 config_data 现读配置；记录缺失/解析失败时回退默认值。
fn load_config(db: &Db, plugin_id: &str) -> UniEnvConfig {
    let raw = db
        .conn()
        .lock()
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT config_data FROM plugins WHERE id = ?1",
                [plugin_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .unwrap_or_else(|| "{}".into());
    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    let mirror = parsed
        .get("downloadMirror")
        .and_then(Value::as_str)
        .unwrap_or("direct");
    let download_mirror = match mirror {
        "huawei" | "aliyun" | "tuna" => mirror.to_string(),
        _ => "direct".to_string(),
    };
    let install_root = parsed
        .get("installRoot")
        .and_then(Value::as_str)
        .and_then(|v| canonicalize_install_root(v).ok())
        .unwrap_or_else(|| PathBuf::from("C:\\UniEnv"));
    let custom_combos = parse_custom_combos(parsed.get("customCombos")).unwrap_or_default();
    // onlineVersions: "on"/"off"，默认 on
    let online_versions = parsed
        .get("onlineVersions")
        .and_then(Value::as_str)
        .map(|v| v != "off")
        .unwrap_or(true);
    UniEnvConfig {
        install_root,
        download_mirror,
        custom_combos,
        online_versions,
    }
}

// ---------------------------------------------------------------------------
// 静态元数据（阶段 A 移植，保持契约一致）
// ---------------------------------------------------------------------------

fn supported_versions_raw() -> Value {
    json!({
        "python": ["3.8.10", "3.9.13", "3.10.11", "3.11.9", "3.12.5", "3.14.7"],
        "node": ["16.20.2", "18.20.4", "20.15.1", "22.5.1", "24.18.1"],
        "git": ["2.43.0", "2.44.0", "2.45.2", "2.46.0", "2.54.0"],
        "go": ["1.21.6", "1.22.4", "1.23.0", "1.26.5"],
        "java": ["17.0.11", "17.0.12", "17.0.20", "21.0.3", "21.0.5", "21.0.12", "22.0.1", "25.0.4"],
        "rust": ["stable"],
        "php": ["8.3.33"]
    })
}

fn tool_meta() -> Value {
    json!([
        { "id": "python", "displayName": "Python", "icon": "\u{1F40D}", "description": "Python 编程语言运行时" },
        { "id": "node", "displayName": "Node.js", "icon": "\u{1F4E6}", "description": "JavaScript 运行时" },
        { "id": "git", "displayName": "Git", "icon": "\u{1F527}", "description": "分布式版本控制" },
        { "id": "go", "displayName": "Go", "icon": "\u{1F4C0}", "description": "Go 编程语言工具链" },
        { "id": "java", "displayName": "Java (JDK)", "icon": "\u{2615}\u{FE0F}", "description": "Java 开发工具包 (Temurin)" },
        { "id": "rust", "displayName": "Rust", "icon": "\u{1F980}", "description": "Rust 工具链（rustup，stable 通道）" },
        { "id": "php", "displayName": "PHP", "icon": "\u{1F418}", "description": "PHP 运行时（NTS x64）" }
    ])
}

fn builtin_combos() -> Value {
    json!([
        {
            "id": "python-fullstack", "name": "Python 全栈", "description": "Python 3.14 + Node 24 + Git",
            "items": [ { "toolId": "python", "version": "3.14.7" }, { "toolId": "node", "version": "24.18.1" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "java-dev", "name": "Java 开发", "description": "JDK 21 + Git",
            "items": [ { "toolId": "java", "version": "21.0.12" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "go-dev", "name": "Go 开发", "description": "Go 1.26 + Git",
            "items": [ { "toolId": "go", "version": "1.26.5" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "frontend-dev", "name": "前端开发", "description": "Node 24 + Git",
            "items": [ { "toolId": "node", "version": "24.18.1" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "fullstack-universal", "name": "全栈通用", "description": "Python 3.14 + Node 24 + Go 1.26 + Git",
            "items": [ { "toolId": "python", "version": "3.14.7" }, { "toolId": "node", "version": "24.18.1" }, { "toolId": "go", "version": "1.26.5" }, { "toolId": "git", "version": "2.54.0" } ]
        }
    ])
}

fn display_name(tool: &str) -> String {
    match tool {
        "python" => "Python".into(),
        "node" => "Node.js".into(),
        "git" => "Git".into(),
        "go" => "Go".into(),
        "java" => "Java (JDK)".into(),
        "rust" => "Rust".into(),
        "php" => "PHP".into(),
        other => other.to_string(),
    }
}

/// 版本列表合并：内置目录 ∪ 在线发现（仅 provider 支持且配置开启），降序去重。
/// 网络失败静默回退内置目录（unienv_versions 内部已 eprintln 诊断）。
fn merged_versions(tool: &str, cfg: &UniEnvConfig) -> Value {
    let mut list = supported_versions_raw()
        .get(tool)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if cfg.online_versions && crate::unienv_versions::provider_supports(tool) {
        for v in crate::unienv_versions::online_versions(tool) {
            if !list.contains(&v) {
                list.push(v);
            }
        }
        list.sort_by(|a, b| crate::unienv_versions::compare_version_desc(b, a));
    }
    json!(list)
}

/// 安装/切换前的版本解析：内置目录命中，或（配置开启且上游可校验时）在线版本。
fn resolve_version(cfg: &UniEnvConfig, tool: &str, version: &str) -> Result<String, Value> {
    if is_supported_version(tool, version) {
        return Ok(version.to_string());
    }
    let dynamic_allowed = cfg.online_versions && crate::unienv_versions::provider_supports(tool);
    if dynamic_allowed {
        let known = crate::unienv_versions::online_artifact(tool, version, &cfg.download_mirror);
        if known.is_ok() {
            return Ok(version.to_string());
        }
    }
    Err(err(
        "unknown-version",
        format!(
            "Unsupported {tool} version: {version}{}",
            if dynamic_allowed {
                String::new()
            } else {
                "（该工具暂不支持在线新版本，请等待插件更新）".to_string()
            }
        ),
    ))
}

// ---------------------------------------------------------------------------
// 启动恢复与互斥守卫（对等 recoverInterruptedInstalls / assertStartupRecoveryReady /
// runInlineMutation 的单飞语义）
// ---------------------------------------------------------------------------

fn all_version_roots(install_root: &Path) -> Vec<PathBuf> {
    let raw = supported_versions_raw();
    let mut roots = Vec::new();
    for (tool, versions) in raw.as_object().unwrap() {
        for version in versions.as_array().unwrap() {
            let version = version.as_str().unwrap();
            if let Ok(path) = safe_join_version_dir(install_root, tool, version) {
                roots.push(path);
            }
        }
    }
    roots
}

/// activate 时执行一次；失败记录到 STARTUP_ERROR 并让后续写操作 fail-closed。
fn ensure_recovery(db: &Db, plugin_id: &str) {
    if RECOVERY_DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    let cfg = load_config(db, plugin_id);
    match unienv_install::recover_interrupted_staging(&all_version_roots(&cfg.install_root)) {
        Ok(removed) if !removed.is_empty() => {
            eprintln!(
                "[unienv] cleaned {} interrupted staging dir(s)",
                removed.len()
            );
        }
        Ok(_) => {}
        Err(message) => {
            eprintln!("[unienv] startup recovery failed: {message}");
            let _ = STARTUP_ERROR.set(message);
        }
    }
}

fn assert_recovery_ready() -> Result<(), Value> {
    if let Some(error) = STARTUP_ERROR.get() {
        return Err(err(
            "startup-recovery-failed",
            format!("启动恢复未完成，拒绝修改环境: {error}"),
        ));
    }
    Ok(())
}

fn require_windows() -> Result<(), Value> {
    #[cfg(windows)]
    {
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(err(
            "unsupported-platform",
            "UniEnv 的环境检测与安装功能目前仅支持 Windows".into(),
        ))
    }
}

/// 轻量内联互斥：仅串行化 inline 写操作（install/installCombo 启动瞬间与
/// uninstall/switchVersion 执行期）。installation 单飞由 TaskManager resourceKey 负责。
fn try_begin_inline(label: &str) -> Result<InlineMutationGuard, Value> {
    {
        let current = INLINE_MUTATION.lock().unwrap();
        if let Some(active) = current.as_ref() {
            return Err(err(
                "mutation-conflict",
                format!("另一个写操作正在执行: {active}"),
            ));
        }
    }
    *INLINE_MUTATION.lock().unwrap() = Some(label.to_string());
    Ok(InlineMutationGuard)
}

/// 内联写操作完整守卫（对等 assertNoInlineMutation + assertNoInstallationTask）：
/// 额外拒绝在安装任务运行期间执行 uninstall/switchVersion。
fn guard_inline_mutation(label: &str) -> Result<InlineMutationGuard, Value> {
    if let Some(active) = tasks().active_task(INSTALLATION_RESOURCE) {
        return Err(err("task-conflict", format!("安装任务正在执行: {active}")));
    }
    try_begin_inline(label)
}

struct InlineMutationGuard;

impl Drop for InlineMutationGuard {
    fn drop(&mut self) {
        *INLINE_MUTATION.lock().unwrap() = None;
    }
}

/// install/installCombo 启动后立即释放 inline 守卫（任务自身由 installation
/// resourceKey 单飞保护）；uninstall/switchVersion 由 InlineMutationGuard::drop 释放。
fn end_inline() {
    *INLINE_MUTATION.lock().unwrap() = None;
}

// ---------------------------------------------------------------------------
// 安装任务 executor
// ---------------------------------------------------------------------------

fn progress_adapter<'a>(
    ctx: &'a TaskContext,
    prefix: Option<(String, usize, usize)>,
) -> impl Fn(&str, u32, &str) + Send + Sync + 'a {
    move |stage, percent, message| match &prefix {
        Some((combo_name, index, total)) => {
            let overall = (((*index as f64 + percent as f64 / 100.0) / *total as f64) * 100.0)
                .min(99.0)
                .round() as u32;
            ctx.update_progress(stage, overall, &format!("{combo_name} · {message}"));
        }
        None => ctx.update_progress(stage, percent, message),
    }
}

fn run_install_executor(
    ctx: &TaskContext,
    install_root: String,
    mirror: String,
    online_versions: bool,
    tool: String,
    version: String,
) -> Result<Value, String> {
    ctx.update_progress(
        "downloading",
        0,
        &format!("准备安装 {} {version}", display_name(&tool)),
    );
    let plan = install_plan(&mirror, online_versions, &tool, &version)?;
    unienv_install::install_with_plan(
        Path::new(&install_root),
        &tool,
        &version,
        &plan,
        &progress_adapter(ctx, None),
        ctx.cancel_flag(),
    )?;
    ctx.check_cancelled()?;
    Ok(json!({
        "kind": "install",
        "tool": tool,
        "version": version,
        "message": format!("{} {version} 安装完成", display_name(&tool)),
    }))
}

/// 构建安装计划：内置版本走静态目录；在线版本走上游元数据（SHA-256 权威）
fn install_plan(
    mirror: &str,
    online_versions: bool,
    tool: &str,
    version: &str,
) -> Result<unienv_install::InstallPlan, String> {
    if is_supported_version(tool, version) {
        return Ok(unienv_install::InstallPlan {
            urls: crate::unienv_catalog::download_urls(tool, version, mirror)?,
            sha256: crate::unienv_catalog::artifact(tool, version)?
                .sha256
                .to_string(),
            filename: crate::unienv_catalog::artifact(tool, version)?
                .filename
                .to_string(),
        });
    }
    if online_versions && crate::unienv_versions::provider_supports(tool) {
        let artifact = crate::unienv_versions::online_artifact(tool, version, mirror)?;
        let filename = artifact
            .urls
            .first()
            .and_then(|(url, _)| url.rsplit('/').next())
            .unwrap_or("artifact.bin")
            .to_string();
        return Ok(unienv_install::InstallPlan {
            urls: artifact.urls,
            sha256: artifact.sha256,
            filename,
        });
    }
    Err(format!(
        "unknown-version: Unsupported {tool} version: {version}"
    ))
}

fn run_combo_executor(
    ctx: &TaskContext,
    install_root: String,
    mirror: String,
    combo_id: String,
    combo_name: String,
    items: Vec<(String, String)>,
) -> Result<Value, String> {
    let total = items.len();
    let mut results: Vec<Value> = Vec::new();
    for (index, (tool, version)) in items.iter().enumerate() {
        ctx.check_cancelled()?;
        let adapter = progress_adapter(ctx, Some((combo_name.clone(), index, total)));
        match unienv_install::install_tool(
            Path::new(&install_root),
            tool,
            version,
            &mirror,
            &adapter,
            ctx.cancel_flag(),
        ) {
            Ok(()) => results.push(json!({
                "tool": display_name(tool),
                "success": true,
                "message": format!("{} {version} 安装成功", display_name(tool)),
            })),
            Err(message) => {
                if ctx.is_cancelled() {
                    return Err("操作已取消".into());
                }
                results.push(json!({
                    "tool": display_name(tool),
                    "success": false,
                    "message": message,
                }));
            }
        }
    }
    let success = results.iter().all(|r| r["success"] == true);
    let final_message = if success {
        format!("{combo_name} 全部安装完成")
    } else {
        format!("{combo_name} 部分安装失败")
    };
    ctx.update_progress("done", 100, &final_message);
    Ok(json!({
        "kind": "combo",
        "comboId": combo_id,
        "success": success,
        "results": results,
        "message": if success {
            format!("组合包“{combo_name}”全部安装完成")
        } else {
            format!("组合包“{combo_name}”部分安装失败")
        },
    }))
}

// ---------------------------------------------------------------------------
// 请求校验与分发
// ---------------------------------------------------------------------------

fn str_field<'a>(request: &'a Value, key: &str, max_len: usize) -> Result<Option<&'a str>, Value> {
    match request.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            if s.is_empty() || s.len() > max_len {
                Err(err(
                    "string-limit",
                    format!("{key} length must be 1..={max_len}"),
                ))
            } else {
                Ok(Some(s.as_str()))
            }
        }
        Some(_) => Err(err("invalid-value", format!("{key} must be a string"))),
    }
}

fn tool_field(request: &Value) -> Result<String, Value> {
    let tool = str_field(request, "tool", 16)?
        .ok_or_else(|| err("invalid-value", "missing field: tool".into()))?;
    if !is_supported_tool(tool) {
        return Err(err("unknown-tool", format!("unsupported tool: {tool}")));
    }
    Ok(tool.to_string())
}

/// 处理 unienv 'message' 操作。返回值始终是可序列化响应对象
/// （协议级错误以 { error, code } 内联返回，对齐冻结线 toErrorResponse 形状）。
fn handle_message(db: &Db, plugin_id: &str, payload: &Value) -> Value {
    let request = match payload {
        Value::Object(_) => payload,
        _ => return err("invalid-value", "message payload must be an object".into()),
    };
    let msg_type = request
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    match msg_type.as_str() {
        "listTools" => json!(tool_meta()),
        "listVersions" => {
            let tool = match tool_field(request) {
                Ok(t) => t,
                Err(e) => return e,
            };
            let cfg = load_config(db, plugin_id);
            json!(merged_versions(&tool, &cfg))
        }
        "listCombos" => {
            let cfg = load_config(db, plugin_id);
            let mut combos = builtin_combos().as_array().cloned().unwrap_or_default();
            for combo in cfg.custom_combos {
                combos.push(json!({
                    "id": combo.id,
                    "name": combo.name,
                    "description": combo.description,
                    "items": combo.items.iter().map(|(tool, version)| json!({
                        "toolId": tool,
                        "version": version,
                    })).collect::<Vec<_>>(),
                }));
            }
            json!(combos)
        }
        "detect" => {
            if let Err(e) = require_windows() {
                return e;
            }
            let tool = match tool_field(request) {
                Ok(t) => t,
                Err(e) => return e,
            };
            let cfg = load_config(db, plugin_id);
            detect_tool(&cfg.install_root.to_string_lossy(), &tool)
        }
        "install" => {
            if let Err(e) = preflight_mutation(db, plugin_id) {
                return e;
            }
            let tool = match tool_field(request) {
                Ok(t) => t,
                Err(e) => return e,
            };
            let raw_version = match str_field(request, "version", 32) {
                Ok(Some(v)) => v.to_string(),
                Ok(None) => return err("invalid-value", "missing field: version".into()),
                Err(e) => return e,
            };
            let cfg = load_config(db, plugin_id);
            let version = match resolve_version(&cfg, &tool, &raw_version) {
                Ok(v) => v,
                Err(e) => return e,
            };
            if let Err(message) = safe_join_version_dir_dynamic(&cfg.install_root, &tool, &version)
            {
                return err("invalid-path", message);
            }
            if let Err(e) = try_begin_inline(&format!("安装 {tool} {version}")) {
                return e;
            }
            let install_root = cfg.install_root.to_string_lossy().into_owned();
            let mirror = cfg.download_mirror.clone();
            let online = cfg.online_versions && crate::unienv_versions::provider_supports(&tool);
            let start = tasks().start(
                INSTALLATION_RESOURCE,
                Box::new(move |ctx| {
                    run_install_executor(ctx, install_root, mirror, online, tool, version)
                }),
            );
            end_inline();
            match start {
                Ok(task_id) => {
                    json!({ "success": true, "taskId": task_id, "message": "安装任务已创建" })
                }
                Err(conflict) => err("task-conflict", conflict),
            }
        }
        "installCombo" => {
            if let Err(e) = preflight_mutation(db, plugin_id) {
                return e;
            }
            let combo_id = match str_field(request, "comboId", 64) {
                Ok(Some(id)) => id.to_string(),
                Ok(None) => return err("invalid-value", "missing field: comboId".into()),
                Err(e) => return e,
            };
            let cfg = load_config(db, plugin_id);
            let found = find_combo(&cfg, &combo_id);
            let Some((combo_name, items)) = found else {
                return err("unknown-combo", format!("未知组合包: {combo_id}"));
            };
            for (tool, version) in &items {
                if let Err(message) = safe_join_version_dir(&cfg.install_root, tool, version) {
                    return err("invalid-path", message);
                }
            }
            if let Err(e) = try_begin_inline(&format!("组合包 {combo_id}")) {
                return e;
            }
            let install_root = cfg.install_root.to_string_lossy().into_owned();
            let mirror = cfg.download_mirror.clone();
            let start = tasks().start(
                INSTALLATION_RESOURCE,
                Box::new(move |ctx| {
                    run_combo_executor(ctx, install_root, mirror, combo_id, combo_name, items)
                }),
            );
            end_inline();
            match start {
                Ok(task_id) => {
                    json!({ "success": true, "taskId": task_id, "message": "组合安装任务已创建" })
                }
                Err(conflict) => err("task-conflict", conflict),
            }
        }
        "getTask" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id.to_string(),
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(e) => return e,
            };
            match tasks().get(&task_id) {
                Some(snapshot) => snapshot,
                None => err("task-not-found", "未找到指定任务".into()),
            }
        }
        "cancelTask" => {
            let task_id = match str_field(request, "taskId", 128) {
                Ok(Some(id)) => id.to_string(),
                Ok(None) => return err("invalid-value", "missing field: taskId".into()),
                Err(e) => return e,
            };
            if tasks().cancel(&task_id) {
                json!({ "success": true, "taskId": task_id })
            } else {
                err("task-not-cancellable", "任务不存在或已结束".into())
            }
        }
        "uninstall" => {
            if let Err(e) = preflight_mutation(db, plugin_id) {
                return e;
            }
            let tool = match tool_field(request) {
                Ok(t) => t,
                Err(e) => return e,
            };
            let cfg = load_config(db, plugin_id);
            let label = format!("卸载 {}", display_name(&tool));
            let guard = match guard_inline_mutation(&label) {
                Ok(g) => g,
                Err(e) => return e,
            };
            let result = unienv_install::uninstall_tool(&cfg.install_root, &tool);
            drop(guard);
            match result {
                Ok(()) => {
                    json!({ "success": true, "message": format!("{} 已卸载", display_name(&tool)) })
                }
                Err(message) => err("uninstall-failed", message),
            }
        }
        "switchVersion" => {
            if let Err(e) = preflight_mutation(db, plugin_id) {
                return e;
            }
            let tool = match tool_field(request) {
                Ok(t) => t,
                Err(e) => return e,
            };
            let raw_version = match str_field(request, "version", 32) {
                Ok(Some(v)) => v.to_string(),
                Ok(None) => return err("invalid-value", "missing field: version".into()),
                Err(e) => return e,
            };
            // 切换仅对已安装版本有意义：目录存在性由 switch_version 校验，
            // 此处允许动态版本（在线装的版本不在内置目录中）
            let version = raw_version;
            let cfg = load_config(db, plugin_id);
            if let Err(message) = safe_join_version_dir_dynamic(&cfg.install_root, &tool, &version)
            {
                return err("invalid-path", message);
            }
            let label = format!("切换 {}", display_name(&tool));
            let guard = match guard_inline_mutation(&label) {
                Ok(g) => g,
                Err(e) => return e,
            };
            let result = unienv_install::switch_version(&cfg.install_root, &tool, &version);
            drop(guard);
            match result {
                Ok(()) => {
                    json!({ "success": true, "message": format!("已切换到 {} {version}", display_name(&tool)) })
                }
                Err(message) => err("switch-failed", message),
            }
        }
        _ => err("unknown-type", format!("unknown message type: {msg_type}")),
    }
}

/// 写操作公共前置：Windows 限定 + 启动恢复 + fail-closed 断言。
fn preflight_mutation(db: &Db, plugin_id: &str) -> Result<(), Value> {
    require_windows()?;
    ensure_recovery(db, plugin_id);
    assert_recovery_ready()
}

/// builtin 优先、custom 兜底的组合包查找（对齐 getCombos(...).find）。
fn find_combo(cfg: &UniEnvConfig, combo_id: &str) -> Option<(String, Vec<(String, String)>)> {
    let builtin = builtin_combos()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|c| c["id"] == json!(combo_id))
        .map(|c| {
            (
                c["name"].as_str().unwrap_or("").to_string(),
                c["items"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|i| {
                        (
                            i["toolId"].as_str().unwrap_or("").to_string(),
                            i["version"].as_str().unwrap_or("").to_string(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        });
    builtin.or_else(|| {
        cfg.custom_combos
            .iter()
            .find(|c| c.id == combo_id)
            .map(|c| (c.name.clone(), c.items.clone()))
    })
}

/// 检测某工具是否已安装（语义逐项对齐冻结线 tools/*.ts detect）：
/// 1) 全局命令探测（PATH 上已有同名工具即视为已安装，path=''）
/// 2) junction 下工具 exe 运行 --version/-version 提取真实版本号
/// 3) 均失败 → 未安装
fn detect_tool(install_root: &str, tool: &str) -> Value {
    // 1) 全局 PATH 探测
    let global: Option<(&str, Vec<&str>, bool)> = match tool {
        "python" => Some(("python.exe", vec!["--version"], false)),
        "node" => Some(("node.exe", vec!["--version"], false)),
        "git" => Some(("git.exe", vec!["--version"], false)),
        "go" => Some(("go.exe", vec!["version"], false)),
        "java" => Some(("java.exe", vec!["-version"], true)),
        "rust" => Some(("rustc.exe", vec!["--version"], false)),
        "php" => Some(("php.exe", vec!["--version"], false)),
        _ => None,
    };
    if let Some((exe, args, from_stderr)) = global {
        if let Some(v) = unienv_install::probe_tool_version(Path::new(exe), &args, from_stderr) {
            return json!({ "installed": true, "version": v, "path": "" });
        }
    }

    // 2) 安装根 junction 下探测（各工具运行时布局见 runtime_subdir）
    let current = PathBuf::from(install_root).join(tool).join("current");
    let rel: Option<(&str, Vec<&str>, bool)> = match tool {
        "python" => Some(("python.exe", vec!["--version"], false)),
        "node" => Some(("node.exe", vec!["--version"], false)),
        "git" => Some(("bin\\git.exe", vec!["--version"], false)),
        "go" => Some(("bin\\go.exe", vec!["version"], false)),
        "java" => Some(("bin\\java.exe", vec!["-version"], true)),
        // rustup：cargo home 内的 rustc/cargo 代理
        "rust" => Some(("cargo\\bin\\rustc.exe", vec!["--version"], false)),
        // php zip 解压根
        "php" => Some(("runtime\\php.exe", vec!["--version"], false)),
        _ => None,
    };
    if let Some((rel_exe, args, from_stderr)) = rel {
        let exe = current.join(rel_exe);
        if exe.is_file() {
            if let Some(v) = unienv_install::probe_tool_version(&exe, &args, from_stderr) {
                return json!({
                    "installed": true,
                    "version": v,
                    "path": current.to_string_lossy(),
                });
            }
        }
    }
    json!({ "installed": false })
}

/// 统一入口：envelope_host::host_dispatch 分发 "trusted.invoke" 时调用。
/// params: { service, operation, payload? }。仅接受 service == "unienv"。
pub fn dispatch(
    db: &Db,
    plugin_id: &str,
    service: &str,
    operation: &str,
    payload: Option<&Value>,
) -> Result<Value, String> {
    if service != "unienv" {
        return Err(format!("unknown trusted service: {service}"));
    }
    if operation == "activate" {
        ensure_recovery(db, plugin_id);
        return Ok(Value::Null);
    }
    if operation == "deactivate" {
        tasks().cancel_all_active();
        return Ok(Value::Null);
    }
    if operation != "message" {
        return Err(format!("unknown trusted operation: {operation}"));
    }
    let payload = payload.ok_or_else(|| "message operation requires payload".to_string())?;
    Ok(handle_message(db, plugin_id, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempDb {
        dir: PathBuf,
        db: Db,
    }

    impl TempDb {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "cruciblebox-unienv-svc-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let db = Db::open(&dir.join("test.db")).unwrap();
            TempDb { dir, db }
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn list_tools_returns_meta() {
        let t = TempDb::new("meta");
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "listTools" })),
        )
        .unwrap();
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 7);
        assert!(arr.iter().any(|tool| tool["id"] == "python"));
        assert!(arr.iter().any(|tool| tool["id"] == "rust"));
        assert!(arr.iter().any(|tool| tool["id"] == "php"));
    }

    #[test]
    fn list_versions_known_and_unknown() {
        let t = TempDb::new("versions");
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "listVersions", "tool": "go" })),
        )
        .unwrap();
        assert!(out.as_array().unwrap().contains(&json!("1.26.5")));
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "listVersions", "tool": "ruby" })),
        )
        .unwrap();
        assert_eq!(out["code"], "unknown-tool");
    }

    #[test]
    fn list_combos_includes_custom_from_config() {
        let t = TempDb::new("combos");
        t.db.conn()
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path, enabled, permissions, config_schema, config_data)
                 VALUES ('unienv', 'unienv', '0.0.0', 'UniEnv', '', '', 1, '[]', '{}', ?1)",
                [json!({
                    "installRoot": "D:\\Tools\\UniEnv",
                    "downloadMirror": "tuna",
                    "customCombos": [
                        { "id": "my-combo", "name": "我的组合", "description": "", "items": [ { "toolId": "go", "version": "1.26.5" } ] }
                    ]
                })
                .to_string()],
            )
            .unwrap();
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "listCombos" })),
        )
        .unwrap();
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 6);
        assert!(arr.iter().any(|c| c["id"] == "my-combo"));
    }

    #[test]
    fn install_rejects_unknown_version_before_task_start() {
        let t = TempDb::new("badver");
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "install", "tool": "go", "version": "9.9.9" })),
        )
        .unwrap();
        assert_eq!(out["code"], "unknown-version");
        assert!(tasks().active_task(INSTALLATION_RESOURCE).is_none());
    }

    #[test]
    fn get_task_unknown_returns_not_found() {
        let t = TempDb::new("gettask");
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "getTask", "taskId": "deadbeef" })),
        )
        .unwrap();
        assert_eq!(out["code"], "task-not-found");
    }

    #[test]
    fn cancel_task_unknown_returns_not_cancellable() {
        let t = TempDb::new("cancel");
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "cancelTask", "taskId": "deadbeef" })),
        )
        .unwrap();
        assert_eq!(out["code"], "task-not-cancellable");
    }

    #[test]
    fn unknown_type_errors() {
        let t = TempDb::new("unknowntype");
        let out = dispatch(
            &t.db,
            "unienv",
            "unienv",
            "message",
            Some(&json!({ "type": "nope" })),
        )
        .unwrap();
        assert_eq!(out["code"], "unknown-type");
    }

    #[test]
    fn wrong_operation_rejected() {
        let t = TempDb::new("op");
        assert!(dispatch(&t.db, "unienv", "unienv", "other", None).is_err());
    }

    #[test]
    fn canonicalize_install_root_rules() {
        assert_eq!(
            canonicalize_install_root("d:/tools/env\\").unwrap(),
            PathBuf::from("D:\\tools\\env")
        );
        assert!(
            canonicalize_install_root("C:\\").is_err(),
            "drive root refused"
        );
        assert!(
            canonicalize_install_root("\\\\server\\share").is_err(),
            "UNC refused"
        );
        assert!(canonicalize_install_root("relative\\path").is_err());
        assert!(canonicalize_install_root("C:\\bad<>.dir").is_err());
        assert!(canonicalize_install_root(" C:\\UniEnv").is_err());
    }

    #[test]
    fn safe_join_rules() {
        let root = PathBuf::from("C:\\UniEnv");
        assert_eq!(
            safe_join_version_dir(&root, "go", "1.26.5").unwrap(),
            PathBuf::from("C:\\UniEnv\\go\\1.26.5")
        );
        assert!(safe_join_version_dir(&root, "ruby", "1.0").is_err());
        assert!(safe_join_version_dir(&root, "go", "9.9.9").is_err());
    }
}

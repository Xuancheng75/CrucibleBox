// 插件 manifest 结构 + 全量校验（1.9.3，对等 plugin-system/PluginManifestPolicy.ts）
// 语义：manifest 是插件安装的信任根；所有校验失败返回 Err(String) 并带字段路径。
// 约束：零 unwrap/expect（panic=abort），错误一律 Result<_, String>。
#![allow(dead_code)] // 1.9.3 安装链后续步骤接入后消除

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

use crate::permissions::ALL_PERMISSIONS;

/// plugin.json 最大字节数（对等 MAX_PLUGIN_MANIFEST_BYTES）
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// JS Number.MAX_SAFE_INTEGER（semver 数字段上限）
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// 顶层字段白名单（对等 assertKeys 的 allowed 列表）
const TOP_LEVEL_KEYS: &[&str] = &[
    "name",
    "version",
    "displayName",
    "description",
    "author",
    "icon",
    "main",
    "renderer",
    "backend",
    "manifestVersion",
    "backendApiVersion",
    "rendererApiVersion",
    "minHostVersion",
    "permissions",
    "config",
];

/// 原型污染防护键（对等 FORBIDDEN_KEYS）
const FORBIDDEN_KEYS: &[&str] = &["__proto__", "prototype", "constructor"];

/// config 字段允许的键（对等 assertKeys 的 allowed 列表）
const CONFIG_FIELD_KEYS: &[&str] = &[
    "type",
    "label",
    "description",
    "default",
    "required",
    "options",
];

/// config 字段允许的类型（对等 CONFIG_FIELD_TYPES）
const CONFIG_FIELD_TYPES: &[&str] = &["string", "number", "boolean", "select", "multiselect"];

/// 插件 manifest（对等 PluginManifest 类型）
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Manifest {
    pub manifest_version: Option<u8>, // 1 | 2
    pub name: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub icon: Option<String>,
    pub main: String,
    pub renderer: String,
    pub backend: Option<bool>,
    pub backend_api_version: Option<u8>,
    pub renderer_api_version: Option<u8>,
    pub min_host_version: Option<String>,
    pub permissions: Vec<String>,
    pub config: serde_json::Map<String, serde_json::Value>,
}

/// 全量校验并解析 manifest 文本（对等 parsePluginManifest）
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("manifest: plugin.json is not valid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "manifest: must be a plain object".to_string())?;

    // 规则 1：顶层字段白名单 + 原型污染键拒绝
    for key in object.keys() {
        if FORBIDDEN_KEYS.contains(&key.as_str()) || !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(format!("manifest.{key}: is not a supported field"));
        }
    }

    // 规则 2：name 必填，string ≤64，匹配 ^[a-z0-9][a-z0-9_-]{0,63}$
    let name = get_string(object, "name", "manifest.name")?;
    validate_string(&name, "manifest.name", 64, false)?;
    if !is_valid_plugin_name(&name) {
        return Err("manifest.name: has an invalid format".to_string());
    }

    // 规则 3：version 必填，string ≤100，严格 semver
    let version = get_string(object, "version", "manifest.version")?;
    validate_string(&version, "manifest.version", 100, false)?;
    parse_semver(&version).map_err(|error| format!("manifest.version: {error}"))?;

    // 规则 4：manifestVersion/backendApiVersion/rendererApiVersion 可选，必须为 1 或 2
    let manifest_version = get_api_version(object, "manifestVersion", "manifest.manifestVersion")?;
    let backend_api_version =
        get_api_version(object, "backendApiVersion", "manifest.backendApiVersion")?;
    let renderer_api_version =
        get_api_version(object, "rendererApiVersion", "manifest.rendererApiVersion")?;
    let min_host_version = get_optional_string(
        object,
        "minHostVersion",
        "manifest.minHostVersion",
        100,
        false,
    )?;
    if let Some(version) = &min_host_version {
        parse_semver(version).map_err(|error| format!("manifest.minHostVersion: {error}"))?;
    }

    // 规则 5：backend 可选，必须 boolean
    let backend = match object.get("backend") {
        None => None,
        Some(serde_json::Value::Bool(value)) => Some(*value),
        Some(_) => return Err("manifest.backend: must be a boolean".to_string()),
    };

    // 规则 4 续：v2 一致性
    if manifest_version == Some(2)
        && (renderer_api_version != Some(2)
            || (backend != Some(false) && backend_api_version != Some(2)))
    {
        return Err(
            "manifest: version 2 requires rendererApiVersion 2 and backendApiVersion 2 when backend is enabled"
                .to_string(),
        );
    }

    // 规则 6：displayName 必填 ≤100；description ≤2000 allowEmpty；author ≤200 allowEmpty；icon ≤512 allowEmpty
    let display_name = get_string(object, "displayName", "manifest.displayName")?;
    validate_string(&display_name, "manifest.displayName", 100, false)?;
    let description =
        get_optional_string(object, "description", "manifest.description", 2000, true)?
            .unwrap_or_default();
    let author =
        get_optional_string(object, "author", "manifest.author", 200, true)?.unwrap_or_default();
    let icon = get_optional_string(object, "icon", "manifest.icon", 512, true)?;

    // 规则 7：main/renderer 必填，normalize_plugin_entry
    let main_raw = get_string(object, "main", "manifest.main")?;
    let main = normalize_plugin_entry(&main_raw, "manifest.main")?;
    let renderer_raw = get_string(object, "renderer", "manifest.renderer")?;
    let renderer = normalize_plugin_entry(&renderer_raw, "manifest.renderer")?;

    // 规则 8：permissions 必填数组，长度 ≤ ALL_PERMISSIONS.len()，已知集合内，不允许重复
    let permissions = parse_permissions(object.get("permissions"))?;

    // 规则 9：config 可选，字段数 ≤100，key 匹配且非 FORBIDDEN_KEYS，字段 schema 校验
    let config = parse_config(object.get("config"))?;

    Ok(Manifest {
        manifest_version,
        name,
        version,
        display_name,
        description,
        author,
        icon,
        main,
        renderer,
        backend,
        backend_api_version,
        renderer_api_version,
        min_host_version,
        permissions,
        config,
    })
}

/// 从插件根目录读取 plugin.json（常规文件 + 非 symlink + ≤256KB，对等 readPluginManifest）
pub fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let root_metadata = std::fs::symlink_metadata(root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "manifest: plugin root does not exist".to_string()
        } else {
            format!("manifest: failed to stat plugin root: {error}")
        }
    })?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("manifest: plugin root must be a regular directory".to_string());
    }
    let manifest_path = root.join("plugin.json");
    let metadata = std::fs::symlink_metadata(&manifest_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "manifest: plugin.json does not exist".to_string()
        } else {
            format!("manifest: failed to stat plugin.json: {error}")
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("manifest: plugin.json must be a regular file".to_string());
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "manifest: plugin.json exceeds {MAX_MANIFEST_BYTES} bytes"
        ));
    }
    let bytes = std::fs::read(&manifest_path)
        .map_err(|error| format!("manifest: failed to read plugin.json: {error}"))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(format!(
            "manifest: plugin.json exceeds {MAX_MANIFEST_BYTES} bytes"
        ));
    }
    // 对等 Buffer.toString('utf8')：非法序列替换为 U+FFFD，随后 JSON 解析失败
    let text = String::from_utf8_lossy(&bytes);
    parse_manifest(&text)
}

/// 安装策略：manifestVersion==2 或 allow_legacy_full_trust 通过，否则拒绝 v1
/// （对等 assertPluginManifestInstallable）
pub fn assert_manifest_installable(
    manifest: &Manifest,
    allow_legacy_full_trust: bool,
) -> Result<(), String> {
    if manifest.manifest_version == Some(2) || allow_legacy_full_trust {
        Ok(())
    } else {
        Err(
            "manifest.manifestVersion: legacy v1 packages can no longer be installed; migrate this plugin to Manifest v2"
                .to_string(),
        )
    }
}

/// Reject a plugin whose declared minimum host version is newer than this
/// application.  The comparison uses the same SemVer ordering as upgrade
/// checks, including beta/rc precedence.
pub fn assert_host_version_compatible(
    manifest: &Manifest,
    host_version: &str,
) -> Result<(), String> {
    let Some(minimum) = &manifest.min_host_version else {
        return Ok(());
    };
    if compare_versions(host_version, minimum)? < 0 {
        return Err(format!(
            "{}: requires host >= {}, current host is {}",
            manifest.name, minimum, host_version
        ));
    }
    Ok(())
}

/// 校验 main/renderer 入口：canonicalize 包含性 + 常规文件 + 非 symlink
/// （对等 validatePluginEntrypoints / resolvePluginEntrypoint）
pub fn validate_entrypoints(root: &Path, manifest: &Manifest) -> Result<(), String> {
    let root_metadata = std::fs::symlink_metadata(root)
        .map_err(|error| format!("manifest.entry: failed to stat plugin root: {error}"))?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("manifest.entry: plugin root must be a regular directory".to_string());
    }
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|error| format!("manifest.entry: failed to canonicalize plugin root: {error}"))?;
    resolve_entrypoint(&canonical_root, &manifest.main)?;
    resolve_entrypoint(&canonical_root, &manifest.renderer)?;
    Ok(())
}

/// 手写 semver 比较（对等 semver.ts compareVersions）；返回 -1/0/1
pub fn compare_versions(left: &str, right: &str) -> Result<i32, String> {
    let a = parse_semver(left)?;
    let b = parse_semver(right)?;
    for (a_part, b_part) in [(a.0, b.0), (a.1, b.1), (a.2, b.2)] {
        if a_part != b_part {
            return Ok(if a_part < b_part { -1 } else { 1 });
        }
    }
    Ok(compare_prerelease(&a.3, &b.3))
}

/// 升级策略：incoming < existing → 拒绝降级；== → 拒绝同版本覆盖
pub fn assert_upgrade_allowed(name: &str, incoming: &str, existing: &str) -> Result<(), String> {
    let comparison = compare_versions(incoming, existing)?;
    if comparison < 0 {
        return Err(format!(
            "{name}: cannot downgrade from {existing} to {incoming}"
        ));
    }
    if comparison == 0 {
        return Err(format!(
            "{name}: cannot overwrite existing version {existing}"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

/// 字符串通用规则：长度 1..max（allowEmpty 时 0..max）、不含控制字符、无首尾空白
/// （对等 readString；长度按 UTF-16 code unit 计数，与 JS .length 一致）
fn validate_string(value: &str, path: &str, max: usize, allow_empty: bool) -> Result<(), String> {
    let min = if allow_empty { 0 } else { 1 };
    let length = utf16_len(value);
    if length < min || length > max {
        return Err(format!("{path}: must contain {min}-{max} characters"));
    }
    if contains_control(value) || value != value.trim() {
        return Err(format!(
            "{path}: contains control characters or surrounding whitespace"
        ));
    }
    Ok(())
}

/// 控制字符：code ≤0x1f 或 0x7f（对等 containsControlCharacter）
fn contains_control(value: &str) -> bool {
    value
        .chars()
        .any(|c| (c as u32) <= 0x1f || (c as u32) == 0x7f)
}

/// UTF-16 code unit 计数（对等 JS String.prototype.length）
fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// 插件名模式 ^[a-z0-9][a-z0-9_-]{0,63}$
fn is_valid_plugin_name(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// config key 模式 ^[a-zA-Z][a-zA-Z0-9_.-]{0,63}$
fn is_valid_config_key(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

/// win32 绝对路径：`C:\...` / `C:/...` / `\\server\share`（对等 win32.isAbsolute）
fn is_win32_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
    {
        return true;
    }
    if bytes.len() >= 2 && bytes[0] == b'\\' && bytes[1] == b'\\' {
        return true;
    }
    false
}

/// posix.normalize 的等价实现（对等 node:path posix.normalize）
fn posix_normalize(value: &str) -> String {
    if value.is_empty() {
        return ".".to_string();
    }
    let mut out: Vec<&str> = Vec::new();
    for segment in value.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

/// 入口路径规范化（对等 normalizePluginEntry）
fn normalize_plugin_entry(value: &str, path: &str) -> Result<String, String> {
    validate_string(value, path, 240, false)?;
    let portable = value.replace('\\', "/");
    if is_win32_absolute(value) || portable.starts_with('/') {
        return Err(format!("{path}: must be a normalized relative path"));
    }
    if portable
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(format!("{path}: must be a normalized relative path"));
    }
    let normalized = posix_normalize(&portable);
    if normalized != portable || !normalized.ends_with(".js") || normalized.contains(':') {
        return Err(format!(
            "{path}: must be a normalized relative JavaScript file"
        ));
    }
    Ok(normalized)
}

/// 必填字符串字段
fn get_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &str,
) -> Result<String, String> {
    match object.get(key) {
        Some(serde_json::Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(format!("{path}: must be a string")),
        None => Err(format!("{path}: is required")),
    }
}

/// 可选字符串字段（带长度/控制字符校验）
fn get_optional_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &str,
    max: usize,
    allow_empty: bool,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => {
            validate_string(value, path, max, allow_empty)?;
            Ok(Some(value.clone()))
        }
        Some(_) => Err(format!("{path}: must be a string")),
    }
}

/// 可选 API 版本字段：必须为 1 或 2（对等 manifestVersion 等检查）
fn get_api_version(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &str,
) -> Result<Option<u8>, String> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => {
            let number = value
                .as_f64()
                .ok_or_else(|| format!("{path}: must be 1 or 2"))?;
            if number == 1.0 {
                Ok(Some(1))
            } else if number == 2.0 {
                Ok(Some(2))
            } else {
                Err(format!("{path}: must be 1 or 2"))
            }
        }
    }
}

/// permissions 解析（对等 parsePermissions）
fn parse_permissions(value: Option<&serde_json::Value>) -> Result<Vec<String>, String> {
    let array = value
        .and_then(|v| v.as_array())
        .ok_or_else(|| "manifest.permissions: must be a bounded array".to_string())?;
    if array.len() > ALL_PERMISSIONS.len() {
        return Err("manifest.permissions: must be a bounded array".to_string());
    }
    let mut seen: HashSet<&str> = HashSet::new();
    let mut permissions = Vec::new();
    for (index, item) in array.iter().enumerate() {
        let permission = item
            .as_str()
            .ok_or_else(|| format!("manifest.permissions[{index}]: must be a string"))?;
        validate_string(
            permission,
            &format!("manifest.permissions[{index}]"),
            64,
            false,
        )?;
        if !ALL_PERMISSIONS.contains(&permission) {
            return Err(format!("manifest.permissions[{index}]: is unknown"));
        }
        if !seen.insert(permission) {
            return Err(format!("manifest.permissions[{index}]: is duplicated"));
        }
        permissions.push(permission.to_string());
    }
    Ok(permissions)
}

/// config 解析（对等 parseConfig）
fn parse_config(
    value: Option<&serde_json::Value>,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let object = match value {
        None => return Ok(serde_json::Map::new()),
        Some(v) => v
            .as_object()
            .ok_or_else(|| "manifest.config: must be a plain object".to_string())?,
    };
    if object.len() > 100 {
        return Err("manifest.config: contains too many fields".to_string());
    }
    for key in object.keys() {
        if FORBIDDEN_KEYS.contains(&key.as_str()) || !is_valid_config_key(key) {
            return Err(format!("manifest.config.{key}: has an invalid key"));
        }
    }
    for (key, field) in object {
        parse_config_field(field, &format!("manifest.config.{key}"))?;
    }
    Ok(object.clone())
}

/// config 单字段校验（对等 parseConfigField）
fn parse_config_field(value: &serde_json::Value, path: &str) -> Result<(), String> {
    let field = value
        .as_object()
        .ok_or_else(|| format!("{path}: must be a plain object"))?;
    for key in field.keys() {
        if !CONFIG_FIELD_KEYS.contains(&key.as_str()) {
            return Err(format!("{path}.{key}: is not a supported field"));
        }
    }
    let field_type = field
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{path}.type: must be a string"))?;
    validate_string(field_type, &format!("{path}.type"), 32, false)?;
    if !CONFIG_FIELD_TYPES.contains(&field_type) {
        return Err(format!("{path}.type: is unsupported"));
    }
    let label = field
        .get("label")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{path}.label: must be a string"))?;
    validate_string(label, &format!("{path}.label"), 100, false)?;
    if let Some(description) = field.get("description") {
        let description = description
            .as_str()
            .ok_or_else(|| format!("{path}.description: must be a string"))?;
        validate_string(description, &format!("{path}.description"), 500, true)?;
    }
    if let Some(required) = field.get("required") {
        if !required.is_boolean() {
            return Err(format!("{path}.required: must be a boolean"));
        }
    }
    if let Some(options) = field.get("options") {
        if (field_type != "select" && field_type != "multiselect") || !options.is_array() {
            return Err(format!(
                "{path}.options: must contain 1-100 options for a select field"
            ));
        }
        let options = options.as_array().ok_or_else(|| {
            format!("{path}.options: must contain 1-100 options for a select field")
        })?;
        if options.is_empty() || options.len() > 100 {
            return Err(format!(
                "{path}.options: must contain 1-100 options for a select field"
            ));
        }
        let mut values: HashSet<&str> = HashSet::new();
        for (index, option) in options.iter().enumerate() {
            let option_path = format!("{path}.options[{index}]");
            let option = option
                .as_object()
                .ok_or_else(|| format!("{option_path}: must be a plain object"))?;
            for key in option.keys() {
                if key != "label" && key != "value" {
                    return Err(format!("{option_path}.{key}: is not a supported field"));
                }
            }
            let option_label = option
                .get("label")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("{option_path}.label: must be a string"))?;
            validate_string(option_label, &format!("{option_path}.label"), 100, false)?;
            let option_value = option
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("{option_path}.value: must be a string"))?;
            validate_string(option_value, &format!("{option_path}.value"), 200, false)?;
            if !values.insert(option_value) {
                return Err(format!("{path}.options: contains duplicate values"));
            }
        }
    }
    if let Some(default) = field.get("default") {
        assert_default_value(field_type, default, &format!("{path}.default"))?;
    }
    Ok(())
}

/// default 必须匹配字段类型（对等 assertDefaultValue）
fn assert_default_value(
    field_type: &str,
    value: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    let valid = match field_type {
        "number" => value.as_f64().is_some_and(|n| n.is_finite()),
        "boolean" => value.is_boolean(),
        "string" | "select" => value.is_string(),
        "multiselect" => value
            .as_array()
            .is_some_and(|array| array.len() <= 100 && array.iter().all(|entry| entry.is_string())),
        _ => false,
    };
    if !valid {
        return Err(format!(
            "{path}: does not match config field type {field_type}"
        ));
    }
    Ok(())
}

/// 解析 semver 为 (major, minor, patch, prerelease)；严格校验（对等 parseSemVer）
fn parse_semver(value: &str) -> Result<(u64, u64, u64, Vec<String>), String> {
    let (core, build, has_plus) = match value.split_once('+') {
        Some((c, b)) => (c, b, true),
        None => (value, "", false),
    };
    if has_plus && build.is_empty() {
        return Err(format!("invalid semantic version: {value}"));
    }
    if !build.is_empty() {
        validate_dot_identifiers(build, "build")?;
    }
    let (core, prerelease, has_dash) = match core.split_once('-') {
        Some((c, p)) => (c, p, true),
        None => (core, "", false),
    };
    if has_dash && prerelease.is_empty() {
        return Err(format!("invalid semantic version: {value}"));
    }
    if !prerelease.is_empty() {
        validate_prerelease(prerelease)?;
    }
    let mut parts = core.split('.');
    let major = parse_version_number(
        parts
            .next()
            .ok_or_else(|| format!("invalid semantic version: {value}"))?,
        value,
    )?;
    let minor = parse_version_number(
        parts
            .next()
            .ok_or_else(|| format!("invalid semantic version: {value}"))?,
        value,
    )?;
    let patch = parse_version_number(
        parts
            .next()
            .ok_or_else(|| format!("invalid semantic version: {value}"))?,
        value,
    )?;
    if parts.next().is_some() {
        return Err(format!("invalid semantic version: {value}"));
    }
    let prerelease_parts = if prerelease.is_empty() {
        Vec::new()
    } else {
        prerelease.split('.').map(|s| s.to_string()).collect()
    };
    Ok((major, minor, patch, prerelease_parts))
}

/// 点分隔标识符校验：每段非空且匹配 [0-9A-Za-z-]+
fn validate_dot_identifiers(value: &str, kind: &str) -> Result<(), String> {
    for identifier in value.split('.') {
        if identifier.is_empty() || !identifier.chars().all(is_identifier_char) {
            return Err(format!("invalid semantic version {kind} metadata: {value}"));
        }
    }
    Ok(())
}

/// prerelease 校验：标识符合法 + 数字段无前导零
fn validate_prerelease(value: &str) -> Result<(), String> {
    for identifier in value.split('.') {
        if identifier.is_empty() || !identifier.chars().all(is_identifier_char) {
            return Err(format!("invalid semantic version prerelease: {value}"));
        }
        if identifier.bytes().all(|b| b.is_ascii_digit())
            && identifier.len() > 1
            && identifier.starts_with('0')
        {
            return Err(format!(
                "numeric prerelease identifiers cannot contain leading zeroes: {value}"
            ));
        }
    }
    Ok(())
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

/// 数字段解析：无前导零 + safe integer（≤ MAX_SAFE_INTEGER）
fn parse_version_number(segment: &str, value: &str) -> Result<u64, String> {
    if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid semantic version: {value}"));
    }
    if segment.len() > 1 && segment.starts_with('0') {
        return Err(format!("invalid semantic version: {value}"));
    }
    let number: u64 = segment
        .parse()
        .map_err(|_| format!("semantic version exceeds safe integer range: {value}"))?;
    if number > MAX_SAFE_INTEGER {
        return Err(format!(
            "semantic version exceeds safe integer range: {value}"
        ));
    }
    Ok(number)
}

/// prerelease 比较（对等 comparePrerelease）：无 prerelease > 有 prerelease；
/// 数字段 < 字母段；数字段按长度/字典序（无前导零保证等价于数值比较）
fn compare_prerelease(left: &[String], right: &[String]) -> i32 {
    if left.is_empty() && right.is_empty() {
        return 0;
    }
    if left.is_empty() {
        return 1;
    }
    if right.is_empty() {
        return -1;
    }
    let length = left.len().max(right.len());
    for index in 0..length {
        let a = left.get(index);
        let b = right.get(index);
        match (a, b) {
            (None, _) => return -1,
            (_, None) => return 1,
            (Some(a), Some(b)) => {
                if a == b {
                    continue;
                }
                let a_numeric = is_numeric_identifier(a);
                let b_numeric = is_numeric_identifier(b);
                if a_numeric && b_numeric {
                    if a.len() != b.len() {
                        return if a.len() < b.len() { -1 } else { 1 };
                    }
                    return if a < b { -1 } else { 1 };
                }
                if a_numeric != b_numeric {
                    return if a_numeric { -1 } else { 1 };
                }
                return if a < b { -1 } else { 1 };
            }
        }
    }
    0
}

fn is_numeric_identifier(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

/// 入口解析：resolve 后必须落在 canonicalRoot 内、存在、常规文件、非 symlink；
/// 二次 canonicalize 后仍须在 root 内（防符号链接逃逸，对等 resolvePluginEntrypoint）
fn resolve_entrypoint(canonical_root: &Path, entry: &str) -> Result<(), String> {
    let candidate = canonical_root.join(entry);
    if !is_inside(canonical_root, &candidate) {
        return Err(format!(
            "manifest.entry: resolved outside the plugin root: {entry}"
        ));
    }
    let metadata = std::fs::symlink_metadata(&candidate).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("manifest.entry: does not exist: {entry}")
        } else {
            format!("manifest.entry: failed to stat {entry}: {error}")
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!("manifest.entry: is not a regular file: {entry}"));
    }
    let canonical_candidate = std::fs::canonicalize(&candidate)
        .map_err(|error| format!("manifest.entry: failed to canonicalize {entry}: {error}"))?;
    if !is_inside(canonical_root, &canonical_candidate) {
        return Err(format!(
            "manifest.entry: resolved through a link outside the plugin root: {entry}"
        ));
    }
    Ok(())
}

/// candidate 是否严格位于 root 内（首段为 Normal 组件）
fn is_inside(root: &Path, candidate: &Path) -> bool {
    match candidate.strip_prefix(root) {
        Ok(relative) => matches!(
            relative.components().next(),
            Some(std::path::Component::Normal(_))
        ),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_manifest() -> serde_json::Value {
        json!({
            "name": "demo",
            "version": "1.2.3",
            "displayName": "Demo Plugin",
            "description": "",
            "author": "",
            "main": "main.js",
            "renderer": "renderer.js",
            "manifestVersion": 2,
            "rendererApiVersion": 2,
            "backend": false,
            "permissions": ["storage:read"],
            "config": {}
        })
    }

    fn parse_with(mutate: impl FnOnce(&mut serde_json::Value)) -> Result<Manifest, String> {
        let mut value = base_manifest();
        mutate(&mut value);
        parse_manifest(&value.to_string())
    }

    fn expect_ok<T>(result: Result<T, String>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok, got Err: {error}"),
        }
    }

    fn expect_err<T>(result: Result<T, String>) -> String {
        match result {
            Err(error) => error,
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    fn io_ok<T>(result: std::io::Result<T>) -> Result<T, String> {
        result.map_err(|error| error.to_string())
    }

    fn temp_dir(tag: &str) -> Result<std::path::PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "cruciblebox-manifest-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }

    #[test]
    fn valid_manifest_parses() {
        let manifest = expect_ok(parse_with(|_| {}));
        assert_eq!(manifest.name, "demo");
        assert_eq!(manifest.version, "1.2.3");
        assert_eq!(manifest.display_name, "Demo Plugin");
        assert_eq!(manifest.main, "main.js");
        assert_eq!(manifest.renderer, "renderer.js");
        assert_eq!(manifest.manifest_version, Some(2));
        assert_eq!(manifest.renderer_api_version, Some(2));
        assert_eq!(manifest.backend, Some(false));
        assert_eq!(manifest.permissions, vec!["storage:read".to_string()]);
        assert!(manifest.config.is_empty());
    }

    #[test]
    fn host_version_constraint_parses_and_validates() {
        let manifest = expect_ok(parse_with(|value| {
            value["minHostVersion"] = json!("2.0.0-beta.3");
        }));
        assert_eq!(manifest.min_host_version.as_deref(), Some("2.0.0-beta.3"));

        let error = expect_err(parse_with(|value| {
            value["minHostVersion"] = json!("2.0");
        }));
        assert!(error.contains("manifest.minHostVersion"), "error: {error}");
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let error = expect_err(parse_with(|value| {
            value["evil"] = json!(1);
        }));
        assert!(error.contains("manifest.evil"), "error: {error}");
    }

    #[test]
    fn rejects_forbidden_keys() {
        for key in ["__proto__", "prototype", "constructor"] {
            let error = expect_err(parse_with(|value| {
                value[key] = json!(1);
            }));
            assert!(error.contains(&format!("manifest.{key}")), "error: {error}");
        }
    }

    #[test]
    fn name_validation() {
        let bad_names: Vec<String> = ["Bad", "has space", "UPPER", "-lead"]
            .iter()
            .map(|s| s.to_string())
            .chain(std::iter::once("a".repeat(65)))
            .collect();
        for bad in bad_names {
            let error = expect_err(parse_with(|value| {
                value["name"] = json!(bad);
            }));
            assert!(error.contains("manifest.name"), "name {bad:?} -> {error}");
        }
        let good_names: Vec<String> = ["a", "a-b_c", "0abc"]
            .iter()
            .map(|s| s.to_string())
            .chain(std::iter::once("a".repeat(64)))
            .collect();
        for good in good_names {
            assert!(
                parse_with(|value| {
                    value["name"] = json!(good);
                })
                .is_ok(),
                "name {good:?} should be valid"
            );
        }
    }

    #[test]
    fn version_boundaries() {
        for good in [
            "0.0.0",
            "1.2.3",
            "1.2.3-alpha",
            "1.2.3-alpha.1",
            "1.2.3+build",
            "1.2.3-rc.1+build.5",
            "10.20.30",
            "9007199254740991.0.0",
        ] {
            assert!(
                parse_with(|value| {
                    value["version"] = json!(good);
                })
                .is_ok(),
                "version {good:?} should be valid"
            );
        }
        for bad in [
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2.3-",
            "1.2.3+",
            "1.2.3-01",
            "1.2.3-alpha..beta",
            "1.2.3-alpha beta",
            "v1.2.3",
            "1.2.3-alpha_1",
            "9007199254740992.0.0",
        ] {
            assert!(
                parse_with(|value| {
                    value["version"] = json!(bad);
                })
                .is_err(),
                "version {bad:?} should be invalid"
            );
        }
    }

    #[test]
    fn v2_consistency() {
        // manifestVersion 2 要求 rendererApiVersion 2
        let error = expect_err(parse_with(|value| {
            value["manifestVersion"] = json!(2);
            value["rendererApiVersion"] = json!(1);
        }));
        assert!(error.contains("version 2 requires"), "error: {error}");

        // backend 启用时要求 backendApiVersion 2
        let error = expect_err(parse_with(|value| {
            value["manifestVersion"] = json!(2);
            value["rendererApiVersion"] = json!(2);
            value["backend"] = json!(true);
            value["backendApiVersion"] = json!(1);
        }));
        assert!(error.contains("version 2 requires"), "error: {error}");

        // backend 禁用时 backendApiVersion 可缺省
        assert!(parse_with(|value| {
            value["manifestVersion"] = json!(2);
            value["rendererApiVersion"] = json!(2);
            value["backend"] = json!(false);
        })
        .is_ok());

        // manifestVersion 必须为 1 或 2
        let error = expect_err(parse_with(|value| {
            value["manifestVersion"] = json!(3);
        }));
        assert!(error.contains("manifest.manifestVersion"), "error: {error}");

        // backend 必须为 boolean
        let error = expect_err(parse_with(|value| {
            value["backend"] = json!("yes");
        }));
        assert!(error.contains("manifest.backend"), "error: {error}");
    }

    #[test]
    fn entry_path_traversal_rejected() {
        for bad in [
            "../evil.js",
            "a/../../evil.js",
            "/abs.js",
            "C:\\evil.js",
            "a//b.js",
            "a/./b.js",
            "a/b/",
            "main.txt",
            "a:b.js",
        ] {
            let error = expect_err(parse_with(|value| {
                value["main"] = json!(bad);
            }));
            assert!(error.contains("manifest.main"), "main {bad:?} -> {error}");
        }
    }

    #[test]
    fn entry_backslash_normalized() {
        let manifest = expect_ok(parse_with(|value| {
            value["main"] = json!("lib\\main.js");
        }));
        assert_eq!(manifest.main, "lib/main.js");
    }

    #[test]
    fn permissions_duplicate_and_unknown_rejected() {
        let error = expect_err(parse_with(|value| {
            value["permissions"] = json!(["storage:read", "storage:read"]);
        }));
        assert!(error.contains("duplicated"), "error: {error}");

        let error = expect_err(parse_with(|value| {
            value["permissions"] = json!(["storage:read", "unknown:perm"]);
        }));
        assert!(error.contains("unknown"), "error: {error}");
    }

    #[test]
    fn config_schema_validation() {
        // 合法 config
        assert!(parse_with(|value| {
            value["config"] = json!({
                "theme": {
                    "type": "select",
                    "label": "Theme",
                    "options": [
                        { "label": "Dark", "value": "dark" },
                        { "label": "Light", "value": "light" }
                    ],
                    "default": "dark"
                },
                "count": { "type": "number", "label": "Count", "default": 3 },
                "flag": { "type": "boolean", "label": "Flag", "required": true },
                "tags": { "type": "multiselect", "label": "Tags", "default": ["a", "b"] }
            });
        })
        .is_ok());

        // 非法 type
        let error = expect_err(parse_with(|value| {
            value["config"] = json!({ "f": { "type": "color", "label": "F" } });
        }));
        assert!(error.contains("config.f.type"), "error: {error}");

        // 非 select 字段带 options
        let error = expect_err(parse_with(|value| {
            value["config"] = json!({
                "f": { "type": "string", "label": "F", "options": [{ "label": "A", "value": "a" }] }
            });
        }));
        assert!(error.contains("config.f.options"), "error: {error}");

        // options value 重复
        let error = expect_err(parse_with(|value| {
            value["config"] = json!({
                "f": {
                    "type": "select",
                    "label": "F",
                    "options": [
                        { "label": "A", "value": "x" },
                        { "label": "B", "value": "x" }
                    ]
                }
            });
        }));
        assert!(error.contains("duplicate values"), "error: {error}");

        // default 类型不匹配
        let error = expect_err(parse_with(|value| {
            value["config"] = json!({ "f": { "type": "number", "label": "F", "default": "3" } });
        }));
        assert!(error.contains("config.f.default"), "error: {error}");

        // 未知字段键
        let error = expect_err(parse_with(|value| {
            value["config"] = json!({ "f": { "type": "string", "label": "F", "extra": 1 } });
        }));
        assert!(error.contains("config.f.extra"), "error: {error}");

        // 非法 config key
        let error = expect_err(parse_with(|value| {
            value["config"] = json!({ "1bad": { "type": "string", "label": "F" } });
        }));
        assert!(error.contains("config.1bad"), "error: {error}");
    }

    #[test]
    fn compare_versions_ordering() {
        assert_eq!(expect_ok(compare_versions("1.0.0", "1.0.0")), 0);
        assert_eq!(expect_ok(compare_versions("1.0.1", "1.0.0")), 1);
        assert_eq!(expect_ok(compare_versions("1.1.0", "1.0.9")), 1);
        assert_eq!(expect_ok(compare_versions("2.0.0", "1.9.9")), 1);
        assert_eq!(expect_ok(compare_versions("1.0.0-alpha", "1.0.0")), -1);
        assert_eq!(expect_ok(compare_versions("1.0.0", "1.0.0-alpha")), 1);
        assert_eq!(expect_ok(compare_versions("1.0.0-alpha", "1.0.0-beta")), -1);
        assert_eq!(
            expect_ok(compare_versions("1.0.0-alpha.1", "1.0.0-alpha.2")),
            -1
        );
        assert_eq!(
            expect_ok(compare_versions("1.0.0-alpha.2", "1.0.0-alpha.10")),
            -1
        );
        assert_eq!(expect_ok(compare_versions("1.0.0-alpha", "1.0.0-1")), 1);
        assert_eq!(expect_ok(compare_versions("1.0.0-1", "1.0.0-alpha")), -1);
        assert_eq!(expect_ok(compare_versions("1.0.0-rc.1", "1.0.0-rc.1")), 0);
        assert!(compare_versions("not-a-version", "1.0.0").is_err());
    }

    #[test]
    fn upgrade_allowed() {
        assert!(assert_upgrade_allowed("demo", "1.1.0", "1.0.0").is_ok());
        let error = expect_err(assert_upgrade_allowed("demo", "1.0.0", "1.1.0"));
        assert!(error.contains("downgrade"), "error: {error}");
        let error = expect_err(assert_upgrade_allowed("demo", "1.0.0", "1.0.0"));
        assert!(error.contains("overwrite"), "error: {error}");
    }

    #[test]
    fn installable_policy() {
        let v2 = expect_ok(parse_with(|_| {}));
        assert!(assert_manifest_installable(&v2, false).is_ok());

        let v1 = expect_ok(parse_with(|value| {
            value["manifestVersion"] = json!(1);
            value["rendererApiVersion"] = json!(1);
            value["backend"] = json!(false);
        }));
        assert!(assert_manifest_installable(&v1, false).is_err());
        assert!(assert_manifest_installable(&v1, true).is_ok());
    }

    #[test]
    fn host_version_compatibility() {
        let manifest = expect_ok(parse_with(|value| {
            value["minHostVersion"] = json!("2.0.0-beta.3");
        }));
        assert!(assert_host_version_compatible(&manifest, "2.0.0-beta.4").is_ok());
        let error = expect_err(assert_host_version_compatible(&manifest, "2.0.0-beta.2"));
        assert!(error.contains("requires host"), "error: {error}");
    }

    #[test]
    fn read_manifest_checks_regular_file() {
        let dir = expect_ok(temp_dir("read_manifest"));
        // 缺失 plugin.json
        let error = expect_err(read_manifest(&dir));
        assert!(
            error.contains("plugin.json does not exist"),
            "error: {error}"
        );

        // plugin.json 是目录
        expect_ok(io_ok(std::fs::create_dir_all(dir.join("plugin.json"))));
        let error = expect_err(read_manifest(&dir));
        assert!(error.contains("regular file"), "error: {error}");
        expect_ok(io_ok(std::fs::remove_dir_all(dir.join("plugin.json"))));

        // 超过 256KB
        let oversized = "x".repeat(300 * 1024);
        expect_ok(io_ok(std::fs::write(dir.join("plugin.json"), oversized)));
        let error = expect_err(read_manifest(&dir));
        assert!(error.contains("exceeds"), "error: {error}");
        expect_ok(io_ok(std::fs::remove_file(dir.join("plugin.json"))));

        // 合法
        expect_ok(io_ok(std::fs::write(
            dir.join("plugin.json"),
            base_manifest().to_string(),
        )));
        let manifest = expect_ok(read_manifest(&dir));
        assert_eq!(manifest.name, "demo");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_entrypoints_requires_existing_files() {
        let dir = expect_ok(temp_dir("entrypoints"));
        let root = dir.join("plugin");
        expect_ok(io_ok(std::fs::create_dir_all(&root)));
        expect_ok(io_ok(std::fs::write(
            root.join("plugin.json"),
            base_manifest().to_string(),
        )));
        let manifest = expect_ok(read_manifest(&root));

        // main.js 不存在
        let error = expect_err(validate_entrypoints(&root, &manifest));
        assert!(error.contains("does not exist"), "error: {error}");

        // 创建 main.js，renderer.js 仍缺失
        expect_ok(io_ok(std::fs::write(root.join("main.js"), "// main")));
        let error = expect_err(validate_entrypoints(&root, &manifest));
        assert!(error.contains("does not exist"), "error: {error}");

        // renderer.js 是目录 → 非常规文件
        expect_ok(io_ok(std::fs::create_dir_all(root.join("renderer.js"))));
        let error = expect_err(validate_entrypoints(&root, &manifest));
        assert!(error.contains("regular file"), "error: {error}");

        // 修复后通过
        expect_ok(io_ok(std::fs::remove_dir_all(root.join("renderer.js"))));
        expect_ok(io_ok(std::fs::write(
            root.join("renderer.js"),
            "// renderer",
        )));
        expect_ok(validate_entrypoints(&root, &manifest));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn validate_entrypoints_rejects_symlink_file_escape() {
        let dir = expect_ok(temp_dir("entry_symlink_file"));
        let root = dir.join("plugin");
        expect_ok(io_ok(std::fs::create_dir_all(&root)));
        expect_ok(io_ok(std::fs::write(
            root.join("plugin.json"),
            base_manifest().to_string(),
        )));
        let outside = dir.join("outside.js");
        expect_ok(io_ok(std::fs::write(&outside, "// outside")));
        let link = root.join("main.js");
        let created = std::os::windows::fs::symlink_file(&outside, &link);
        if created.is_ok() {
            expect_ok(io_ok(std::fs::write(
                root.join("renderer.js"),
                "// renderer",
            )));
            let manifest = expect_ok(read_manifest(&root));
            let error = expect_err(validate_entrypoints(&root, &manifest));
            assert!(error.contains("regular file"), "error: {error}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn validate_entrypoints_rejects_symlink_dir_escape() {
        let dir = expect_ok(temp_dir("entry_symlink_dir"));
        let root = dir.join("plugin");
        expect_ok(io_ok(std::fs::create_dir_all(&root)));
        let outside_dir = dir.join("outside");
        expect_ok(io_ok(std::fs::create_dir_all(&outside_dir)));
        expect_ok(io_ok(std::fs::write(
            outside_dir.join("main.js"),
            "// main",
        )));
        expect_ok(io_ok(std::fs::write(
            root.join("renderer.js"),
            "// renderer",
        )));
        let sub = root.join("sub");
        let created = std::os::windows::fs::symlink_dir(&outside_dir, &sub);
        if created.is_ok() {
            let mut value = base_manifest();
            value["main"] = json!("sub/main.js");
            expect_ok(io_ok(std::fs::write(
                root.join("plugin.json"),
                value.to_string(),
            )));
            let manifest = expect_ok(read_manifest(&root));
            let error = expect_err(validate_entrypoints(&root, &manifest));
            assert!(error.contains("outside"), "error: {error}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

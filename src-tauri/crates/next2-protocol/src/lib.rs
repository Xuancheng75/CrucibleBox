//! Manifest/API 6 and generic wire 4. Frozen Manifest/API 5 remains a separate crate.
pub mod gateway;
mod generated;
mod schema;
pub use generated::*;
use serde_json::Value;
use std::{collections::HashSet, sync::OnceLock};

pub const MAX_FRAME_BYTES: usize = BUDGET_BYTES;
/// Validate namespace data with the same JSON limits as RPC payloads, independent of envelope shape.
pub fn validate_json_value(raw: &str, bytes: usize) -> Result<Value, String> {
    if raw.len() > bytes {
        return Err("BUDGET_EXCEEDED".into());
    }
    let value: Value = serde_json::from_str(raw).map_err(|_| "INVALID_REQUEST")?;
    schema::budget(&value, 1, &mut 0, BUDGET_NODES)?;
    Ok(value)
}
pub fn validate_storage_value(raw: &str) -> Result<Value, String> {
    validate_json_value(raw, BUDGET_STORAGE_VALUE_BYTES)
}
pub fn validate_payload(raw: &str) -> Result<Value, String> {
    validate_json_value(raw, MAX_FRAME_BYTES)
}
pub fn contract() -> &'static Value {
    static CONTRACT: OnceLock<Value> = OnceLock::new();
    CONTRACT
        .get_or_init(|| serde_json::from_str(CONTRACT_JSON).expect("generated Next 2.0 contract"))
}
pub fn compatible(version: &str, range: &VersionRange) -> Result<bool, String> {
    let version = semver::Version::parse(version).map_err(|e| e.to_string())?;
    let min = semver::Version::parse(&range.min).map_err(|e| e.to_string())?;
    let max = semver::Version::parse(&range.max_exclusive).map_err(|e| e.to_string())?;
    if min >= max {
        return Err("INVALID_VERSION_RANGE".into());
    }
    Ok(version >= min && version < max)
}
fn unique<'a>(items: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
    let mut names = HashSet::new();
    for name in items {
        if !names.insert(name) {
            return Err("DUPLICATE_DECLARATION".into());
        }
    }
    Ok(())
}
fn inventory(files: &[FileEntry]) -> Result<(), String> {
    let mut names = HashSet::new();
    for file in files {
        let name = file.path.to_ascii_lowercase();
        if !names.insert(name.clone()) {
            return Err("DUPLICATE_DECLARATION".into());
        }
        if contract()["install"]["reservedPaths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reserved| {
                let reserved = reserved.as_str().unwrap();
                name == reserved || name.starts_with(&format!("{reserved}/"))
            })
        {
            return Err("RESERVED_PACKAGE_FILE".into());
        }
    }
    for name in &names {
        let mut parent = name.as_str();
        while let Some((ancestor, _)) = parent.rsplit_once('/') {
            if names.contains(ancestor) {
                return Err("PACKAGE_FILE_DIRECTORY_COLLISION".into());
            }
            parent = ancestor;
        }
    }
    Ok(())
}
pub fn validate_manifest(raw: &str) -> Result<Manifest, String> {
    let manifest: Manifest = schema::parse(raw, "Manifest", BUDGET_MANIFEST_BYTES)?;
    inventory(&manifest.files)?;
    if manifest.files.iter().map(|v| v.bytes).sum::<u64>() > BUDGET_PACKAGE_BYTES as u64 {
        return Err("BUDGET_EXCEEDED".into());
    }
    unique(manifest.exports.iter().map(|v| v.id.as_str()))?;
    unique(manifest.dependencies.iter().map(|v| v.id.as_str()))?;
    unique(manifest.resources.iter().map(|v| v.id.as_str()))?;
    unique(manifest.contributes.commands.iter().map(|v| v.id.as_str()))?;
    for export in &manifest.exports {
        unique(export.methods.iter().map(String::as_str))?;
    }
    for dependency in &manifest.dependencies {
        compatible(&dependency.version.min, &dependency.version)?;
        if dependency.provider_id == manifest.id {
            return Err("SELF_DEPENDENCY".into());
        }
    }
    for resource in &manifest.resources {
        compatible(&resource.version.min, &resource.version)?;
    }
    let mut entries = vec![manifest.renderer.as_str()];
    entries.extend(
        manifest
            .resources
            .iter()
            .filter_map(|resource| resource.catalog.as_deref()),
    );
    match &manifest.backend {
        Some(Backend::NativeBackend(backend)) => {
            if !backend.entry.ends_with(".exe") {
                return Err("INVALID_NATIVE_ENTRY".into());
            }
            entries.push(&backend.entry);
        }
        Some(Backend::JsBackend(backend)) => {
            if !backend.entry.ends_with(".js") {
                return Err("INVALID_JS_ENTRY".into());
            }
            entries.push(&backend.entry);
        }
        None if manifest.background || !manifest.exports.is_empty() => {
            return Err("BACKEND_REQUIRED".into())
        }
        None => {}
    }
    if !manifest.renderer.ends_with(".js") {
        return Err("INVALID_RENDERER_ENTRY".into());
    }
    if let Some(icon) = manifest
        .contributes
        .navigation
        .as_ref()
        .and_then(|v| v.icon.as_ref())
    {
        entries.push(icon);
    }
    if entries
        .iter()
        .any(|entry| !manifest.files.iter().any(|v| v.path == *entry))
    {
        return Err("UNDECLARED_ENTRY".into());
    }
    Ok(manifest)
}
pub fn validate_resource_manifest(raw: &str) -> Result<ResourceManifest, String> {
    let manifest: ResourceManifest =
        schema::parse(raw, "ResourceManifest", BUDGET_RESOURCE_MANIFEST_BYTES)?;
    inventory(&manifest.files)?;
    compatible(&manifest.plugin_version.min, &manifest.plugin_version)?;
    if manifest.files.iter().map(|v| v.bytes).sum::<u64>() > BUDGET_RESOURCE_BYTES as u64 {
        return Err("BUDGET_EXCEEDED".into());
    }
    Ok(manifest)
}
pub fn validate_resource_catalog(raw: &str) -> Result<ResourceCatalog, String> {
    let catalog: ResourceCatalog =
        schema::parse(raw, "ResourceCatalog", BUDGET_RESOURCE_MANIFEST_BYTES)?;
    unique(catalog.releases.iter().map(|release| release.id.as_str()))?;
    for release in &catalog.releases {
        validate_resource_manifest(
            &serde_json::to_string(&release.manifest).map_err(|e| e.to_string())?,
        )?;
        unique(release.downloads.iter().map(|file| file.path.as_str()))?;
        let files: HashSet<_> = release
            .manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect();
        let downloads: HashSet<_> = release
            .downloads
            .iter()
            .map(|file| file.path.as_str())
            .collect();
        if files != downloads {
            return Err("RESOURCE_DOWNLOAD_INVENTORY_MISMATCH".into());
        }
    }
    Ok(catalog)
}
pub fn validate_request(raw: &str) -> Result<Request, String> {
    let request: Request = schema::parse(raw, "Request", MAX_FRAME_BYTES)?;
    if matches!(request.target, Target::PlatformTarget(_)) {
        let method = contract()["methods"]
            .get(&request.method)
            .ok_or("INVALID_PLATFORM_CALL")?;
        if !schema::matches(&method["params"], &request.params) {
            return Err("INVALID_PLATFORM_CALL".into());
        }
        if request.method == "tasks.start" && request.params["target"]["kind"] == "platform" {
            return Err("INVALID_TASK_TARGET".into());
        }
    }
    Ok(request)
}
pub fn validate_response(raw: &str, id: &str) -> Result<Response, String> {
    let response: Response = schema::parse(raw, "Response", MAX_FRAME_BYTES)?;
    let request_id = match &response {
        Response::Success(v) => &v.request_id,
        Response::Failure(v) => &v.request_id,
    };
    if request_id != id {
        return Err("INVALID_RESPONSE".into());
    }
    Ok(response)
}
pub fn validate_backend_frame(raw: &str) -> Result<BackendFrame, String> {
    let frame: BackendFrame = schema::parse(raw, "BackendFrame", MAX_FRAME_BYTES)?;
    let value = serde_json::to_value(&frame).map_err(|e| e.to_string())?;
    if let Some(request) = value.get("request") {
        validate_request(&request.to_string())?;
    }
    if let Some(response) = value.get("response") {
        validate_response(
            &response.to_string(),
            response["requestId"].as_str().ok_or("INVALID_RESPONSE")?,
        )?;
    }
    Ok(frame)
}
pub fn validate_method_result(method: &str, value: &Value) -> bool {
    contract()["methods"]
        .get(method)
        .is_some_and(|method| schema::matches(&method["result"], value))
}

//! Next 3.0 manifest, versioned wire frames and platform request validation.
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
        .get_or_init(|| serde_json::from_str(CONTRACT_JSON).expect("generated Next 3.0 contract"))
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
impl ServiceMethod {
    pub fn name(&self) -> &str {
        match self {
            Self::LegacyServiceMethod(name) => name,
            Self::CapabilityMethod(method) => &method.name,
        }
    }
}

fn validate_capability_schema(rule: &Value) -> Result<(), String> {
    for (min, max) in [
        ("minimum", "maximum"),
        ("minLength", "maxLength"),
        ("minItems", "maxItems"),
    ] {
        if rule[min]
            .as_f64()
            .zip(rule[max].as_f64())
            .is_some_and(|(min, max)| min > max)
        {
            return Err("INVALID_CAPABILITY_SCHEMA".into());
        }
    }
    if let Some(properties) = rule["properties"].as_object() {
        if rule["required"].as_array().is_some_and(|keys| {
            keys.iter()
                .any(|key| !properties.contains_key(key.as_str().unwrap()))
        }) {
            return Err("INVALID_CAPABILITY_SCHEMA".into());
        }
        for child in properties.values() {
            validate_capability_schema(child)?;
        }
    }
    if let Some(items) = rule.get("items") {
        validate_capability_schema(items)?;
    }
    Ok(())
}

pub fn validate_manifest(raw: &str) -> Result<Manifest, String> {
    let manifest: Manifest = schema::parse(raw, "Manifest", BUDGET_MANIFEST_BYTES)?;
    if manifest.id
        == contract()["capabilities"]["builtins"]["providerId"]
            .as_str()
            .unwrap()
    {
        return Err("RESERVED_PROVIDER_ID".into());
    }
    let minimum = semver::Version::parse(&manifest.min_host_version).map_err(|e| e.to_string())?;
    let features = manifest.tools.is_some()
        || manifest
            .permissions
            .iter()
            .any(|permission| matches!(permission.as_str(), "process:run" | "network:download"))
        || manifest
            .contributes
            .commands
            .iter()
            .any(|command| command.placements.is_some() || command.keywords.is_some());
    let required = if manifest.runtime_capabilities.is_some()
        || manifest.runtime_requirements.is_some()
        || manifest.permissions.iter().any(|permission| {
            matches!(
                permission.as_str(),
                "runtime:execute" | "native:publish" | "files:write"
            )
        })
        || manifest
            .dependencies
            .iter()
            .any(|dependency| dependency.id.starts_with("host-"))
    {
        contract()["versions"]["completionHostVersion"]
            .as_str()
            .unwrap()
    } else if features {
        FEATURES_HOST_VERSION
    } else {
        MIN_HOST_VERSION
    };
    if minimum < semver::Version::parse(required).expect("contract minimum host version") {
        return Err("HOST_VERSION_UNDERSTATED".into());
    }
    if let Some(migration) = &manifest.data_migration {
        compatible(&manifest.version, &migration.from_plugin_version)?;
    }
    if manifest.background_interval_ms.is_some()
        && (!manifest.background || !matches!(manifest.backend, Some(Backend::JsBackend(_))))
    {
        return Err("BACKGROUND_INTERVAL_REQUIRES_JS_SERVICE".into());
    }
    inventory(&manifest.files)?;
    if manifest.files.iter().map(|v| v.bytes).sum::<u64>() > BUDGET_PACKAGE_BYTES as u64 {
        return Err("BUDGET_EXCEEDED".into());
    }
    unique(manifest.exports.iter().map(|v| v.id.as_str()))?;
    unique(manifest.dependencies.iter().map(|v| v.id.as_str()))?;
    unique(manifest.resources.iter().map(|v| v.id.as_str()))?;
    let tools = manifest.tools.as_deref().unwrap_or_default();
    unique(tools.iter().map(|tool| tool.id.as_str()))?;
    let runtimes = manifest.runtime_capabilities.as_deref().unwrap_or_default();
    unique(
        runtimes
            .iter()
            .map(|runtime| runtime.capability_id.as_str()),
    )?;
    let requirements = manifest.runtime_requirements.as_deref().unwrap_or_default();
    unique(
        requirements
            .iter()
            .map(|runtime| runtime.capability_id.as_str()),
    )?;
    for requirement in requirements {
        compatible(&requirement.version.min, &requirement.version)?;
        if !manifest
            .dependencies
            .iter()
            .any(|dependency| dependency.id == requirement.capability_id)
        {
            return Err("RUNTIME_DEPENDENCY_UNDECLARED".into());
        }
    }

    if runtimes.iter().any(|runtime| {
        !tools.iter().any(|tool| tool.id == runtime.tool_id)
            || !manifest.exports.iter().any(|export| {
                export.id == runtime.capability_id
                    && export.methods.iter().any(|method| method.name() == "run")
            })
    }) {
        return Err("INVALID_RUNTIME_CAPABILITY".into());
    }
    if !tools.is_empty()
        && (!manifest
            .permissions
            .iter()
            .any(|permission| permission == "process:run")
            || tools.iter().any(|tool| {
                !tool.entry.to_ascii_lowercase().ends_with(".exe")
                    || !manifest
                        .resources
                        .iter()
                        .any(|resource| resource.id == tool.resource_id)
            }))
    {
        return Err("INVALID_PROCESS_TOOL_DECLARATION".into());
    }
    unique(manifest.contributes.commands.iter().map(|v| v.id.as_str()))?;
    for export in &manifest.exports {
        let events = export.events.as_deref().unwrap_or_default();
        if export.methods.is_empty() && events.is_empty() {
            return Err("EMPTY_CAPABILITY".into());
        }
        unique(events.iter().map(|event| event.name.as_str()))?;
        for event in events {
            validate_capability_schema(&event.payload)?;
        }
        let descriptor = serde_json::to_value(export).map_err(|error| error.to_string())?;
        if descriptor.to_string().len() > BUDGET_CAPABILITY_DESCRIPTOR_BYTES {
            return Err("CAPABILITY_DESCRIPTOR_BUDGET_EXCEEDED".into());
        }
        schema::budget(&descriptor, 1, &mut 0, BUDGET_CAPABILITY_DESCRIPTOR_NODES)
            .map_err(|_| "CAPABILITY_DESCRIPTOR_BUDGET_EXCEEDED")?;
        unique(export.methods.iter().map(ServiceMethod::name))?;
        for method in &export.methods {
            if let ServiceMethod::CapabilityMethod(method) = method {
                validate_capability_schema(&method.params)?;
                validate_capability_schema(&method.result)?;
            }
        }
    }
    for dependency in &manifest.dependencies {
        compatible(&dependency.version.min, &dependency.version)?;
        if dependency.provider_id.as_deref() == Some(manifest.id.as_str()) {
            return Err("SELF_DEPENDENCY".into());
        }
    }
    for resource in &manifest.resources {
        compatible(&resource.version.min, &resource.version)?;
    }
    let mut entries = Vec::new();
    if let Some(renderer) = &manifest.renderer {
        if !renderer.ends_with(".js") {
            return Err("INVALID_RENDERER_ENTRY".into());
        }
        entries.push(renderer.as_str());
    } else if manifest.backend.is_none()
        || manifest.contributes.navigation.is_some()
        || manifest
            .contributes
            .file_handlers
            .as_ref()
            .is_some_and(|items| !items.is_empty())
    {
        return Err("RENDERER_REQUIRED".into());
    }
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
        None if manifest.background
            || !manifest.exports.is_empty()
            || !manifest.contributes.commands.is_empty()
            || !tools.is_empty() =>
        {
            return Err("BACKEND_REQUIRED".into())
        }
        None => {}
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
    if manifest.runtime_identity.is_some()
        && (manifest.kind != "runtime" || !manifest.contains_native_code)
    {
        return Err("INVALID_RUNTIME_RESOURCE".into());
    }
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
    let mut request: Request = schema::parse(raw, "Request", MAX_FRAME_BYTES)?;
    if matches!(request.target, Target::PlatformTarget(_)) {
        let method = contract()["methods"]
            .get(&request.method)
            .ok_or("INVALID_PLATFORM_CALL")?;
        if !schema::matches(&method["params"], &request.params) {
            return Err("INVALID_PLATFORM_CALL".into());
        }
        schema::normalize(&method["params"], &mut request.params);
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
    if matches!(&frame, BackendFrame::BackendCancel(cancel) if cancel.task_id.is_some() == cancel.request_id.is_some())
    {
        return Err("INVALID_BACKEND_CANCEL".into());
    }
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

pub fn capability_event<'a>(
    export: &'a ServiceExport,
    name: &str,
) -> Result<&'a CapabilityEvent, String> {
    export
        .events
        .iter()
        .flatten()
        .find(|event| event.name == name)
        .ok_or_else(|| "EVENT_UNDECLARED".into())
}

pub fn validate_event_payload(event: &CapabilityEvent, payload: &Value) -> Result<(), String> {
    if payload.to_string().len() > BUDGET_EVENT_BYTES {
        return Err("EVENT_BUDGET_EXCEEDED".into());
    }
    if !schema::matches(&event.payload, payload) {
        return Err("INVALID_EVENT_PAYLOAD".into());
    }
    Ok(())
}
pub fn validate_method_result(method: &str, value: &Value) -> bool {
    contract()["methods"]
        .get(method)
        .is_some_and(|method| schema::matches(&method["result"], value))
}

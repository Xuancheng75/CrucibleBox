use super::{Execution, Manager};
use base64::{engine::general_purpose::STANDARD, Engine};
use cruciblebox_next2_protocol::{FileEntry, ResourceManifest, ResourceMount, ToolDeclaration};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn executable<'a>(
    tool: &ToolDeclaration,
    (mount, manifest): &'a (ResourceMount, ResourceManifest),
) -> Result<(PathBuf, &'a FileEntry), String> {
    let root = Path::new(&mount.directory);
    if manifest.kind != "runtime" || !manifest.contains_native_code {
        return Err("PROCESS_ENTRY_NOT_APPROVED".into());
    }
    let entry = manifest
        .files
        .iter()
        .find(|file| file.path == tool.entry)
        .ok_or("PROCESS_ENTRY_NOT_APPROVED")?;
    let path = tool
        .entry
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component));
    Ok((path, entry))
}

impl Manager {
    pub(super) fn runtime_status(&self, row: &crate::db::PluginRow) -> Result<Value, String> {
        let plugin = self.installed(row)?;
        let tools = plugin.manifest().tools.as_deref().unwrap_or_default();
        if tools.is_empty() {
            return Ok(json!([]));
        }
        let mounts = self.verified_mounts(row, &plugin)?;
        let status: Result<Vec<_>, String> = tools
            .iter()
            .map(|tool| {
                let mut status =
                    json!({"id":tool.id,"resourceId":tool.resource_id,"state":"missing"});
                if let Some(verified) = mounts
                    .iter()
                    .find(|(mount, _)| mount.id == tool.resource_id)
                {
                    executable(tool, verified)?;
                    status["state"] = json!("ready");
                    status["version"] = json!(verified.0.version);
                }
                Ok(status)
            })
            .collect();
        Ok(json!(status?))
    }

    pub(super) fn run_process(
        &self,
        row: &crate::db::PluginRow,
        params: &Value,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        let (_, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
        if provider != row.name {
            return Err("TASK_AUTHORITY_DENIED".into());
        }
        let plugin = self.installed(row)?;
        let tool = plugin
            .manifest()
            .tools
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|tool| params["toolId"] == tool.id)
            .ok_or("PROCESS_TOOL_UNDECLARED")?;
        let mounts = self.verified_mounts(row, &plugin)?;
        let mount = mounts
            .iter()
            .find(|(mount, _)| mount.id == tool.resource_id)
            .ok_or("PROCESS_RESOURCE_MISSING")?;
        let (path, entry) = executable(tool, mount)?;
        let mut binary = crate::next2_files::open_readonly(&path)?;
        let mut digest = Sha256::new();
        let bytes =
            std::io::copy(&mut binary, &mut digest).map_err(|_| "PROCESS_ENTRY_UNAVAILABLE")?;
        if bytes != entry.bytes || format!("{:x}", digest.finalize()) != entry.sha256 {
            return Err("PROCESS_ENTRY_INTEGRITY_MISMATCH".into());
        }
        let args: Vec<String> = serde_json::from_value(params["args"].clone())
            .map_err(|error| format!("INVALID_REQUEST: {error}"))?;
        let input = STANDARD
            .decode(params["stdin"].as_str().unwrap_or_default())
            .map_err(|_| "INVALID_PROCESS_INPUT")?;
        if input.len() > 4096 || args.iter().any(|arg| arg.contains('\0')) {
            return Err("INVALID_PROCESS_INPUT".into());
        }
        let root = Self::writable_data(row)?;
        let scratch = tempfile::Builder::new()
            .prefix("process-")
            .tempdir_in(root)
            .map_err(|error| format!("PROCESS_WORKSPACE_FAILED: {error}"))?;
        let deadline =
            Instant::now() + Duration::from_millis(params["timeoutMs"].as_u64().unwrap_or(30000));
        let directory = params["workingDirectory"]
            .as_str()
            .map(|reference| self.resolve_file(&row.name, execution.caller, reference))
            .transpose()?;
        let directory = directory
            .as_deref()
            .unwrap_or(scratch.path())
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let authorized_directory = crate::next2_files::open_authorized(&directory)?;
        if !authorized_directory
            .metadata()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            return Err("DIRECTORY_REQUIRED".into());
        }
        let environment: std::collections::HashMap<String, String> = params
            .get("environment")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|_| "INVALID_PROCESS_ENVIRONMENT")?
            .unwrap_or_default();
        if environment.len() > 32 {
            return Err("PROCESS_ENVIRONMENT_LIMIT".into());
        }
        let mut names = std::collections::HashSet::new();
        for (name, value) in &environment {
            if name.is_empty()
                || name.len() > 128
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || name.as_bytes()[0].is_ascii_digit()
                || value.contains('\0')
                || !names.insert(name.to_ascii_uppercase())
            {
                return Err("INVALID_PROCESS_ENVIRONMENT".into());
            }
            if matches!(
                name.to_ascii_uppercase().as_str(),
                "TEMP" | "TMP" | "TMPDIR" | "SYSTEMROOT"
            ) {
                return Err("PROCESS_ENVIRONMENT_RESERVED".into());
            }
        }
        let output = cruciblebox_next2_runtime::tool::run(
            &path,
            &directory,
            scratch.path(),
            &environment,
            &args,
            &input,
            deadline,
            || execution.check().and_then(|()| self.check(row)),
        )?;
        Ok(json!({
            "exitCode":output.exit_code,
            "stdout":STANDARD.encode(output.stdout),
            "stderr":STANDARD.encode(output.stderr),
        }))
    }
}

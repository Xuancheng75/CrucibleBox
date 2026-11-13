//! Trusted native providers use the host publication journal and task identity.
use super::{Execution, Manager};
use serde_json::Value;
use std::path::Path;

impl Manager {
    pub(super) fn publish_native(
        &self,
        row: &crate::db::PluginRow,
        params: &Value,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        let (context, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
        if provider != row.name || params["taskId"] != context.task_id() {
            return Err("TASK_AUTHORITY_DENIED".into());
        }
        let plugin = self.installed(row)?;
        if !matches!(
            plugin.manifest().backend,
            Some(cruciblebox_next2_protocol::Backend::NativeBackend(_))
        ) {
            return Err("NATIVE_PROVIDER_REQUIRED".into());
        }
        let stage = Path::new(params["stage"].as_str().unwrap());
        let target = Path::new(params["target"].as_str().unwrap());
        if !stage.is_absolute() || !target.is_absolute() || target.file_name().is_none() {
            return Err("INVALID_PUBLICATION_PATH".into());
        }
        let stage_parent = stage
            .parent()
            .ok_or("INVALID_PUBLICATION_PATH")?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let target_parent = target
            .parent()
            .ok_or("INVALID_PUBLICATION_PATH")?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if stage_parent != target_parent {
            return Err("PUBLICATION_MUST_USE_SIBLING_STAGE".into());
        }
        let stage = stage.canonicalize().map_err(|error| error.to_string())?;
        let target = target_parent.join(target.file_name().unwrap());
        let _parent = crate::next2_files::open_authorized(&target_parent)?;
        drop(crate::next2_files::open_authorized(&stage)?);
        if target.exists() {
            let _target = crate::next2_files::open_authorized(&target)?;
        }
        let reference = params["reference"].as_str().map(Path::new);
        // The shared publication authority validates a canonical directory
        // ancestor before writing its journal. This also supports one result
        // reference for a batch that merges files into nested directories.
        execution.check()?;
        context.publish_object(
            &stage,
            &target,
            params["replace"].as_bool().unwrap(),
            params["terminal"].as_bool().unwrap(),
            reference,
        )
    }
}

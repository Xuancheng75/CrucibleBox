use super::{
    file_operations::{child, entries, name},
    Execution, Manager,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn copy(
    source: &Path,
    target: &Path,
    execution: Execution<'_>,
    completed: &mut u64,
    total: u64,
) -> Result<(), String> {
    let mut input = crate::next2_files::open_readonly(source)?;
    let expected = input.metadata().map_err(|error| error.to_string())?.len();
    if expected > 17179869184 {
        return Err("FILE_BUDGET_EXCEEDED".into());
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(target)
        .map_err(|error| error.to_string())?;
    let mut buffer = [0; 65536];
    let mut received = 0;
    let mut percent = 0;
    loop {
        execution.check()?;
        let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        received += count as u64;
        *completed += count as u64;
        if received > expected || *completed > 17179869184 {
            return Err("FILE_CHANGED_DURING_COPY".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| error.to_string())?;
        let next = ((*completed * 95 / total.max(1)).min(95)) as u32;
        if next != percent {
            percent = next;
            execution.task.unwrap().0.update_progress(
                "copy",
                percent,
                "复制文件",
                Some(json!({"bytes":completed,"total":total})),
            );
        }
    }
    if received != expected {
        return Err("FILE_CHANGED_DURING_COPY".into());
    }
    output.sync_all().map_err(|error| error.to_string())
}

impl Manager {
    pub(super) fn transfer_file(
        &self,
        owner: &str,
        params: &Value,
        method: &str,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        let (context, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
        if provider != owner {
            return Err("TASK_AUTHORITY_DENIED".into());
        }
        let source = self.resolve_file(owner, execution.caller, params["ref"].as_str().unwrap())?;
        let source_handle = crate::next2_files::open_authorized(&source)?;
        let directory = source_handle
            .metadata()
            .map_err(|error| error.to_string())?
            .is_dir();
        let parent = source.parent().ok_or("FILE_ROOT_MUTATION_DENIED")?;
        let _source_parent = crate::next2_files::open_authorized(parent)?;
        let recursive = params["recursive"].as_bool().unwrap();
        if method == "files.remove" {
            if directory && recursive {
                entries(&source, true, true)?;
            }
            execution.check()?;
            context.begin_external_commit()?;
            drop(source_handle);
            if directory {
                if recursive {
                    fs::remove_dir_all(&source)
                } else {
                    fs::remove_dir(&source)
                }
            } else {
                fs::remove_file(&source)
            }
            .map_err(|error| error.to_string())?;
            return Ok(json!(true));
        }
        if directory && !recursive {
            return Err("RECURSIVE_OPERATION_REQUIRED".into());
        }
        let target_root = self.resolve_file(
            owner,
            execution.caller,
            params["directory"].as_str().unwrap(),
        )?;
        let target_name = params["name"].as_str().unwrap();
        name(target_name)?;
        let (target, _target_parent) = child(&target_root, target_name)?;
        if target == source || (directory && target.starts_with(&source)) {
            return Err("FILE_DESTINATION_OVERLAPS_SOURCE".into());
        }
        let replace = params["replace"].as_bool().unwrap();
        if target.exists() {
            if !replace {
                return Err("FILE_ALREADY_EXISTS".into());
            }
            drop(crate::next2_files::open_authorized(&target)?);
        }
        context.update_progress("copy", 0, "复制文件", None);
        let published = if directory {
            let values = entries(&source, true, true)?;
            let mut total = 0u64;
            for (path, _) in &values {
                total = total
                    .checked_add(fs::metadata(path).map_err(|error| error.to_string())?.len())
                    .ok_or("FILE_BUDGET_EXCEEDED")?;
            }
            if total > 17179869184 {
                return Err("FILE_BUDGET_EXCEEDED".into());
            }
            let stage = tempfile::Builder::new()
                .prefix(".directory-stage-")
                .tempdir_in(&target_root)
                .map_err(|error| error.to_string())?;
            let mut completed = 0;
            for (path, relative) in values {
                execution.check()?;
                let destination = stage.path().join(relative.replace('/', "\\"));
                if path.is_dir() {
                    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
                } else {
                    if let Some(parent) = destination.parent() {
                        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                    }
                    copy(&path, &destination, execution, &mut completed, total)?;
                }
            }
            execution.check()?;
            if method == "files.move" {
                context.begin_external_commit()?;
            }
            let result = context.publish_object(stage.path(), &target, replace, false, None);
            if result.is_err() && context.publication_pending() {
                let _retained = stage.keep();
            }
            result?;
            target
        } else {
            let transaction = crate::output_transaction::OutputTransaction::new(&target, replace)?;
            let mut completed = 0;
            let total = source_handle
                .metadata()
                .map_err(|error| error.to_string())?
                .len();
            copy(
                &source,
                transaction.stage_path(),
                execution,
                &mut completed,
                total,
            )?;
            execution.check()?;
            if method == "files.move" {
                context.begin_external_commit()?;
            }
            transaction.publish_durable(context, false, |_| Ok(()))?
        };
        drop(source_handle);
        if method == "files.move" {
            if directory {
                fs::remove_dir_all(&source)
            } else {
                fs::remove_file(&source)
            }
            .map_err(|error| format!("MOVE_SOURCE_RETAINED: {error}"))?;
        }
        self.remember_file(execution.caller.unwrap_or(owner), &published)
    }
}

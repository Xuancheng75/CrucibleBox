//! Mutable file operations stay behind owner-bound references and host tasks.
use super::{Execution, Manager};
use base64::{engine::general_purpose::STANDARD, Engine};
use notify::Watcher;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

struct Writer {
    file: File,
    transaction: crate::output_transaction::OutputTransaction,
    _parent: File,
    owner: String,
    recipient: String,
    session: String,
    task: String,
    received: u64,
    expected: Option<u64>,
    expires: Instant,
}
struct Temporary {
    owner: String,
    recipient: String,
    session: String,
    _directory: tempfile::TempDir,
}
struct Watch {
    _watcher: notify::RecommendedWatcher,
    receiver: mpsc::Receiver<notify::Result<notify::Event>>,
    overflow: Arc<AtomicBool>,
    root: PathBuf,
    directory: bool,
    owner: String,
    caller: Option<String>,
    session: String,
}
#[derive(Default)]
struct State {
    writers: HashMap<String, Writer>,
    temporary: HashMap<String, Temporary>,
    watches: HashMap<String, Watch>,
}
#[derive(Default)]
pub(super) struct MutableFiles(Mutex<State>);
impl MutableFiles {
    pub(super) fn clear_task(&self, task: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?
            .writers
            .retain(|_, writer| writer.task != task);
        Ok(())
    }
    pub(super) fn clear_session(&self, session: &str) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
        state.writers.retain(|_, value| value.session != session);
        state.watches.retain(|_, value| value.session != session);
        state.temporary.retain(|_, value| value.session != session);
        Ok(())
    }
    pub(super) fn clear_owner(&self, owner: &str) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
        state
            .writers
            .retain(|_, value| value.owner != owner && value.recipient != owner);
        state
            .watches
            .retain(|_, value| value.owner != owner && value.caller.as_deref() != Some(owner));
        state
            .temporary
            .retain(|_, value| value.owner != owner && value.recipient != owner);
        Ok(())
    }
}
fn budget(state: &State, session: &str) -> Result<(), String> {
    let total = state.writers.len() + state.temporary.len() + state.watches.len();
    let active = state
        .writers
        .values()
        .filter(|value| value.session == session)
        .count()
        + state
            .temporary
            .values()
            .filter(|value| value.session == session)
            .count()
        + state
            .watches
            .values()
            .filter(|value| value.session == session)
            .count();
    if total >= 64 || active >= 8 {
        return Err("FILE_OPERATIONS_LIMIT".into());
    }
    Ok(())
}
pub(super) fn name(value: &str) -> Result<(), String> {
    let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|number| {
                matches!(
                    number,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
    if value.is_empty()
        || value.chars().count() > 240
        || value
            .chars()
            .any(|ch| ch.is_control() || "<>:\"/\\|?*".contains(ch))
        || value.ends_with(['.', ' '])
        || device
    {
        return Err("INVALID_FILE_NAME".into());
    }
    Ok(())
}
pub(super) fn child(root: &Path, relative: &str) -> Result<(PathBuf, File), String> {
    if !crate::next2_files::open_authorized(root)?
        .metadata()
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("DIRECTORY_REQUIRED".into());
    }
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        name(component)?;
        path.push(component);
    }
    let parent = path
        .parent()
        .ok_or("INVALID_FILE_NAME")?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !parent.starts_with(root) {
        return Err("FILE_AUTHORITY_DENIED".into());
    }
    let handle = crate::next2_files::open_authorized(&parent)?;
    Ok((
        parent.join(path.file_name().ok_or("INVALID_FILE_NAME")?),
        handle,
    ))
}
pub(super) fn entries(
    root: &Path,
    recursive: bool,
    reject_links: bool,
) -> Result<Vec<(PathBuf, String)>, String> {
    use std::os::windows::fs::MetadataExt;
    let mut pending = vec![root.to_path_buf()];
    let mut result = Vec::new();
    while let Some(directory) = pending.pop() {
        let _directory = crate::next2_files::open_authorized(&directory)?;
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                if reject_links {
                    return Err("FILE_AUTHORITY_DENIED".into());
                }
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "FILE_AUTHORITY_DENIED")?
                .to_string_lossy()
                .replace('\\', "/");
            if relative.len() > 1024 || result.len() >= 4096 {
                return Err("DIRECTORY_WALK_LIMIT".into());
            }
            if recursive && metadata.is_dir() {
                pending.push(path.clone());
            }
            result.push((path, relative));
        }
    }
    result.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(result)
}

impl Manager {
    pub(super) fn file_operation(
        &self,
        owner: &str,
        row: &crate::db::PluginRow,
        request: &cruciblebox_next2_protocol::Request,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        let params = &request.params;
        let session = &request.session;
        let recipient = execution.caller.unwrap_or(owner);
        match request.method.as_str() {
            "files.child" => {
                let root =
                    self.resolve_file(owner, execution.caller, params["ref"].as_str().unwrap())?;
                let (path, _parent) = child(&root, params["path"].as_str().unwrap())?;
                drop(crate::next2_files::open_authorized(&path)?);
                serde_json::to_value(self.files.grant(recipient, &path)?)
                    .map_err(|error| error.to_string())
            }
            "files.directory.create" => {
                let root =
                    self.resolve_file(owner, execution.caller, params["ref"].as_str().unwrap())?;
                name(params["name"].as_str().unwrap())?;
                let (path, _parent) = child(&root, params["name"].as_str().unwrap())?;
                fs::create_dir(&path).map_err(|error| error.to_string())?;
                self.remember_file(recipient, &path)
            }
            "files.directory.walk" => {
                let root =
                    self.resolve_file(owner, execution.caller, params["ref"].as_str().unwrap())?;
                let values = entries(&root, params["recursive"].as_bool().unwrap(), false)?;
                let after = params["after"].as_str().unwrap_or("");
                let limit = params["limit"].as_u64().unwrap() as usize;
                let selected: Vec<_> = values
                    .iter()
                    .filter(|(_, path)| path.as_str() > after)
                    .take(limit + 1)
                    .collect();
                let mut items = Vec::new();
                for (path, relative) in selected.iter().take(limit) {
                    items.push(json!({"path":relative,"info":crate::next2_files::info(path)?}));
                }
                let mut page = json!({"items":items});
                if selected.len() > limit {
                    page["next"] = json!(selected[limit - 1].1);
                }
                Ok(page)
            }
            "files.temporary" => {
                let name = params["name"].as_str().unwrap();
                super::file_operations::name(name)?;
                let mut state = self
                    .file_operations
                    .0
                    .lock()
                    .map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
                budget(&state, session)?;
                let directory = tempfile::Builder::new()
                    .prefix("temporary-")
                    .tempdir_in(Self::writable_data(row)?)
                    .map_err(|error| error.to_string())?;
                let path = directory.path().join(name);
                if params["kind"] == "directory" {
                    fs::create_dir(&path)
                } else {
                    File::create(&path).map(|_| ())
                }
                .map_err(|error| error.to_string())?;
                let reference = self.files.grant(recipient, &path)?;
                state.temporary.insert(
                    reference.r#ref.clone(),
                    Temporary {
                        owner: owner.into(),
                        recipient: recipient.into(),
                        session: session.clone(),
                        _directory: directory,
                    },
                );
                serde_json::to_value(reference).map_err(|error| error.to_string())
            }
            "files.temporary.release" => {
                let mut state = self
                    .file_operations
                    .0
                    .lock()
                    .map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
                let id = params["ref"].as_str().unwrap();
                if !state
                    .temporary
                    .get(id)
                    .is_some_and(|value| value.session == *session)
                {
                    return Err("FILE_AUTHORITY_DENIED".into());
                }
                fs::remove_dir_all(state.temporary.get(id).unwrap()._directory.path())
                    .map_err(|error| format!("FILE_TEMPORARY_CLEANUP_FAILED: {error}"))?;
                state.temporary.remove(id);
                Ok(json!(true))
            }
            "files.write.open" => {
                let (context, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
                if provider != owner {
                    return Err("TASK_AUTHORITY_DENIED".into());
                }
                let (target, parent) = if let Some(reference) = params["ref"].as_str() {
                    let target = self.resolve_file(owner, execution.caller, reference)?;
                    drop(crate::next2_files::open_authorized(&target)?);
                    let parent = crate::next2_files::open_authorized(
                        target.parent().ok_or("INVALID_FILE_NAME")?,
                    )?;
                    (target, parent)
                } else if let Some(reference) = params["directory"].as_str() {
                    let root = self.resolve_file(owner, execution.caller, reference)?;
                    name(params["name"].as_str().unwrap())?;
                    child(&root, params["name"].as_str().unwrap())?
                } else {
                    let root = Self::writable_data(&self.record(recipient)?)?;
                    let results = super::managed_child(&root, "results")?;
                    let directory = super::managed_child(&results, &context.task_id())?;
                    name(params["name"].as_str().unwrap())?;
                    child(
                        &directory
                            .canonicalize()
                            .map_err(|error| error.to_string())?,
                        params["name"].as_str().unwrap(),
                    )?
                };
                let replace = params["replace"].as_bool().unwrap_or(false);
                if target.exists() && !replace {
                    return Err("FILE_ALREADY_EXISTS".into());
                }
                let transaction =
                    crate::output_transaction::OutputTransaction::new(&target, replace)?;
                let file = File::options()
                    .write(true)
                    .open(transaction.stage_path())
                    .map_err(|error| error.to_string())?;
                let id = crate::rand_token::random_token_hex()?;
                let mut state = self
                    .file_operations
                    .0
                    .lock()
                    .map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
                state
                    .writers
                    .retain(|_, writer| Instant::now() < writer.expires);
                budget(&state, session)?;
                state.writers.insert(
                    id.clone(),
                    Writer {
                        file,
                        transaction,
                        _parent: parent,
                        owner: owner.into(),
                        recipient: recipient.into(),
                        session: session.clone(),
                        task: context.task_id(),
                        received: 0,
                        expected: params["bytes"].as_u64(),
                        expires: Instant::now() + Duration::from_secs(300),
                    },
                );
                Ok(json!({"streamId":id}))
            }
            "files.write.append" | "files.write.abort" | "files.write.commit" => {
                let (context, provider) = execution.task.ok_or("TASK_EXECUTION_REQUIRED")?;
                if provider != owner {
                    return Err("TASK_AUTHORITY_DENIED".into());
                }
                let id = params["streamId"].as_str().unwrap();
                let mut state = self
                    .file_operations
                    .0
                    .lock()
                    .map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
                let writer = state
                    .writers
                    .get_mut(id)
                    .filter(|writer| {
                        writer.session == *session
                            && writer.task == context.task_id()
                            && Instant::now() < writer.expires
                    })
                    .ok_or("FILE_STREAM_UNAVAILABLE")?;
                if request.method == "files.write.append" {
                    let bytes = STANDARD
                        .decode(params["data"].as_str().unwrap())
                        .map_err(|_| "INVALID_FILE_DATA")?;
                    if bytes.len() > 24576
                        || writer.received + bytes.len() as u64
                            > writer.expected.unwrap_or(17179869184)
                    {
                        return Err("FILE_BUDGET_EXCEEDED".into());
                    }
                    writer
                        .file
                        .write_all(&bytes)
                        .map_err(|error| error.to_string())?;
                    writer.received += bytes.len() as u64;
                    writer.expires = Instant::now() + Duration::from_secs(300);
                    return Ok(json!({"bytes":writer.received}));
                }
                let writer = state.writers.remove(id).unwrap();
                drop(state);
                if request.method == "files.write.abort" {
                    return Ok(json!(true));
                }
                if writer
                    .expected
                    .is_some_and(|expected| writer.received != expected)
                {
                    return Err("FILE_SIZE_MISMATCH".into());
                }
                writer.file.sync_all().map_err(|error| error.to_string())?;
                drop(writer.file);
                execution.check()?;
                let path = writer
                    .transaction
                    .publish_durable(context, false, |_| Ok(()))?;
                self.remember_file(&writer.recipient, &path)
            }
            "files.watch" => {
                let root =
                    self.resolve_file(owner, execution.caller, params["ref"].as_str().unwrap())?;
                let authorized = crate::next2_files::open_authorized(&root)?;
                let directory = authorized
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .is_dir();
                drop(authorized);
                let (sender, receiver) = mpsc::sync_channel(128);
                let overflow = Arc::new(AtomicBool::new(false));
                let dropped = overflow.clone();
                let mut watcher = notify::recommended_watcher(move |event| {
                    if sender.try_send(event).is_err() {
                        dropped.store(true, Ordering::SeqCst);
                    }
                })
                .map_err(|error| error.to_string())?;
                watcher
                    .watch(
                        &root,
                        if params["recursive"].as_bool().unwrap() {
                            notify::RecursiveMode::Recursive
                        } else {
                            notify::RecursiveMode::NonRecursive
                        },
                    )
                    .map_err(|error| error.to_string())?;
                let mut state = self
                    .file_operations
                    .0
                    .lock()
                    .map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
                budget(&state, session)?;
                let id = crate::rand_token::random_token_hex()?;
                state.watches.insert(
                    id.clone(),
                    Watch {
                        _watcher: watcher,
                        receiver,
                        overflow,
                        root,
                        directory,
                        owner: owner.into(),
                        caller: execution.caller.map(str::to_owned),
                        session: session.clone(),
                    },
                );
                Ok(json!({"watchId":id}))
            }
            "files.watch.poll" | "files.watch.close" => {
                let mut state = self
                    .file_operations
                    .0
                    .lock()
                    .map_err(|_| "FILE_OPERATIONS_UNAVAILABLE")?;
                let id = params["watchId"].as_str().unwrap();
                let watch = state
                    .watches
                    .get(id)
                    .filter(|watch| watch.session == *session)
                    .ok_or("FILE_WATCH_UNAVAILABLE")?;
                if request.method == "files.watch.close" {
                    state.watches.remove(id);
                    return Ok(json!(true));
                }
                let mut events = Vec::new();
                let mut overflow = watch.overflow.swap(false, Ordering::SeqCst);
                for _ in 0..16 {
                    let event = match watch.receiver.try_recv() {
                        Ok(event) => {
                            event.map_err(|error| format!("FILE_WATCH_FAILED: {error}"))?
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(_) => return Err("FILE_WATCH_UNAVAILABLE".into()),
                    };
                    let kind = match event.kind {
                        notify::EventKind::Create(_) => "created",
                        notify::EventKind::Remove(_) => "removed",
                        _ => "modified",
                    };
                    for path in event.paths {
                        if (!watch.directory && path != watch.root)
                            || !path.starts_with(&watch.root)
                        {
                            continue;
                        }
                        let relative = path
                            .strip_prefix(&watch.root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/");
                        let relative = if relative.is_empty() {
                            watch
                                .root
                                .file_name()
                                .unwrap()
                                .to_string_lossy()
                                .into_owned()
                        } else {
                            relative
                        };
                        if events.len() == 16 || relative.len() > 1024 {
                            overflow = true;
                            continue;
                        }
                        events.push(json!({"path":relative,"kind":kind}));
                    }
                }
                Ok(json!({"events":events,"overflow":overflow}))
            }
            "files.copy" | "files.move" | "files.remove" => {
                self.transfer_file(owner, params, &request.method, execution)
            }
            _ => Err("PLATFORM_METHOD_NOT_CONNECTED".into()),
        }
    }
}

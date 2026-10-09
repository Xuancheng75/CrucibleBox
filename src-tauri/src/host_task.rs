//! Persistent host task snapshots. Executors own terminal transitions; the
//! renderer consumes snapshots and ordered events rather than retaining truth.

use crate::db::Db;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{Emitter, State, WebviewWindow};

type HostEmitter = Arc<dyn Fn(&str, Value) + Send + Sync>;

use cruciblebox_repository::task_projection::upsert_projection;
pub use cruciblebox_repository::task_projection::{
    list, recover, remove_terminal, upsert, HostTaskMutation, HostTaskSnapshot,
};
fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn should_emit(task: &HostTaskSnapshot) -> bool {
    static LAST_EMIT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let last = LAST_EMIT.get_or_init(|| Mutex::new(HashMap::new()));
    let now = Instant::now();
    {
        let mut last = last.lock().unwrap();
        if is_terminal(&task.status)
            || last
                .get(&task.id)
                .is_none_or(|previous| now.duration_since(*previous) >= Duration::from_millis(100))
        {
            last.insert(task.id.clone(), now);
            true
        } else {
            false
        }
    }
}

fn emit_task(window: &WebviewWindow, task: &HostTaskSnapshot) {
    if should_emit(task) {
        let _ = window.emit("host:task", task);
    }
}

pub fn executor_observer(
    db: Arc<Mutex<Db>>,
    emitter: HostEmitter,
    owner: &'static str,
) -> Arc<dyn Fn(Value) + Send + Sync> {
    Arc::new(move |snapshot| {
        let Some(id) = snapshot.get("taskId").and_then(Value::as_str) else {
            return;
        };
        let Some(sequence) = snapshot.get("sequence").and_then(Value::as_u64) else {
            return;
        };
        let kind = snapshot
            .get("resourceKey")
            .and_then(Value::as_str)
            .unwrap_or("general");
        let status = match snapshot.get("status").and_then(Value::as_str) {
            Some("succeeded") => "completed",
            Some("queued") => "queued",
            Some("running") => "running",
            Some("paused") => "paused",
            Some("failed" | "interrupted") => "failed",
            Some("cancelled") => "cancelled",
            _ => return,
        };
        let progress = snapshot.get("progress");
        let refs = if let Some(refs) = snapshot.get("resultRefs").and_then(Value::as_array) {
            refs.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        } else {
            snapshot
                .get("result")
                .into_iter()
                .flat_map(|result| {
                    ["outputPath", "path", "filePath", "destination"]
                        .into_iter()
                        .filter_map(|key| {
                            result.get(key).and_then(Value::as_str).map(str::to_string)
                        })
                })
                .collect()
        };
        let change = HostTaskMutation {
            id: id.to_string(),
            owner: Some(owner.into()),
            kind: Some(kind.into()),
            source: Some(if owner == "marketplace" {
                "marketplace".into()
            } else {
                "plugin".into()
            }),
            title: Some(if owner == "marketplace" {
                "下载插件".into()
            } else if owner == "unienv" {
                "运行时安装".into()
            } else if owner == "archive-extractor" {
                "归档任务".into()
            } else if owner == "plugin-install" {
                "插件安装".into()
            } else if owner == "plugin-process" {
                "插件进程任务".into()
            } else {
                format!("文档任务 · {kind}")
            }),
            detail: progress
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string),
            status: Some(status.into()),
            stage: progress
                .and_then(|value| value.get("stage"))
                .and_then(Value::as_str)
                .map(str::to_string),
            progress: if status == "completed" {
                Some(100)
            } else {
                progress
                    .and_then(|value| value.get("percent"))
                    .and_then(Value::as_i64)
            },
            error: snapshot
                .get("error")
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string),
            result_refs: Some(refs),
            executor_key: Some(format!("{owner}.{kind}")),
            checkpoint_ref: None,
            parent_task_id: None,
        };
        let result = {
            let db = db.lock().unwrap();
            upsert_projection(&db, change, Some(sequence))
        };
        match result {
            Ok(task) if should_emit(&task) => {
                if let Ok(payload) = serde_json::to_value(task) {
                    emitter("host:task", payload);
                }
            }
            Ok(_) => {}
            Err(error) => eprintln!("[host-task] failed to record {owner} task {id}: {error}"),
        }
    })
}

#[tauri::command(async)]
pub fn host_tasks_list(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
) -> Result<Vec<HostTaskSnapshot>, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    list(&db.lock().unwrap())
}

#[tauri::command(async)]
pub fn host_task_upsert(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    change: HostTaskMutation,
) -> Result<HostTaskSnapshot, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    let task = upsert(&db.lock().unwrap(), change)?;
    emit_task(&window, &task);
    Ok(task)
}

#[tauri::command(async)]
pub fn host_task_reveal_result(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: String,
    path: String,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    let task = {
        let db = db.lock().unwrap();
        cruciblebox_repository::task_projection::get(&db, &id)?.ok_or("task not found")?
    };
    if !task.result_refs.contains(&path) {
        return Err("path is not a task result".into());
    }
    let path = std::path::Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("结果文件不可用：{error}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let path_text = path.to_string_lossy();
        let path_text = if let Some(unc) = path_text.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            path_text
                .strip_prefix("\\\\?\\")
                .unwrap_or(&path_text)
                .to_string()
        };
        let argument = if path.is_dir() {
            path_text
        } else {
            format!("/select,{path_text}")
        };
        std::process::Command::new("explorer.exe")
            .arg(argument)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|error| format!("打开结果位置失败：{error}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("result reveal is only supported on Windows".into())
    }
}

#[tauri::command(async)]
pub fn host_task_cancel(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    runtime: State<'_, Arc<crate::task_runtime::TaskRuntime>>,
    id: String,
) -> Result<bool, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    let task = {
        let db = db.lock().unwrap();
        cruciblebox_repository::task_projection::get(&db, &id)?.ok_or("task not found")?
    };
    if !matches!(task.status.as_str(), "queued" | "running" | "paused") {
        return Ok(false);
    }
    if task.source == "marketplace" {
        return crate::commands::marketplace_cancel_task(window, id);
    }
    Ok(match task.owner.as_str() {
        "document-engine" => runtime.cancel("document-engine", &id),
        "unienv" => runtime.cancel("unienv", &id),
        "archive-extractor" => runtime.cancel("archive-extractor", &id),
        "plugin-process" => runtime.cancel("plugin-process", &id),
        "plugin-install" => runtime.cancel("plugin-install", &id),
        _ => false,
    })
}

#[tauri::command(async)]
pub fn host_tasks_remove_terminal(
    window: WebviewWindow,
    db: State<'_, Arc<Mutex<Db>>>,
    id: Option<String>,
) -> Result<usize, String> {
    if window.label() != "main" {
        return Err("unauthorized".into());
    }
    remove_terminal(&db.lock().unwrap(), id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db() -> (Db, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "cb-host-task-{}-{}",
            std::process::id(),
            crate::rand_token::random_token_alnum(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (Db::open(&dir.join("tasks.db")).unwrap(), dir)
    }

    fn change(id: &str, status: &str, progress: i64) -> HostTaskMutation {
        HostTaskMutation {
            id: id.into(),
            owner: Some("document-engine".into()),
            kind: Some("pdf-convert".into()),
            source: Some("plugin".into()),
            title: Some("转换 PDF".into()),
            detail: None,
            status: Some(status.into()),
            stage: Some("convert".into()),
            progress: Some(progress),
            error: None,
            result_refs: None,
            executor_key: None,
            checkpoint_ref: None,
            parent_task_id: None,
        }
    }

    #[test]
    fn journal_persists_sequence_and_refuses_progress_regression() {
        let (db, dir) = temp_db();
        assert_eq!(
            upsert(&db, change("job-1", "queued", 0)).unwrap().sequence,
            1
        );
        let running = upsert(&db, change("job-1", "running", 70)).unwrap();
        assert_eq!(running.sequence, 2);
        let late = upsert(&db, change("job-1", "running", 20)).unwrap();
        assert_eq!(late.progress, 70);
        assert_eq!(late.sequence, 3);
        let done = upsert(&db, change("job-1", "completed", 100)).unwrap();
        assert_eq!(done.sequence, 4);
        assert!(upsert(&db, change("job-1", "running", 10)).is_err());
        drop(db);
        let reopened = Db::open(&dir.join("tasks.db")).unwrap();
        let snapshot = list(&reopened).unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].status, "completed");
        assert_eq!(snapshot[0].sequence, 4);
        drop(reopened);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restart_marks_unfinished_task_interrupted() {
        let (db, dir) = temp_db();
        upsert(&db, change("job-2", "running", 42)).unwrap();
        assert_eq!(recover(&db).unwrap(), 1);
        let snapshot = list(&db).unwrap();
        assert_eq!(snapshot[0].status, "failed");
        assert_eq!(snapshot[0].progress, 42);
        assert_eq!(snapshot[0].sequence, 2);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restart_keeps_valid_checkpoint_recoverable() {
        let (db, dir) = temp_db();
        let checkpoint = dir.join("checkpoint.json");
        std::fs::write(&checkpoint, b"{}").unwrap();
        let mut task = change("job-3", "running", 24);
        task.checkpoint_ref = Some(checkpoint.to_string_lossy().into_owned());
        upsert(&db, task).unwrap();
        assert_eq!(recover(&db).unwrap(), 1);
        let snapshot = list(&db).unwrap();
        assert_eq!(snapshot[0].status, "paused");
        assert_eq!(snapshot[0].stage, "recoverable");
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authoritative_replay_corrects_projection_and_rejects_renderer_and_late_events() {
        let (db, dir) = temp_db();
        upsert(&db, change("owned", "failed", 20)).unwrap();
        let corrected = upsert_projection(&db, change("owned", "completed", 100), Some(4)).unwrap();
        assert_eq!(corrected.sequence, 2);
        assert_eq!(corrected.status, "completed");
        assert!(upsert(&db, change("owned", "running", 80))
            .unwrap_err()
            .contains("executor-owned"));
        assert_eq!(
            upsert_projection(&db, change("owned", "running", 80), Some(3))
                .unwrap()
                .status,
            "completed"
        );
        let mut wrong_owner = change("owned", "cancelled", 100);
        wrong_owner.owner = Some("unienv".into());
        assert!(upsert_projection(&db, wrong_owner, Some(5)).is_err());
        upsert_projection(&db, change("active", "running", 10), Some(1)).unwrap();
        assert_eq!(recover(&db).unwrap(), 0);
        drop(db);
        let db = Db::open(&dir.join("tasks.db")).unwrap();
        assert!(upsert(&db, change("owned", "running", 80)).is_err());
        assert_eq!(
            list(&db)
                .unwrap()
                .iter()
                .find(|task| task.id == "owned")
                .unwrap()
                .status,
            "completed"
        );
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn executor_adapter_persists_terminal_result_reference() {
        let (db, dir) = temp_db();
        let db = Arc::new(Mutex::new(db));
        let observer = executor_observer(db.clone(), Arc::new(|_, _| {}), "document-engine");
        observer(serde_json::json!({
            "taskId": "pdf-1", "resourceKey": "convert", "status": "queued", "sequence": 1
        }));
        observer(serde_json::json!({
            "taskId": "pdf-1", "resourceKey": "convert", "status": "running", "sequence": 2,
            "progress": { "stage": "render", "percent": 60, "message": "处理中" }
        }));
        observer(serde_json::json!({
            "taskId": "pdf-1", "resourceKey": "convert", "status": "succeeded", "sequence": 3,
            "result": { "outputPath": "C:/exports/result.pdf" }
        }));
        let tasks = list(&db.lock().unwrap()).unwrap();
        assert_eq!(tasks[0].status, "completed");
        assert_eq!(tasks[0].progress, 100);
        assert_eq!(tasks[0].result_refs, ["C:/exports/result.pdf"]);
        drop(observer);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

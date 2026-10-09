//! Durable task-centre projection. The task runtime remains the execution authority.
use crate::Db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
const MAX_TASKS: i64 = 200;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostTaskSnapshot {
    pub id: String,
    pub owner: String,
    pub kind: String,
    pub source: String,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub stage: String,
    pub progress: i64,
    pub sequence: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
    pub result_refs: Vec<String>,
    pub executor_key: Option<String>,
    pub checkpoint_ref: Option<String>,
    pub parent_task_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostTaskMutation {
    pub id: String,
    pub owner: Option<String>,
    pub kind: Option<String>,
    pub source: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub stage: Option<String>,
    pub progress: Option<i64>,
    pub error: Option<String>,
    pub result_refs: Option<Vec<String>>,
    pub executor_key: Option<String>,
    pub checkpoint_ref: Option<String>,
    pub parent_task_id: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HostTaskSnapshot> {
    let refs: String = row.get(13)?;
    Ok(HostTaskSnapshot {
        id: row.get(0)?,
        owner: row.get(1)?,
        kind: row.get(2)?,
        source: row.get(3)?,
        title: row.get(4)?,
        detail: row.get(5)?,
        status: row.get(6)?,
        stage: row.get(7)?,
        progress: row.get(8)?,
        sequence: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        error: row.get(12)?,
        result_refs: serde_json::from_str(&refs).unwrap_or_default(),
        executor_key: row.get(14)?,
        checkpoint_ref: row.get(15)?,
        parent_task_id: row.get(16)?,
    })
}

const COLUMNS: &str = "id,owner,kind,source,title,detail,status,stage,progress,sequence,created_at,updated_at,error,result_refs,executor_key,checkpoint_ref,parent_task_id";

fn get_from_connection(conn: &Connection, id: &str) -> rusqlite::Result<Option<HostTaskSnapshot>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM host_tasks WHERE id = ?1"),
        [id],
        read_row,
    )
    .optional()
}

pub fn list(db: &Db) -> Result<Vec<HostTaskSnapshot>, String> {
    let conn = db.conn().lock().unwrap();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM host_tasks ORDER BY updated_at DESC, sequence DESC LIMIT {MAX_TASKS}"
        ))
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], read_row)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub fn upsert(db: &Db, change: HostTaskMutation) -> Result<HostTaskSnapshot, String> {
    upsert_projection(db, change, None)
}

pub fn upsert_projection(
    db: &Db,
    change: HostTaskMutation,
    revision: Option<u64>,
) -> Result<HostTaskSnapshot, String> {
    validate_text(&change.id, 128, "id")?;
    if change.id.is_empty() {
        return Err("invalid task id".into());
    }
    let mut conn = db.conn().lock().unwrap();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let previous = get_from_connection(&tx, &change.id).map_err(|error| error.to_string())?;
    let authority: Option<(String, i64)> = tx
        .query_row(
            "SELECT owner,core_sequence FROM host_task_executor_revisions WHERE task_id=?1",
            [&change.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match (revision, authority.as_ref()) {
        (None, Some(_)) => return Err("executor-owned task cannot be changed by renderer".into()),
        (Some(sequence), Some((owner, previous_sequence))) => {
            if change.owner.as_deref() != Some(owner.as_str()) {
                return Err("executor owner mismatch".into());
            }
            if sequence <= *previous_sequence as u64 {
                return previous.ok_or_else(|| "stale executor projection removed".into());
            }
        }
        _ => {}
    }
    if revision.is_none()
        && previous
            .as_ref()
            .is_some_and(|task| is_terminal(&task.status))
    {
        return Err("terminal task cannot be changed; retry creates a new task".into());
    }
    let owner = choose(
        change.owner,
        previous.as_ref().map(|task| task.owner.as_str()),
        "host",
    );
    let kind = choose(
        change.kind,
        previous.as_ref().map(|task| task.kind.as_str()),
        "general",
    );
    let source = choose(
        change.source,
        previous.as_ref().map(|task| task.source.as_str()),
        "host",
    );
    let title = choose(
        change.title,
        previous.as_ref().map(|task| task.title.as_str()),
        "任务",
    );
    let detail = choose(
        change.detail,
        previous.as_ref().map(|task| task.detail.as_str()),
        "",
    );
    let status = choose(
        change.status,
        previous.as_ref().map(|task| task.status.as_str()),
        "queued",
    );
    let stage = choose(
        change.stage,
        previous.as_ref().map(|task| task.stage.as_str()),
        "",
    );
    for (value, limit, field) in [
        (&owner, 128, "owner"),
        (&kind, 128, "kind"),
        (&source, 32, "source"),
        (&title, 256, "title"),
        (&detail, 2048, "detail"),
        (&stage, 128, "stage"),
    ] {
        validate_text(value, limit, field)?;
    }
    if !matches!(
        source.as_str(),
        "host" | "plugin" | "marketplace" | "update"
    ) {
        return Err("invalid task source".into());
    }
    if !matches!(
        status.as_str(),
        "queued" | "running" | "paused" | "waiting-user" | "completed" | "failed" | "cancelled"
    ) {
        return Err("invalid task status".into());
    }
    let progress = change
        .progress
        .unwrap_or_else(|| previous.as_ref().map_or(0, |task| task.progress))
        .clamp(0, 100)
        .max(previous.as_ref().map_or(0, |task| task.progress));
    let error = if revision.is_some() {
        change.error
    } else {
        change
            .error
            .or_else(|| previous.as_ref().and_then(|task| task.error.clone()))
    };
    if let Some(error) = &error {
        validate_text(error, 4096, "error")?;
    }
    let refs = change
        .result_refs
        .or_else(|| previous.as_ref().map(|task| task.result_refs.clone()))
        .unwrap_or_default();
    if refs.len() > 32 || refs.iter().any(|reference| reference.len() > 2048) {
        return Err("task result references exceed limit".into());
    }
    let refs = serde_json::to_string(&refs).map_err(|error| error.to_string())?;
    let executor_key = change
        .executor_key
        .or_else(|| previous.as_ref().and_then(|task| task.executor_key.clone()));
    let checkpoint_ref = change.checkpoint_ref.or_else(|| {
        previous
            .as_ref()
            .and_then(|task| task.checkpoint_ref.clone())
    });
    let parent_task_id = change.parent_task_id.or_else(|| {
        previous
            .as_ref()
            .and_then(|task| task.parent_task_id.clone())
    });
    for (value, field) in [
        (&executor_key, "executorKey"),
        (&checkpoint_ref, "checkpointRef"),
        (&parent_task_id, "parentTaskId"),
    ] {
        if let Some(value) = value {
            validate_text(value, 2048, field)?;
        }
    }
    let now = now_ms();
    let created_at = previous.as_ref().map_or(now, |task| task.created_at);
    let sequence = previous.as_ref().map_or(1, |task| task.sequence + 1);
    tx.execute(
        "INSERT INTO host_tasks
         (id,owner,kind,source,title,detail,status,stage,progress,sequence,created_at,updated_at,error,result_refs,executor_key,checkpoint_ref,parent_task_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
         ON CONFLICT(id) DO UPDATE SET
           owner=excluded.owner,kind=excluded.kind,source=excluded.source,title=excluded.title,
           detail=excluded.detail,status=excluded.status,stage=excluded.stage,progress=excluded.progress,
           sequence=excluded.sequence,updated_at=excluded.updated_at,error=excluded.error,
           result_refs=excluded.result_refs,executor_key=excluded.executor_key,
           checkpoint_ref=excluded.checkpoint_ref,parent_task_id=excluded.parent_task_id",
        params![change.id, owner, kind, source, title, detail, status, stage, progress,
            sequence, created_at, now, error, refs, executor_key, checkpoint_ref, parent_task_id],
    )
    .map_err(|error| error.to_string())?;
    if let Some(sequence) = revision {
        let sequence = i64::try_from(sequence).map_err(|_| "executor sequence exceeds limit")?;
        tx.execute("INSERT INTO host_task_executor_revisions(task_id,owner,core_sequence) VALUES (?1,?2,?3) ON CONFLICT(task_id) DO UPDATE SET core_sequence=excluded.core_sequence", params![change.id, owner, sequence]).map_err(|error| error.to_string())?;
    }
    let task = get_from_connection(&tx, &change.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "task disappeared after update".to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(task)
}

pub fn recover(db: &Db) -> Result<usize, String> {
    let mut conn = db.conn().lock().unwrap();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let active: Vec<(String, Option<String>)> = {
        let mut statement = tx
            .prepare("SELECT id, checkpoint_ref FROM host_tasks WHERE status IN ('queued','running','paused','waiting-user') AND NOT EXISTS (SELECT 1 FROM host_task_executor_revisions WHERE task_id=host_tasks.id)")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for (id, checkpoint) in &active {
        let recoverable = checkpoint
            .as_deref()
            .is_some_and(|path| std::path::Path::new(path).is_file());
        let (status, stage, error) = if recoverable {
            ("paused", "recoverable", None)
        } else {
            (
                "failed",
                "interrupted",
                Some("应用重启，任务已中断；请重新执行"),
            )
        };
        tx.execute(
            "UPDATE host_tasks SET status=?1,stage=?2,error=?3,updated_at=?4,sequence=sequence+1 WHERE id=?5",
            params![status, stage, error, now_ms(), id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(active.len())
}

pub fn remove_terminal(db: &Db, id: Option<&str>) -> Result<usize, String> {
    let conn = db.conn().lock().unwrap();
    let sql = if id.is_some() {
        "DELETE FROM host_tasks WHERE id=?1 AND status IN ('completed','failed','cancelled')"
    } else {
        "DELETE FROM host_tasks WHERE ?1 IS NULL AND status IN ('completed','failed','cancelled')"
    };
    conn.execute(sql, params![id])
        .map_err(|error| error.to_string())
}

fn choose(value: Option<String>, previous: Option<&str>, default: &str) -> String {
    value.unwrap_or_else(|| previous.unwrap_or(default).to_string())
}

fn validate_text(value: &str, max: usize, field: &str) -> Result<(), String> {
    if value.len() > max || value.contains('\0') {
        return Err(format!("invalid task {field}"));
    }
    Ok(())
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

pub fn get(db: &Db, id: &str) -> Result<Option<HostTaskSnapshot>, String> {
    let connection = db.conn.lock().map_err(|e| e.to_string())?;
    get_from_connection(&connection, id).map_err(|e| e.to_string())
}

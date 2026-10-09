//! Dedicated experimental task journal, supplied by the composition root.
//! Does not migrate or delete the beta3 host task table or plugin storage.
use cruciblebox_task_core::{Error, Repository, Snapshot, MAX_RETAINED_TASKS, MAX_SNAPSHOT_BYTES};
use rusqlite::{params, Connection, OptionalExtension};
use std::{path::Path, sync::Mutex};
pub struct HistoryPage {
    pub entries: Vec<Snapshot>,
    /// Continue even when this page contains only other owners' records.
    pub next_cursor: Option<i64>,
}
pub struct SqliteRepository {
    connection: Mutex<Connection>,
}
fn storage(error: impl std::fmt::Display) -> Error {
    Error::Storage(error.to_string())
}
impl SqliteRepository {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(storage)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS next_tasks(id TEXT PRIMARY KEY, sequence INTEGER NOT NULL CHECK(sequence>0), snapshot TEXT NOT NULL); CREATE TABLE IF NOT EXISTS next_task_history(id TEXT PRIMARY KEY, sequence INTEGER NOT NULL CHECK(sequence>0), snapshot TEXT NOT NULL);").map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}
impl SqliteRepository {
    pub fn history(&self, owner: &str, after: i64, limit: usize) -> Result<HistoryPage, Error> {
        if owner.is_empty() || owner.len() > 128 || after < 0 || !(1..=100).contains(&limit) {
            return Err(Error::Invalid);
        }
        let c = self.connection.lock().map_err(storage)?;
        let mut stmt=c.prepare(&format!("SELECT rowid,id,sequence,CASE WHEN length(CAST(snapshot AS BLOB))<={} THEN snapshot ELSE NULL END FROM next_task_history WHERE rowid>?1 ORDER BY rowid LIMIT ?2",MAX_SNAPSHOT_BYTES)).map_err(storage)?;
        // Bound fetched rows, including foreign owners, before JSON decoding.
        let rows = stmt
            .query_map(params![after, limit as i64], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(storage)?;
        let mut out = Vec::new();
        let mut scanned = 0;
        let mut last_cursor = None;
        for row in rows {
            let (cursor, id, seq, raw) = row.map_err(storage)?;
            scanned += 1;
            last_cursor = Some(cursor);
            let snapshot: Snapshot = serde_json::from_str(
                &raw.ok_or_else(|| storage("History snapshot budget exceeded"))?,
            )
            .map_err(storage)?;
            snapshot.validate()?;
            if !snapshot.status.terminal() || snapshot.id != id || snapshot.sequence as i64 != seq {
                return Err(storage("Corrupt archived identity/revision/status"));
            }
            if snapshot.owner == owner {
                out.push(snapshot);
            }
        }
        Ok(HistoryPage {
            entries: out,
            next_cursor: if scanned == limit { last_cursor } else { None },
        })
    }
}

impl Repository for SqliteRepository {
    fn list(&self) -> Result<Vec<Snapshot>, Error> {
        let connection = self.connection.lock().map_err(storage)?;
        let count: i64 = connection
            .query_row("SELECT count(*) FROM next_tasks", [], |row| row.get(0))
            .map_err(storage)?;
        if count > MAX_RETAINED_TASKS as i64 {
            return Err(Error::Capacity);
        }
        let mut query = connection
            .prepare(&format!("SELECT id,sequence,CASE WHEN length(CAST(snapshot AS BLOB))<={} THEN snapshot ELSE NULL END FROM next_tasks ORDER BY id", MAX_SNAPSHOT_BYTES))
            .map_err(storage)?;
        let raw = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(storage)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage)?;
        raw.into_iter()
            .map(|(id, sequence, json)| {
                let json =
                    json.ok_or_else(|| Error::Storage("Task snapshot budget exceeded".into()))?;
                let snapshot: Snapshot = serde_json::from_str(&json).map_err(storage)?;
                if snapshot.id != id || i64::try_from(snapshot.sequence).ok() != Some(sequence) {
                    return Err(Error::Storage("Corrupt task identity/revision".into()));
                }
                Ok(snapshot)
            })
            .collect()
    }
    fn save(&self, expected: Option<u64>, next: &Snapshot) -> Result<(), Error> {
        next.validate()?;
        let json = serde_json::to_string(next).map_err(storage)?;
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::Capacity);
        }
        let mut connection = self.connection.lock().map_err(storage)?;
        let transaction = connection.transaction().map_err(storage)?;
        let current: Option<i64> = transaction
            .query_row(
                "SELECT sequence FROM next_tasks WHERE id=?1",
                [&next.id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage)?;
        if current
            != expected
                .map(i64::try_from)
                .transpose()
                .map_err(|_| Error::Invalid)?
        {
            return Err(Error::Stale);
        }
        if expected.is_none() {
            let archived: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM next_task_history WHERE id=?1)",
                    [&next.id],
                    |r| r.get(0),
                )
                .map_err(storage)?;
            if archived {
                return Err(Error::Stale);
            }
        }
        if next.sequence != expected.unwrap_or(0).checked_add(1).ok_or(Error::Invalid)? {
            return Err(Error::Invalid);
        }
        transaction.execute("INSERT INTO next_tasks(id,sequence,snapshot) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET sequence=excluded.sequence,snapshot=excluded.snapshot",params![next.id,i64::try_from(next.sequence).map_err(|_| Error::Invalid)?,json]).map_err(storage)?;
        transaction.commit().map_err(storage)
    }
    fn archive(&self, snapshot: &Snapshot) -> Result<(), Error> {
        snapshot.validate()?;
        if !snapshot.status.terminal() {
            return Err(Error::Invalid);
        }
        let json = serde_json::to_string(snapshot).map_err(storage)?;
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::Capacity);
        }
        let mut connection = self.connection.lock().map_err(storage)?;
        let tx = connection.transaction().map_err(storage)?;
        let changed = tx.execute("INSERT INTO next_task_history(id,sequence,snapshot) SELECT id,sequence,snapshot FROM next_tasks WHERE id=?1 AND sequence=?2 AND snapshot=?3", params![snapshot.id,snapshot.sequence as i64,json]).map_err(storage)?;
        if changed != 1 {
            return Err(Error::Stale);
        }
        let deleted = tx
            .execute(
                "DELETE FROM next_tasks WHERE id=?1 AND sequence=?2",
                params![snapshot.id, snapshot.sequence as i64],
            )
            .map_err(storage)?;
        if deleted != 1 {
            return Err(Error::Stale);
        }
        tx.commit().map_err(storage)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cruciblebox_task_core::{Action, Status};
    #[test]
    fn durable_cas_restart_and_corruption_preservation() {
        let path = std::env::temp_dir().join(format!(
            "cb-next-journal-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let queued =
            Snapshot::queued("a".into(), "diary".into(), "document".into(), "cpu".into()).unwrap();
        {
            let repo = SqliteRepository::open(&path).unwrap();
            repo.save(None, &queued).unwrap();
            let running = queued.transition(Action::Start).unwrap();
            repo.save(Some(1), &running).unwrap();
            assert_eq!(repo.save(Some(1), &running), Err(Error::Stale));
            assert_eq!(repo.list().unwrap(), vec![running]);
        }
        {
            let repo = SqliteRepository::open(&path).unwrap();
            let snapshot = repo.list().unwrap().remove(0);
            let interrupted = snapshot
                .transition(Action::Recover { published: None })
                .unwrap();
            repo.save(Some(2), &interrupted).unwrap();
            assert_eq!(repo.list().unwrap()[0].status, Status::Interrupted);
            repo.connection
                .lock()
                .unwrap()
                .execute(
                    "UPDATE next_tasks SET snapshot='corrupt-preserved' WHERE id='a'",
                    [],
                )
                .unwrap();
            assert!(matches!(repo.list(), Err(Error::Storage(_))));
            let raw: String = repo
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT snapshot FROM next_tasks WHERE id='a'", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(raw, "corrupt-preserved");
        }
        std::fs::remove_file(&path).unwrap();
        for suffix in ["-wal", "-shm"] {
            let file = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
            if file.is_file() {
                std::fs::remove_file(file).unwrap();
            }
        }
    }
    #[test]
    fn actual_output_publish_then_terminal_write_failure_recovers_from_sqlite() {
        use cruciblebox_task_core::{Limits, Observer, Runtime};
        use std::{collections::BTreeMap, io::Write};
        struct FaultRepository(SqliteRepository);
        impl Repository for FaultRepository {
            fn list(&self) -> Result<Vec<Snapshot>, Error> {
                self.0.list()
            }
            fn save(&self, expected: Option<u64>, next: &Snapshot) -> Result<(), Error> {
                if next.status == Status::Succeeded {
                    return Err(Error::Storage("injected terminal write failure".into()));
                }
                self.0.save(expected, next)
            }
        }
        struct Silent;
        impl Observer for Silent {
            fn persisted(&self, _: &Snapshot) {}
        }
        let directory = std::env::temp_dir().join(format!(
            "cb-next-publish-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let database = directory.join("tasks.db");
        let output = directory.join("output.txt");
        let limits = || Limits {
            queued: 2,
            retained: 8,
            resources: BTreeMap::from([("cpu".into(), 1)]),
        };
        {
            let runtime = Runtime::open(
                FaultRepository(SqliteRepository::open(&database).unwrap()),
                Silent,
                limits(),
            )
            .unwrap();
            runtime
                .submit(
                    Snapshot::queued("a".into(), "diary".into(), "document".into(), "cpu".into())
                        .unwrap(),
                )
                .unwrap();
            runtime.apply("diary", "a", 1, Action::Start).unwrap();
            runtime
                .apply(
                    "diary",
                    "a",
                    2,
                    Action::PreparePublication("journal:1".into()),
                )
                .unwrap();
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)
                .unwrap();
            file.write_all("真实输出，终态写入故障".as_bytes()).unwrap();
            file.sync_all().unwrap();
            drop(file);
            assert!(matches!(
                runtime.apply(
                    "diary",
                    "a",
                    3,
                    Action::Published {
                        journal: "journal:1".into(),
                        refs: vec![output.to_string_lossy().into_owned()]
                    }
                ),
                Err(Error::Storage(_))
            ));
            assert_eq!(
                runtime.apply("diary", "a", 3, Action::RequestCancel),
                Err(Error::PublicationPending)
            );
        }
        {
            assert_eq!(
                std::fs::read_to_string(&output).unwrap(),
                "真实输出，终态写入故障"
            );
            let runtime =
                Runtime::open(SqliteRepository::open(&database).unwrap(), Silent, limits())
                    .unwrap();
            assert_eq!(
                runtime.list("diary").unwrap()[0].publication.as_deref(),
                Some("journal:1")
            );
            let completed = runtime
                .apply(
                    "diary",
                    "a",
                    3,
                    Action::Recover {
                        published: Some(vec![output.to_string_lossy().into_owned()]),
                    },
                )
                .unwrap();
            assert_eq!(completed.status, Status::Succeeded);
            assert_eq!(
                runtime.apply("diary", "a", 4, Action::Progress(9)),
                Err(Error::Terminal)
            );
        }
        // Only this test-created directory, after all database/file handles close.
        std::fs::remove_dir_all(&directory).unwrap();
    }
    #[test]
    fn oversized_and_excess_rows_fail_closed_without_loading_or_deleting_them() {
        let repo = SqliteRepository::open(Path::new(":memory:")).unwrap();
        repo.connection
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO next_tasks(id,sequence,snapshot) VALUES('huge',1,?1)",
                ["x".repeat(MAX_SNAPSHOT_BYTES + 1)],
            )
            .unwrap();
        assert_eq!(
            repo.list().unwrap_err(),
            Error::Storage("Task snapshot budget exceeded".into())
        );
        let length: i64 = repo
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT length(snapshot) FROM next_tasks WHERE id='huge'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(length, MAX_SNAPSHOT_BYTES as i64 + 1);
        // Additional synthetic in-memory rows; no user database or task history.
        for i in 0..MAX_RETAINED_TASKS {
            repo.connection
                .lock()
                .unwrap()
                .execute(
                    "INSERT INTO next_tasks(id,sequence,snapshot) VALUES(?1,1,'{}')",
                    [format!("row-{i}")],
                )
                .unwrap();
        }
        assert_eq!(repo.list().unwrap_err(), Error::Capacity);
        let count: i64 = repo
            .connection
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM next_tasks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, MAX_RETAINED_TASKS as i64 + 1);
    }
    fn terminal(repo: &SqliteRepository, id: &str, owner: &str) -> Snapshot {
        let q = Snapshot::queued(id.into(), owner.into(), "document".into(), "cpu".into()).unwrap();
        repo.save(None, &q).unwrap();
        let r = q.transition(Action::Start).unwrap();
        repo.save(Some(1), &r).unwrap();
        let t = r
            .transition(Action::Succeed(vec![format!("output/{id}")]))
            .unwrap();
        repo.save(Some(2), &t).unwrap();
        t
    }
    fn archive_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "cb-task-archive-{label}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    struct NoEvents;
    impl cruciblebox_task_core::Observer for NoEvents {
        fn persisted(&self, _: &Snapshot) {}
    }
    #[test]
    fn bounded_runtime_archives_terminal_history_and_refuses_reusing_archived_id() {
        use cruciblebox_task_core::{Limits, Runtime};
        use std::collections::BTreeMap;
        let path = archive_path("runtime");
        let rt = Runtime::open(
            SqliteRepository::open(&path).unwrap(),
            NoEvents,
            Limits {
                queued: 2,
                retained: 2,
                resources: BTreeMap::from([("cpu".into(), 1)]),
            },
        )
        .unwrap();
        for id in ["a", "b"] {
            let q = Snapshot::queued(id.into(), "diary".into(), "document".into(), "cpu".into())
                .unwrap();
            rt.submit(q).unwrap();
            rt.apply("diary", id, 1, Action::Start).unwrap();
            rt.apply(
                "diary",
                id,
                2,
                Action::Succeed(vec![format!("output/{id}")]),
            )
            .unwrap();
        }
        rt.submit(
            Snapshot::queued("c".into(), "diary".into(), "document".into(), "cpu".into()).unwrap(),
        )
        .unwrap();
        assert_eq!(rt.list("diary").unwrap().len(), 2);
        drop(rt);
        let repo = SqliteRepository::open(&path).unwrap();
        let page = repo.history("diary", 0, 100).unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].id, "a");
        assert_eq!(page.entries[0].result_refs, vec!["output/a"]);
        assert_eq!(
            repo.save(
                None,
                &Snapshot::queued("a".into(), "diary".into(), "document".into(), "cpu".into())
                    .unwrap()
            ),
            Err(Error::Stale)
        );
        assert_eq!(repo.list().unwrap().len(), 2);
    }
    #[test]
    fn archive_delete_fault_rolls_back_history_and_preserves_exact_hot_record() {
        let repo = SqliteRepository::open(&archive_path("fault")).unwrap();
        let t = terminal(&repo, "a", "diary");
        repo.connection.lock().unwrap().execute_batch("CREATE TRIGGER inject_archive_failure BEFORE DELETE ON next_tasks BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(matches!(repo.archive(&t), Err(Error::Storage(_))));
        assert_eq!(repo.list().unwrap(), vec![t.clone()]);
        assert!(repo.history("diary", 0, 100).unwrap().entries.is_empty());
        repo.connection
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER inject_archive_failure")
            .unwrap();
        repo.archive(&t).unwrap();
        assert!(repo.list().unwrap().is_empty());
        assert_eq!(repo.history("diary", 0, 100).unwrap().entries, vec![t]);
    }
    #[test]
    fn bounded_history_pagination_does_not_expose_other_owner_and_can_continue_empty_page() {
        let repo = SqliteRepository::open(&archive_path("pagination")).unwrap();
        for (id, owner) in [("a", "other"), ("b", "diary")] {
            let t = terminal(&repo, id, owner);
            repo.archive(&t).unwrap();
        }
        let first = repo.history("diary", 0, 1).unwrap();
        assert!(first.entries.is_empty());
        let next = first.next_cursor.unwrap();
        let second = repo.history("diary", next, 1).unwrap();
        assert_eq!(second.entries[0].id, "b");
        assert_eq!(second.entries[0].owner, "diary");
        assert!(repo
            .history("diary", second.next_cursor.unwrap(), 1)
            .unwrap()
            .entries
            .is_empty());
        assert!(matches!(repo.history("diary", 0, 101), Err(Error::Invalid)));
    }
    #[test]
    fn running_and_stale_snapshots_cannot_be_archived() {
        let repo = SqliteRepository::open(&archive_path("active")).unwrap();
        let q =
            Snapshot::queued("a".into(), "diary".into(), "document".into(), "cpu".into()).unwrap();
        repo.save(None, &q).unwrap();
        assert_eq!(repo.archive(&q), Err(Error::Invalid));
        let r = q.transition(Action::Start).unwrap();
        repo.save(Some(1), &r).unwrap();
        let t = r.transition(Action::Succeed(vec![])).unwrap();
        assert_eq!(repo.archive(&t), Err(Error::Stale));
        assert_eq!(repo.list().unwrap(), vec![r]);
    }
}

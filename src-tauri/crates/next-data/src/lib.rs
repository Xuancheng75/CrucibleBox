//! Experimental, explicit once-only namespace copy. Never invoked at startup or by plugin RPC.
//! Reviewed plans copy old values into new keys and preserve all original rows. Files and
//! paired program/database restoration belong to the outer coordinator, not this SQL transaction.
pub mod blob;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub source_key: String,
    pub expected_source: String,
    pub target_key: String,
    pub target_value: Value,
}
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub owner: String,
    pub id: String,
    pub entries: Vec<Entry>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    OwnerMissing,
    SourceChanged,
    TargetConflict,
    PlanChanged,
    Unavailable,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied { copied: usize },
    AlreadyApplied,
}
fn key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}
fn sql<T>(result: rusqlite::Result<T>) -> Result<T, Error> {
    result.map_err(|_| Error::Unavailable)
}
impl Plan {
    fn fingerprint(&self) -> Result<String, Error> {
        if self.owner.len() < 2
            || self.owner.len() > 64
            || !self.owner.as_bytes()[0].is_ascii_lowercase()
            || !self
                .owner
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !key(&self.id)
            || self.id.contains(':')
            || self.entries.len() > 1024
        {
            return Err(Error::Invalid);
        }
        let mut bytes_used: usize = 0;
        let mut targets = BTreeSet::new();
        let sources: BTreeSet<_> = self.entries.iter().map(|e| &e.source_key).collect();
        for e in &self.entries {
            if e.source_key.is_empty()
                || e.source_key.len() > 4096
                || e.expected_source.len() > 16 * 1024 * 1024
                || !key(&e.target_key)
                || sources.contains(&e.target_key)
                || !targets.insert(&e.target_key)
            {
                return Err(Error::Invalid);
            }
            let value = serde_json::to_string(&e.target_value).map_err(|_| Error::Invalid)?;
            cruciblebox_next_protocol::validate_storage_value(&value)
                .map_err(|_| Error::Invalid)?;
            bytes_used = bytes_used
                .saturating_add(e.expected_source.len())
                .saturating_add(value.len())
                .saturating_add(e.source_key.len())
                .saturating_add(e.target_key.len());
            if bytes_used > 8 * 1024 * 1024 {
                return Err(Error::Invalid);
            }
        }
        let bytes = serde_json::to_vec(self).map_err(|_| Error::Invalid)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(Error::Invalid);
        }
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}
/// An outer coordinator must quiesce this owner and provide a coherent copy/backup first.
/// Applying the same plan twice is a no-op, even after the user edits the migrated target.
/// A changed plan under the same ID is refused. There is no destructive reverse migration.
pub fn apply(connection: &mut Connection, plan: &Plan) -> Result<Outcome, Error> {
    let fingerprint = plan.fingerprint()?;
    let prefix = format!("next:data:{}:", plan.id);
    let marker = format!("{prefix}{fingerprint}");
    let transaction = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
    let exists: bool = sql(transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM plugins WHERE id=?1)",
        [&plan.owner],
        |r| r.get(0),
    ))?;
    if !exists {
        return Err(Error::OwnerMissing);
    }
    let applied: Vec<String> = {
        let mut stmt = sql(transaction.prepare("SELECT migration FROM plugin_storage_migrations WHERE plugin_id=?1 AND substr(migration,1,?2)=?3"))?;
        let rows = sql(
            stmt.query_map(params![plan.owner, prefix.len() as i64, prefix], |r| {
                r.get(0)
            }),
        )?;
        sql(rows.collect())?
    };
    if !applied.is_empty() {
        return if applied == [marker] {
            Ok(Outcome::AlreadyApplied)
        } else {
            Err(Error::PlanChanged)
        };
    }
    let mut copied = 0;
    for entry in &plan.entries {
        let source: Option<String> = sql(transaction
            .query_row(
                "SELECT value FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
                params![plan.owner, entry.source_key],
                |r| r.get(0),
            )
            .optional())?;
        if source.as_deref() != Some(entry.expected_source.as_str()) {
            return Err(Error::SourceChanged);
        }
        let target: Option<String> = sql(transaction
            .query_row(
                "SELECT value FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
                params![plan.owner, entry.target_key],
                |r| r.get(0),
            )
            .optional())?;
        let value = serde_json::to_string(&entry.target_value).map_err(|_| Error::Invalid)?;
        match target {
            Some(existing) if existing != value => return Err(Error::TargetConflict),
            Some(_) => (),
            None => {
                sql(transaction.execute(
                    "INSERT INTO plugin_storage(plugin_id,key,value) VALUES(?1,?2,?3)",
                    params![plan.owner, entry.target_key, value],
                ))?;
                copied += 1;
            }
        }
    }
    sql(transaction.execute(
        "INSERT INTO plugin_storage_migrations(plugin_id,migration) VALUES(?1,?2)",
        params![plan.owner, marker],
    ))?;
    sql(transaction.commit())?;
    Ok(Outcome::Applied { copied })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE plugins(id TEXT PRIMARY KEY); CREATE TABLE plugin_storage(plugin_id TEXT NOT NULL REFERENCES plugins(id),key TEXT,value TEXT,PRIMARY KEY(plugin_id,key)); CREATE TABLE plugin_storage_migrations(plugin_id TEXT REFERENCES plugins(id),migration TEXT,PRIMARY KEY(plugin_id,migration)); INSERT INTO plugins VALUES('diary'),('other'); INSERT INTO plugin_storage VALUES('diary','old','non-json-original'),('other','old','other-original');").unwrap();
        c
    }
    fn plan() -> Plan {
        Plan {
            owner: "diary".into(),
            id: "diary-v1".into(),
            entries: vec![Entry {
                source_key: "old".into(),
                expected_source: "non-json-original".into(),
                target_key: "next.notes.v1".into(),
                target_value: serde_json::json!({"text":"中文"}),
            }],
        }
    }
    fn value(c: &Connection, owner: &str, key: &str) -> Option<String> {
        c.query_row(
            "SELECT value FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
            params![owner, key],
            |r| r.get(0),
        )
        .optional()
        .unwrap()
    }
    #[test]
    fn copy_preserves_original_namespace_and_repeated_plan_never_overwrites_user_edit() {
        let mut c = fixture();
        let p = plan();
        assert_eq!(apply(&mut c, &p), Ok(Outcome::Applied { copied: 1 }));
        assert_eq!(
            value(&c, "diary", "old").as_deref(),
            Some("non-json-original")
        );
        assert_eq!(value(&c, "other", "old").as_deref(), Some("other-original"));
        assert_eq!(value(&c, "other", "next.notes.v1"), None);
        c.execute(
            "UPDATE plugin_storage SET value='user-edit' WHERE key='next.notes.v1'",
            [],
        )
        .unwrap();
        assert_eq!(apply(&mut c, &p), Ok(Outcome::AlreadyApplied));
        assert_eq!(
            value(&c, "diary", "next.notes.v1").as_deref(),
            Some("user-edit")
        );
        let mut changed = p;
        changed.entries[0].target_value = Value::Null;
        assert_eq!(apply(&mut c, &changed), Err(Error::PlanChanged));
    }
    #[test]
    fn source_and_target_conflicts_roll_back_all_writes_and_marker() {
        for conflict in ["source", "target", "trigger"] {
            let mut c = fixture();
            let mut p = plan();
            p.entries.push(Entry {
                source_key: "old".into(),
                expected_source: if conflict == "source" {
                    "wrong"
                } else {
                    "non-json-original"
                }
                .into(),
                target_key: "next.second".into(),
                target_value: Value::Null,
            });
            if conflict == "target" {
                c.execute(
                    "INSERT INTO plugin_storage VALUES('diary','next.second','keep')",
                    [],
                )
                .unwrap();
            }
            if conflict == "trigger" {
                c.execute_batch("CREATE TRIGGER inject BEFORE INSERT ON plugin_storage WHEN NEW.key='next.second' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
            }
            assert_eq!(
                apply(&mut c, &p),
                Err(match conflict {
                    "source" => Error::SourceChanged,
                    "target" => Error::TargetConflict,
                    _ => Error::Unavailable,
                })
            );
            assert_eq!(value(&c, "diary", "next.notes.v1"), None);
            assert_eq!(
                c.query_row("SELECT count(*) FROM plugin_storage_migrations", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
                0
            );
        }
    }
    #[test]
    fn malformed_budget_and_destructive_targets_fail_before_sql() {
        let mut c = fixture();
        let mut p = plan();
        p.owner = "missing".into();
        assert_eq!(apply(&mut c, &p), Err(Error::OwnerMissing));
        {
            let mut p = plan();
            p.entries[0].target_key = "old".into();
            assert_eq!(apply(&mut c, &p), Err(Error::Invalid));
            p.entries[0].target_key = "new".into();
            p.entries[0].target_value =
                Value::String("x".repeat(cruciblebox_next_protocol::MAX_STORAGE_VALUE_BYTES));
            assert_eq!(apply(&mut c, &p), Err(Error::Invalid));
        }
    }
    #[test]
    fn migration_marker_survives_real_database_restart() {
        let mut c = fixture();
        let p = plan();
        assert_eq!(apply(&mut c, &p), Ok(Outcome::Applied { copied: 1 }));
        let path = std::env::temp_dir().join(format!(
            "cb-next-migration-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        drop(c);
        let mut reopened = Connection::open(&path).unwrap();
        assert_eq!(apply(&mut reopened, &p), Ok(Outcome::AlreadyApplied));
        assert_eq!(
            value(&reopened, "diary", "old").as_deref(),
            Some("non-json-original")
        );
        assert_eq!(
            reopened
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
}

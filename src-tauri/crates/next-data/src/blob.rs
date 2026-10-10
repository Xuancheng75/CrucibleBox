//! Host-reviewed large JSON copy. No plugin RPC or automatic startup entry point.
//! Chunks are immutable content-addressed rows; old source and previous versions survive.
use crate::{sql, Error, Outcome};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
const CHUNK: usize = 24 * 1024;
const MAX: usize = 4 * 1024 * 1024;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn decode(value: &str) -> Result<Vec<u8>, Error> {
    if value.len() > CHUNK * 2
        || !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid);
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|_| Error::Invalid))
        .collect()
}
/// A caller must first quiesce the owner and snapshot the paired old program/database.
/// The returned index key is the explicit migration mapping, never the old source key.
pub fn copy_json(
    c: &mut Connection,
    owner: &str,
    id: &str,
    source_key: &str,
    expected_source: &str,
) -> Result<(Outcome, String), Error> {
    if owner.len() < 2
        || owner.len() > 64
        || !owner.as_bytes()[0].is_ascii_lowercase()
        || !owner
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || !crate::key(id)
        || id.contains(':')
        || source_key.is_empty()
        || source_key.len() > 4096
        || expected_source.len() > MAX
    {
        return Err(Error::Invalid);
    }
    // Preserve the exact source representation, including JSON whitespace and Unicode.
    serde_json::from_str::<Value>(expected_source).map_err(|_| Error::Invalid)?;
    let digest = hash(expected_source.as_bytes());
    let base = format!("next.blob.{}", hash(source_key.as_bytes()));
    let index_key = format!("{base}.index");
    let chunk_prefix = format!("next.chunk.{digest}");
    let count = expected_source.len().div_ceil(CHUNK);
    let index = json!({"format":"hex-json-v1","bytes":expected_source.len(),"sha256":digest,"chunks":count,"prefix":chunk_prefix});
    let marker = format!(
        "next:blob:{id}:{}",
        hash(format!("{owner}\0{source_key}\0{expected_source}").as_bytes())
    );
    let marker_prefix = format!("next:blob:{id}:");
    let tx = sql(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    let exists: bool = sql(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM plugins WHERE id=?1)",
        [owner],
        |r| r.get(0),
    ))?;
    if !exists {
        return Err(Error::OwnerMissing);
    }
    let old_marker:Option<String>=sql(tx.query_row("SELECT migration FROM plugin_storage_migrations WHERE plugin_id=?1 AND substr(migration,1,?2)=?3",params![owner,marker_prefix.len() as i64,marker_prefix],|r|r.get(0)).optional())?;
    if let Some(old) = old_marker {
        return if old == marker {
            Ok((Outcome::AlreadyApplied, index_key))
        } else {
            Err(Error::PlanChanged)
        };
    }
    let source: Option<String> = sql(tx
        .query_row(
            "SELECT value FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
            params![owner, source_key],
            |r| r.get(0),
        )
        .optional())?;
    if source.as_deref() != Some(expected_source) {
        return Err(Error::SourceChanged);
    }
    let mut copied = 0;
    for (key, value) in expected_source
        .as_bytes()
        .chunks(CHUNK)
        .enumerate()
        .map(|(i, b)| (format!("{chunk_prefix}.{i}"), Value::String(hex(b))))
        .chain(std::iter::once((index_key.clone(), index)))
    {
        let raw = serde_json::to_string(&value).map_err(|_| Error::Invalid)?;
        cruciblebox_next_protocol::validate_payload(&raw).map_err(|_| Error::Invalid)?;
        let existing: Option<String> = sql(tx
            .query_row(
                "SELECT value FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
                params![owner, key],
                |r| r.get(0),
            )
            .optional())?;
        match existing {
            Some(old) if old != raw => return Err(Error::TargetConflict),
            Some(_) => {}
            None => {
                sql(tx.execute(
                    "INSERT INTO plugin_storage(plugin_id,key,value) VALUES(?1,?2,?3)",
                    params![owner, key, raw],
                ))?;
                copied += 1;
            }
        }
    }
    sql(tx.execute(
        "INSERT INTO plugin_storage_migrations(plugin_id,migration) VALUES(?1,?2)",
        params![owner, marker],
    ))?;
    sql(tx.commit())?;
    Ok((Outcome::Applied { copied }, index_key))
}
/// Validate a complete persisted copy inside one consistent read transaction.
/// Missing, mixed, malformed or corrupt chunks fail without modifying any row.
pub fn read_json(c: &mut Connection, owner: &str, index_key: &str) -> Result<String, Error> {
    if !crate::key(index_key) {
        return Err(Error::Invalid);
    }
    let tx = sql(c.transaction())?;
    let raw:String=sql(tx.query_row("SELECT value FROM plugin_storage WHERE plugin_id=?1 AND key=?2 AND length(CAST(value AS BLOB))<=65536",params![owner,index_key],|r|r.get(0)))?;
    let v: Value = serde_json::from_str(&raw).map_err(|_| Error::Invalid)?;
    let obj = v.as_object().ok_or(Error::Invalid)?;
    if obj.len() != 5 || v["format"] != "hex-json-v1" {
        return Err(Error::Invalid);
    }
    let bytes = v["bytes"].as_u64().ok_or(Error::Invalid)? as usize;
    let chunks = v["chunks"].as_u64().ok_or(Error::Invalid)? as usize;
    let digest = v["sha256"].as_str().ok_or(Error::Invalid)?;
    if bytes > MAX
        || chunks != bytes.div_ceil(CHUNK)
        || digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid);
    }
    let prefix = format!("next.chunk.{digest}");
    if v["prefix"] != prefix {
        return Err(Error::Invalid);
    }
    let mut data = Vec::with_capacity(bytes);
    for i in 0..chunks {
        let raw:String=sql(tx.query_row("SELECT value FROM plugin_storage WHERE plugin_id=?1 AND key=?2 AND length(CAST(value AS BLOB))<=65536",params![owner,format!("{prefix}.{i}")],|r|r.get(0)))?;
        let chunk: Value = serde_json::from_str(&raw).map_err(|_| Error::Invalid)?;
        let decoded = decode(chunk.as_str().ok_or(Error::Invalid)?)?;
        if decoded.len()
            != if i + 1 == chunks {
                bytes - i * CHUNK
            } else {
                CHUNK
            }
        {
            return Err(Error::Invalid);
        }
        data.extend(decoded);
    }
    if data.len() != bytes || hash(&data) != digest {
        return Err(Error::Invalid);
    }
    let result = String::from_utf8(data).map_err(|_| Error::Invalid)?;
    serde_json::from_str::<Value>(&result).map_err(|_| Error::Invalid)?;
    sql(tx.commit())?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE plugins(id TEXT PRIMARY KEY); CREATE TABLE plugin_storage(plugin_id TEXT,key TEXT,value TEXT,PRIMARY KEY(plugin_id,key)); CREATE TABLE plugin_storage_migrations(plugin_id TEXT,migration TEXT,PRIMARY KEY(plugin_id,migration)); INSERT INTO plugins VALUES('diary'),('other');").unwrap();
        c
    }
    #[test]
    fn large_unicode_exact_copy_keeps_source_and_repeated_marker_keeps_user_edits() {
        let mut c = fixture();
        let raw = format!(
            " {{\"content\":{}}} ",
            serde_json::to_string(&"中文🦀\\\"".repeat(70_000)).unwrap()
        );
        c.execute(
            "INSERT INTO plugin_storage VALUES('diary','entry:2026-10-02',?1)",
            [&raw],
        )
        .unwrap();
        let (out, index) =
            copy_json(&mut c, "diary", "diary-v1", "entry:2026-10-02", &raw).unwrap();
        assert!(matches!(out,Outcome::Applied{copied} if copied>2));
        assert_eq!(read_json(&mut c, "diary", &index).unwrap(), raw);
        let old: String = c
            .query_row(
                "SELECT value FROM plugin_storage WHERE key='entry:2026-10-02'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, raw);
        c.execute(
            "UPDATE plugin_storage SET value='user-edited' WHERE key=?1",
            [&index],
        )
        .unwrap();
        assert_eq!(
            copy_json(&mut c, "diary", "diary-v1", "entry:2026-10-02", &raw)
                .unwrap()
                .0,
            Outcome::AlreadyApplied
        );
        assert!(read_json(&mut c, "other", &index).is_err());
    }
    #[test]
    fn conflict_and_sql_fault_roll_back_all_chunks_and_marker() {
        let mut c = fixture();
        let raw = serde_json::to_string(&"a".repeat(70_000)).unwrap();
        c.execute(
            "INSERT INTO plugin_storage VALUES('diary','items',?1)",
            [&raw],
        )
        .unwrap();
        c.execute_batch("CREATE TRIGGER fail_index BEFORE INSERT ON plugin_storage WHEN NEW.key LIKE 'next.blob.%' BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
        assert_eq!(
            copy_json(&mut c, "diary", "items-v1", "items", &raw),
            Err(Error::Unavailable)
        );
        let rows: i64 = c
            .query_row("SELECT count(*) FROM plugin_storage", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        let marks: i64 = c
            .query_row("SELECT count(*) FROM plugin_storage_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(marks, 0);
        c.execute_batch("DROP TRIGGER fail_index").unwrap();
        let (_, index) = copy_json(&mut c, "diary", "items-v1", "items", &raw).unwrap();
        c.execute(
            "UPDATE plugin_storage SET value='\"00\"' WHERE key LIKE 'next.chunk.%'",
            [],
        )
        .unwrap();
        assert!(read_json(&mut c, "diary", &index).is_err());
        assert_eq!(
            copy_json(&mut c, "diary", "items-v1", "items", "{}"),
            Err(Error::PlanChanged)
        );
    }
    #[test]
    fn existing_index_conflict_and_changed_source_preserve_original_rows() {
        let mut c = fixture();
        let raw = "{\"content\":\"old\"}";
        c.execute(
            "INSERT INTO plugin_storage VALUES('diary','notes',?1)",
            [raw],
        )
        .unwrap();
        let index = format!("next.blob.{}.index", hash(b"notes"));
        c.execute(
            "INSERT INTO plugin_storage VALUES('diary',?1,'user-existing')",
            [&index],
        )
        .unwrap();
        assert_eq!(
            copy_json(&mut c, "diary", "notes-v1", "notes", raw),
            Err(Error::TargetConflict)
        );
        let count: i64 = c
            .query_row("SELECT count(*) FROM plugin_storage", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            copy_json(&mut c, "diary", "notes-v2", "notes", "{}"),
            Err(Error::SourceChanged)
        );
        let old: String = c
            .query_row(
                "SELECT value FROM plugin_storage WHERE key='notes'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, raw);
    }
    #[test]
    fn large_copy_survives_database_reopen_and_detects_missing_chunk() {
        let mut c = fixture();
        let raw = serde_json::to_string(&"🦀中文".repeat(60000)).unwrap();
        c.execute(
            "INSERT INTO plugin_storage VALUES('diary','draft:2026-10-02',?1)",
            [&raw],
        )
        .unwrap();
        let (_, index) = copy_json(&mut c, "diary", "draft-v1", "draft:2026-10-02", &raw).unwrap();
        let path = std::env::temp_dir().join(format!(
            "cb-next-blob-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        drop(c);
        let mut c = Connection::open(path).unwrap();
        assert_eq!(read_json(&mut c, "diary", &index).unwrap(), raw);
        assert_eq!(
            copy_json(&mut c, "diary", "draft-v1", "draft:2026-10-02", &raw)
                .unwrap()
                .0,
            Outcome::AlreadyApplied
        );
        c.execute(
            "DELETE FROM plugin_storage WHERE key=?1",
            [format!("next.chunk.{}.0", hash(raw.as_bytes()))],
        )
        .unwrap();
        assert!(read_json(&mut c, "diary", &index).is_err());
        let original: String = c
            .query_row(
                "SELECT value FROM plugin_storage WHERE key='draft:2026-10-02'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(original, raw);
        let integrity: String = c
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
    }
    #[test]
    fn malformed_and_oversized_source_refused_without_write() {
        let mut c = fixture();
        assert_eq!(
            copy_json(&mut c, "diary", "v1", "entry", "invalid"),
            Err(Error::Invalid)
        );
        assert_eq!(
            copy_json(&mut c, "diary", "v1", "entry", &"x".repeat(MAX + 1)),
            Err(Error::Invalid)
        );
    }
}

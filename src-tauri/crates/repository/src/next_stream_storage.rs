//! Bounded staging. Logical keys stay in plugin_storage for paired rollback.
use crate::Db;
use base64::{engine::general_purpose::STANDARD, Engine};
use cruciblebox_next_protocol::{
    validate_storage_value, MAX_STORAGE_CHUNK_BYTES as CHUNK,
    MAX_STORAGE_TRANSACTION_BYTES as TOTAL, MAX_STORAGE_TRANSACTION_OPS as OPS,
    MAX_STORAGE_VALUE_BYTES as VALUE,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

type Result<T> = std::result::Result<T, String>;
fn sql<T>(value: rusqlite::Result<T>) -> Result<T> {
    value.map_err(|_| "STORAGE_UNAVAILABLE".into())
}
fn hash(session: &str) -> String {
    format!("{:x}", Sha256::digest(session.as_bytes()))
}
fn now() -> Result<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "STORAGE_UNAVAILABLE")?
        .as_secs() as i64)
}
fn cleanup(conn: &Connection, time: i64) -> Result<()> {
    sql(conn.execute(
        "DELETE FROM next_storage_write_transactions WHERE expires_at<=?1",
        [time],
    ))?;
    sql(conn.execute(
        "DELETE FROM next_storage_read_sessions WHERE expires_at<=?1",
        [time],
    ))?;
    Ok(())
}
fn check(
    conn: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
    owner: &str,
    session: &str,
    allow_missing: bool,
) -> Result<bool> {
    // Table and column names are private constants at every call site.
    let query =
        format!("SELECT plugin_id,session_hash,expires_at FROM {table} WHERE {id_column}=?1");
    let row: Option<(String, String, i64)> = sql(conn
        .query_row(&query, [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional())?;
    match row {
        Some((plugin, scope, expiry))
            if plugin == owner && scope == hash(session) && expiry > now()? =>
        {
            Ok(true)
        }
        None if allow_missing => Ok(false),
        _ => Err("SESSION_DENIED".into()),
    }
}
fn capacity(
    conn: &Connection,
    table: &str,
    owner: &str,
    scope: &str,
    per_session: i64,
) -> Result<()> {
    let (all, own): (i64, i64) = sql(conn.query_row(
        &format!("SELECT count(*),coalesce(sum(plugin_id=?1 AND session_hash=?2),0) FROM {table}"),
        params![owner, scope],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ))?;
    if all >= 32 || own >= per_session {
        return Err("BUSY".into());
    }
    Ok(())
}
pub fn keys(db: &Db, owner: &str, prefix: &str, limit: u32, after: Option<&str>) -> Result<Value> {
    let mut arguments = json!({"prefix":prefix,"limit":limit});
    if let Some(after) = after {
        arguments["after"] = json!(after);
    }
    super::next_storage::validate_params("storage.keys", arguments)?;
    let conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    let mut stmt=sql(conn.prepare("SELECT key FROM plugin_storage WHERE plugin_id=?1 AND substr(key,1,length(?2))=?2 AND key>?3 COLLATE BINARY ORDER BY key COLLATE BINARY LIMIT ?4"))?;
    let rows = sql(stmt.query_map(
        params![owner, prefix, after.unwrap_or(""), limit + 1],
        |r| r.get::<_, String>(0),
    ))?;
    let mut items = sql(rows.collect::<rusqlite::Result<Vec<_>>>())?;
    for key in &items {
        super::next_storage::validate_params("storage.get", json!({"key":key}))
            .map_err(|_| "STORAGE_CORRUPT")?;
    }
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let cursor = if more { items.last().cloned() } else { None };
    Ok(json!({"items":items,"nextCursor":cursor}))
}
pub fn read_begin(db: &Db, owner: &str, session: &str, key: &str) -> Result<Value> {
    super::next_storage::validate_params("storage.read.begin", json!({"key":key}))?;
    let mut conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    let tx = sql(conn.transaction())?;
    cleanup(&tx, now()?)?;
    capacity(&tx, "next_storage_read_sessions", owner, &hash(session), 4)?;
    let bytes: Option<Option<Vec<u8>>> = sql(tx.query_row(
        "SELECT CASE WHEN length(CAST(value AS BLOB))<=?3 THEN CAST(value AS BLOB) ELSE NULL END FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
        params![owner,key,VALUE as i64],|r|r.get(0)).optional())?;
    let Some(bytes) = bytes else {
        sql(tx.commit())?;
        return Ok(json!({"found":false,"readId":null,"byteLength":0}));
    };
    let bytes = bytes.ok_or("BUDGET_EXCEEDED")?;
    validate_storage_value(std::str::from_utf8(&bytes).map_err(|_| "STORAGE_CORRUPT")?)
        .map_err(|_| "STORAGE_CORRUPT")?;
    let id = token().map_err(|_| "STORAGE_UNAVAILABLE")?;
    sql(tx.execute("INSERT INTO next_storage_read_sessions(read_id,plugin_id,session_hash,key,raw_value,expires_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,owner,hash(session),key,bytes,now()?+1800]))?;
    sql(tx.commit())?;
    Ok(json!({"found":true,"readId":id,"byteLength":bytes.len()}))
}
pub fn read_chunk(db: &Db, owner: &str, session: &str, id: &str, offset: u64) -> Result<Value> {
    let conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    check(
        &conn,
        "next_storage_read_sessions",
        "read_id",
        id,
        owner,
        session,
        false,
    )?;
    let length: i64 = sql(conn.query_row(
        "SELECT length(raw_value) FROM next_storage_read_sessions WHERE read_id=?1",
        [id],
        |r| r.get(0),
    ))?;
    if offset >= length as u64 || !offset.is_multiple_of(CHUNK as u64) {
        return Err("INVALID_REQUEST".into());
    }
    let bytes: Vec<u8> = sql(conn.query_row(
        "SELECT substr(raw_value,?2,?3) FROM next_storage_read_sessions WHERE read_id=?1",
        params![id, (offset + 1) as i64, CHUNK as i64],
        |r| r.get(0),
    ))?;
    Ok(json!({"offset":offset,"data":STANDARD.encode(bytes)}))
}
pub fn read_close(db: &Db, owner: &str, session: &str, id: &str) -> Result<()> {
    let conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    if check(
        &conn,
        "next_storage_read_sessions",
        "read_id",
        id,
        owner,
        session,
        true,
    )? {
        sql(conn.execute(
            "DELETE FROM next_storage_read_sessions WHERE read_id=?1",
            [id],
        ))?;
    }
    Ok(())
}
pub fn write_begin(
    db: &Db,
    owner: &str,
    session: &str,
    writes: &[Value],
    deletes: &[Value],
) -> Result<Value> {
    super::next_storage::validate_params(
        "storage.write.begin",
        json!({"writes":writes,"deletes":deletes}),
    )?;
    if writes.len() + deletes.len() == 0 || writes.len() + deletes.len() > OPS {
        return Err("INVALID_REQUEST".into());
    }
    let mut keys = HashSet::new();
    let mut total = 0u64;
    for spec in writes {
        if !keys.insert(spec["key"].as_str().ok_or("INVALID_REQUEST")?) {
            return Err("INVALID_REQUEST".into());
        }
        total += spec["byteLength"].as_u64().ok_or("INVALID_REQUEST")?;
    }
    for key in deletes {
        if !keys.insert(key.as_str().ok_or("INVALID_REQUEST")?) {
            return Err("INVALID_REQUEST".into());
        }
    }
    if total > TOTAL as u64 {
        return Err("BUDGET_EXCEEDED".into());
    }
    let mut conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    let tx = sql(conn.transaction())?;
    cleanup(&tx, now()?)?;
    capacity(
        &tx,
        "next_storage_write_transactions",
        owner,
        &hash(session),
        1,
    )?;
    let id = token().map_err(|_| "STORAGE_UNAVAILABLE")?;
    sql(tx.execute("INSERT INTO next_storage_write_transactions(transaction_id,plugin_id,session_hash,writes_json,deletes_json,total_bytes,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![id,owner,hash(session),serde_json::to_string(writes).map_err(|_|"INVALID_REQUEST")?,serde_json::to_string(deletes).map_err(|_|"INVALID_REQUEST")?,total as i64,now()?+1800]))?;
    sql(tx.commit())?;
    Ok(json!({"transactionId":id}))
}
pub fn write_chunk(
    db: &Db,
    owner: &str,
    session: &str,
    id: &str,
    key: &str,
    offset: u64,
    data: &str,
) -> Result<Value> {
    if data.len() > CHUNK.div_ceil(3) * 4 {
        return Err("BUDGET_EXCEEDED".into());
    }
    let bytes = STANDARD.decode(data).map_err(|_| "INVALID_REQUEST")?;
    if bytes.is_empty() || bytes.len() > CHUNK || STANDARD.encode(&bytes) != data {
        return Err("INVALID_REQUEST".into());
    }
    let mut conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    let tx = sql(conn.transaction())?;
    check(
        &tx,
        "next_storage_write_transactions",
        "transaction_id",
        id,
        owner,
        session,
        false,
    )?;
    let raw: String = sql(tx.query_row(
        "SELECT writes_json FROM next_storage_write_transactions WHERE transaction_id=?1",
        [id],
        |r| r.get(0),
    ))?;
    let writes: Vec<Value> = serde_json::from_str(&raw).map_err(|_| "STORAGE_CORRUPT")?;
    let expected = writes
        .iter()
        .find(|s| s["key"].as_str() == Some(key))
        .and_then(|s| s["byteLength"].as_u64())
        .ok_or("INVALID_REQUEST")?;
    let received:i64=sql(tx.query_row("SELECT coalesce(sum(length(bytes)),0) FROM next_storage_write_chunks WHERE transaction_id=?1 AND key=?2",params![id,key],|r|r.get(0)))?;
    let end = offset
        .checked_add(bytes.len() as u64)
        .ok_or("INVALID_REQUEST")?;
    if offset != received as u64
        || !offset.is_multiple_of(CHUNK as u64)
        || end > expected
        || (end < expected && bytes.len() != CHUNK)
    {
        return Err("INVALID_REQUEST".into());
    }
    sql(tx.execute("INSERT INTO next_storage_write_chunks(transaction_id,key,chunk_index,bytes) VALUES(?1,?2,?3,?4)",params![id,key,(offset/CHUNK as u64) as i64,bytes]))?;
    sql(tx.commit())?;
    Ok(json!({"receivedBytes":end}))
}
pub fn write_commit(db: &Db, owner: &str, session: &str, id: &str) -> Result<()> {
    let mut conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    let tx = sql(conn.transaction())?;
    check(
        &tx,
        "next_storage_write_transactions",
        "transaction_id",
        id,
        owner,
        session,
        false,
    )?;
    let (wr,del):(String,String)=sql(tx.query_row("SELECT writes_json,deletes_json FROM next_storage_write_transactions WHERE transaction_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))))?;
    let writes: Vec<Value> = serde_json::from_str(&wr).map_err(|_| "STORAGE_CORRUPT")?;
    let deletes: Vec<String> = serde_json::from_str(&del).map_err(|_| "STORAGE_CORRUPT")?;
    for spec in writes {
        let key = spec["key"].as_str().ok_or("STORAGE_CORRUPT")?;
        let expected = spec["byteLength"].as_u64().ok_or("STORAGE_CORRUPT")? as usize;
        if expected > VALUE {
            return Err("STORAGE_CORRUPT".into());
        }
        let bytes = {
            let mut stmt=sql(tx.prepare("SELECT chunk_index,bytes FROM next_storage_write_chunks WHERE transaction_id=?1 AND key=?2 ORDER BY chunk_index"))?;
            let rows = sql(stmt.query_map(params![id, key], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
            }))?;
            let mut bytes = Vec::with_capacity(expected);
            for (index, row) in rows.enumerate() {
                let (chunk, data) = sql(row)?;
                if chunk != index as i64 || bytes.len() + data.len() > expected {
                    return Err("INVALID_REQUEST".into());
                }
                bytes.extend_from_slice(&data);
            }
            bytes
        };
        if bytes.len() != expected {
            return Err("INVALID_REQUEST".into());
        }
        let raw = std::str::from_utf8(&bytes).map_err(|_| "INVALID_REQUEST")?;
        validate_storage_value(raw).map_err(str::to_owned)?;
        sql(tx.execute("INSERT INTO plugin_storage(plugin_id,key,value) VALUES(?1,?2,?3) ON CONFLICT(plugin_id,key) DO UPDATE SET value=excluded.value,updated_at=datetime('now','localtime')",params![owner,key,raw]))?;
    }
    for key in deletes {
        sql(tx.execute(
            "DELETE FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
            params![owner, key],
        ))?;
    }
    sql(tx.execute(
        "DELETE FROM next_storage_write_transactions WHERE transaction_id=?1",
        [id],
    ))?;
    sql(tx.commit())
}
pub fn write_abort(db: &Db, owner: &str, session: &str, id: &str) -> Result<()> {
    let conn = db.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
    if check(
        &conn,
        "next_storage_write_transactions",
        "transaction_id",
        id,
        owner,
        session,
        true,
    )? {
        sql(conn.execute(
            "DELETE FROM next_storage_write_transactions WHERE transaction_id=?1",
            [id],
        ))?;
    }
    Ok(())
}

pub(crate) fn token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| "STORAGE_UNAVAILABLE")?;
    Ok(bytes.iter().map(|value| format!("{value:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cruciblebox_next_protocol::runtime::Storage;
    fn db() -> (Db, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cb-stream-{}", token().unwrap()));
        std::fs::create_dir(&dir).unwrap();
        let db = Db::open(&dir.join("test.db")).unwrap();
        db.conn().lock().unwrap().execute_batch("INSERT INTO plugins(id,name,version,display_name,entry_main,installed_path) VALUES('a','a','1.0.0','A','','C:/a'),('b','b','1.0.0','B','','C:/b');").unwrap();
        (db, dir)
    }
    fn begin(db: &Db, key: &str, raw: &str) -> String {
        db.write_begin(
            "a",
            "session-a",
            &[json!({"key":key,"byteLength":raw.len()})],
            &[],
        )
        .unwrap()["transactionId"]
            .as_str()
            .unwrap()
            .into()
    }
    fn chunks(db: &Db, id: &str, key: &str, raw: &str) {
        for (i, chunk) in raw.as_bytes().chunks(CHUNK).enumerate() {
            db.write_chunk(
                "a",
                "session-a",
                id,
                key,
                (i * CHUNK) as u64,
                &STANDARD.encode(chunk),
            )
            .unwrap();
        }
    }
    #[test]
    fn large_unicode_snapshot_namespace_and_restart() {
        let (db, dir) = db();
        let raw = serde_json::to_string(&"日记🌏".repeat(131072)).unwrap();
        let id = begin(&db, "diary.original", &raw);
        chunks(&db, &id, "diary.original", &raw);
        assert_eq!(
            db.write_commit("b", "session-a", &id).unwrap_err(),
            "SESSION_DENIED"
        );
        assert_eq!(
            db.write_commit("a", "other-session", &id).unwrap_err(),
            "SESSION_DENIED"
        );
        db.write_commit("a", "session-a", &id).unwrap();
        assert_eq!(db.storage_get("a", "diary.original").unwrap().unwrap(), raw);
        let read = db.read_begin("a", "session-a", "diary.original").unwrap();
        let rid = read["readId"].as_str().unwrap();
        db.storage_set("a", "diary.original", "\"new\"").unwrap();
        assert_eq!(
            db.read_chunk("b", "session-a", rid, 0).unwrap_err(),
            "SESSION_DENIED"
        );
        let mut bytes = Vec::new();
        for offset in (0..raw.len()).step_by(CHUNK) {
            let response = db.read_chunk("a", "session-a", rid, offset as u64).unwrap();
            bytes.extend(STANDARD.decode(response["data"].as_str().unwrap()).unwrap());
        }
        assert_eq!(bytes, raw.as_bytes());
        db.read_close("a", "session-a", rid).unwrap();
        let abandoned = begin(&db, "diary.original", &raw);
        chunks(&db, &abandoned, "diary.original", &raw);
        drop(db);
        let db = Db::open(&dir.join("test.db")).unwrap();
        assert_eq!(
            db.storage_get("a", "diary.original").unwrap().unwrap(),
            "\"new\""
        );
        db.write_abort("a", "session-a", &abandoned).unwrap();
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn sql_fault_and_incomplete_commit_preserve_atomic_pair() {
        let (db, dir) = db();
        db.storage_set("a", "first", "1").unwrap();
        db.storage_set("a", "second", "2").unwrap();
        let id = db
            .write_begin(
                "a",
                "session-a",
                &[
                    json!({"key":"first","byteLength":1}),
                    json!({"key":"second","byteLength":1}),
                ],
                &[],
            )
            .unwrap()["transactionId"]
            .as_str()
            .unwrap()
            .to_owned();
        chunks(&db, &id, "first", "3");
        assert!(db.write_commit("a", "session-a", &id).is_err());
        assert_eq!(db.storage_get("a", "first").unwrap().unwrap(), "1");
        chunks(&db, &id, "second", "4");
        db.conn().lock().unwrap().execute_batch("CREATE TRIGGER fail_pair BEFORE UPDATE ON plugin_storage WHEN NEW.key='second' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert_eq!(
            db.write_commit("a", "session-a", &id).unwrap_err(),
            "STORAGE_UNAVAILABLE"
        );
        assert_eq!(db.storage_get("a", "first").unwrap().unwrap(), "1");
        assert_eq!(db.storage_get("a", "second").unwrap().unwrap(), "2");
        db.conn()
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_pair;")
            .unwrap();
        db.write_commit("a", "session-a", &id).unwrap();
        assert_eq!(db.storage_get("a", "first").unwrap().unwrap(), "3");
        assert_eq!(db.storage_get("a", "second").unwrap().unwrap(), "4");
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn malformed_duplicate_overflow_and_expired_handles_fail_closed() {
        let (db, dir) = db();
        assert!(db
            .write_begin("a", "s", &[json!({"key":"a","byteLength":VALUE+1})], &[])
            .is_err());
        assert!(db
            .write_begin(
                "a",
                "s",
                &[json!({"key":"a","byteLength":1})],
                &[json!("a")]
            )
            .is_err());
        let id = begin(&db, "x", "123");
        assert!(db
            .write_chunk("a", "session-a", &id, "x", 1, "MTIz")
            .is_err());
        assert!(db
            .write_chunk("a", "session-a", &id, "x", 0, "!!!!")
            .is_err());
        chunks(&db, &id, "x", "123");
        assert!(db
            .write_chunk("a", "session-a", &id, "x", 0, "MTIz")
            .is_err());
        db.conn()
            .lock()
            .unwrap()
            .execute(
                "UPDATE next_storage_write_transactions SET expires_at=0 WHERE transaction_id=?1",
                [&id],
            )
            .unwrap();
        assert_eq!(
            db.write_commit("a", "session-a", &id).unwrap_err(),
            "SESSION_DENIED"
        );
        assert!(db.storage_get("a", "x").unwrap().is_none());
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

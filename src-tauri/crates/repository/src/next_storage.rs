//! Adapter preserves existing plugin_storage rows; no schema or key migration here.
use crate::Db;
use cruciblebox_next_protocol::runtime::Storage;
use rusqlite::params;
use serde_json::{json, Value};

pub fn validate_params(method: &str, params: Value) -> Result<(), String> {
    let request = json!({"wireVersion":3,"requestId":"native-storage-validation","session":"a".repeat(64),"method":method,"params":params});
    cruciblebox_next_protocol::validate_typed_request(&request.to_string())
        .map(|_| ())
        .map_err(str::to_owned)
}
impl Storage for Db {
    fn config_get(&self, owner: &str) -> Result<Value, String> {
        self.plugin_config_get(owner)
    }
    fn config_patch(&self, owner: &str, values: &Value) -> Result<Value, String> {
        self.plugin_config_patch(owner, values)
    }
    fn keys(
        &self,
        owner: &str,
        prefix: &str,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Value, String> {
        crate::next_stream_storage::keys(self, owner, prefix, limit, after)
    }
    fn read_begin(&self, owner: &str, session: &str, key: &str) -> Result<Value, String> {
        crate::next_stream_storage::read_begin(self, owner, session, key)
    }
    fn read_chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        offset: u64,
    ) -> Result<Value, String> {
        crate::next_stream_storage::read_chunk(self, owner, session, id, offset)
    }
    fn read_close(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        crate::next_stream_storage::read_close(self, owner, session, id)
    }
    fn write_begin(
        &self,
        owner: &str,
        session: &str,
        writes: &[Value],
        deletes: &[Value],
    ) -> Result<Value, String> {
        crate::next_stream_storage::write_begin(self, owner, session, writes, deletes)
    }
    fn write_chunk(
        &self,
        owner: &str,
        session: &str,
        id: &str,
        key: &str,
        offset: u64,
        data: &str,
    ) -> Result<Value, String> {
        crate::next_stream_storage::write_chunk(self, owner, session, id, key, offset, data)
    }
    fn write_commit(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        crate::next_stream_storage::write_commit(self, owner, session, id)
    }
    fn write_abort(&self, owner: &str, session: &str, id: &str) -> Result<(), String> {
        crate::next_stream_storage::write_abort(self, owner, session, id)
    }

    fn delete(&self, owner: &str, key: &str) -> Result<(), String> {
        validate_params("storage.delete", json!({"key":key}))?;
        self.conn()
            .lock()
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .execute(
                "DELETE FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
                params![owner, key],
            )
            .map(|_| ())
            .map_err(|_| "STORAGE_UNAVAILABLE".into())
    }
    fn batch(&self, owner: &str, operations: &[Value]) -> Result<(), String> {
        validate_params("storage.batch", json!({"operations":operations}))?;
        let mut conn = self.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
        let tx = conn.transaction().map_err(|_| "STORAGE_UNAVAILABLE")?;
        for op in operations {
            let key = op["key"].as_str().ok_or("INVALID_REQUEST")?;
            match op["type"].as_str() {
                Some("set") => {
                    let value =
                        serde_json::to_string(&op["value"]).map_err(|_| "INVALID_REQUEST")?;
                    tx.execute("INSERT INTO plugin_storage(plugin_id,key,value) VALUES(?1,?2,?3) ON CONFLICT(plugin_id,key) DO UPDATE SET value=excluded.value,updated_at=datetime('now','localtime')",params![owner,key,value]).map_err(|_|"STORAGE_UNAVAILABLE")?;
                }
                Some("delete") => {
                    tx.execute(
                        "DELETE FROM plugin_storage WHERE plugin_id=?1 AND key=?2",
                        params![owner, key],
                    )
                    .map_err(|_| "STORAGE_UNAVAILABLE")?;
                }
                _ => return Err("INVALID_REQUEST".into()),
            }
        }
        tx.commit().map_err(|_| "STORAGE_UNAVAILABLE".into())
    }
    fn list(
        &self,
        owner: &str,
        prefix: &str,
        limit: u32,
        after: Option<&str>,
    ) -> Result<Value, String> {
        let mut params = json!({"prefix":prefix,"limit":limit});
        if let Some(after) = after {
            params["after"] = json!(after);
        }
        validate_params("storage.list", params)?;
        let rows = {
            let conn = self.conn().lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
            let mut stmt=conn.prepare("SELECT key,CASE WHEN length(CAST(value AS BLOB))<=65536 THEN value ELSE NULL END FROM plugin_storage WHERE plugin_id=?1 AND substr(key,1,length(?2))=?2 AND key>?3 COLLATE BINARY ORDER BY key COLLATE BINARY LIMIT ?4").map_err(|_|"STORAGE_UNAVAILABLE")?;
            let rows = stmt
                .query_map(
                    params![owner, prefix, after.unwrap_or(""), limit + 1],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
                )
                .map_err(|_| "STORAGE_UNAVAILABLE")?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| "STORAGE_UNAVAILABLE")?
        };
        let mut items = Vec::new();
        let mut next_cursor = None;
        for (index, (key, raw)) in rows.iter().enumerate() {
            if index == limit as usize {
                next_cursor = items
                    .last()
                    .and_then(|v: &Value| v["key"].as_str().map(str::to_owned));
                break;
            }
            validate_params("storage.get", json!({"key":key})).map_err(|_| "STORAGE_CORRUPT")?;
            let value = cruciblebox_next_protocol::validate_payload(
                raw.as_deref().ok_or("STORAGE_CORRUPT")?,
            )
            .map_err(|_| "STORAGE_CORRUPT")?;
            items.push(json!({"key":key,"value":value}));
            let candidate = json!({"items":items,"nextCursor":key});
            if candidate.to_string().len() > 60 * 1024
                || cruciblebox_next_protocol::validate_payload(&candidate.to_string()).is_err()
            {
                items.pop();
                if items.is_empty() {
                    return Err("BUDGET_EXCEEDED".into());
                }
                next_cursor = items
                    .last()
                    .and_then(|v: &Value| v["key"].as_str().map(str::to_owned));
                break;
            }
        }
        Ok(json!({"items":items,"nextCursor":next_cursor}))
    }

    fn get(&self, owner: &str, key: &str) -> Result<Option<Value>, String> {
        let raw = self
            .storage_get(owner, key)
            .map_err(|_| "STORAGE_UNAVAILABLE")?;
        raw.map(|text| {
            if text.len() > cruciblebox_next_protocol::MAX_FRAME_BYTES {
                return Err("BUDGET_EXCEEDED".to_string());
            }
            cruciblebox_next_protocol::validate_payload(&text)
                .map_err(|_| "STORAGE_CORRUPT".to_string())
        })
        .transpose()
    }
    fn set(&self, owner: &str, key: &str, value: &Value) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|_| "INVALID_REQUEST")?;
        self.storage_set(owner, key, &raw)
            .map_err(|_| "STORAGE_UNAVAILABLE".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cruciblebox_next_protocol::{gateway::Session, runtime::dispatch};
    fn session(owner: &str) -> Session {
        Session::new(
            "a".repeat(64),
            "main".into(),
            owner.into(),
            100,
            ["storage:read".into(), "storage:write".into()],
        )
    }
    fn request(id: &str, method: &str, params: Value) -> String {
        serde_json::json!({"wireVersion":3,"requestId":id,"session":"a".repeat(64),
            "method":method,"params":params})
        .to_string()
    }
    #[test]
    fn real_database_namespace_survives_restart_and_keeps_corrupt_legacy_row() {
        let scratch = tempfile::Builder::new()
            .prefix("cb-next-storage-")
            .tempdir()
            .unwrap();
        let dir = scratch.path().to_path_buf();
        let path = dir.join("test.db");
        {
            let db = Db::open(&path).unwrap();
            db.conn()
                .lock()
                .unwrap()
                .execute_batch(
                    "INSERT INTO plugins(id,name,version,display_name,entry_main,installed_path)
                 VALUES('a','a','1.0.0','A','','C:/a'),('b','b','1.0.0','B','','C:/b');",
                )
                .unwrap();
            let mut a = session("a");
            let set = request(
                "set",
                "storage.set",
                serde_json::json!({"key":"note.v1","value":{"schema":1,"text":"中文"}}),
            );
            dispatch(&mut a, &db, &set, "main", 1).unwrap();
            db.storage_set("a", "legacy", "non-json-preserved").unwrap();
            db.storage_set(
                "a",
                "oversized",
                &serde_json::to_string(&"x".repeat(65536)).unwrap(),
            )
            .unwrap();
        }
        {
            let db = Db::open(&path).unwrap();
            let get = request("get", "storage.get", serde_json::json!({"key":"note.v1"}));
            assert_eq!(
                dispatch(&mut session("a"), &db, &get, "main", 1).unwrap(),
                serde_json::json!({"schema":1,"text":"中文"})
            );
            assert_eq!(
                dispatch(&mut session("b"), &db, &get, "main", 1).unwrap(),
                Value::Null
            );
            assert_eq!(
                Storage::get(&db, "a", "legacy").unwrap_err(),
                "STORAGE_CORRUPT"
            );
            assert_eq!(
                Storage::get(&db, "a", "oversized").unwrap_err(),
                "BUDGET_EXCEEDED"
            );
            assert_eq!(
                db.storage_get("a", "oversized").unwrap().unwrap().len(),
                65538
            );
            assert_eq!(
                db.storage_get("a", "legacy").unwrap().unwrap(),
                "non-json-preserved"
            );
        }
        // TempDir owns only this test's fresh directory, including SQLite auxiliary files.
        scratch.close().unwrap();
    }
    #[test]
    fn real_batch_is_atomic_pagination_literal_and_owner_scoped() {
        let dir = std::env::temp_dir().join(format!(
            "cb-next-batch-{}",
            crate::next_stream_storage::token().unwrap()
        ));
        std::fs::create_dir(&dir).unwrap();
        let db = Db::open(&dir.join("data.db")).unwrap();
        db.conn().lock().unwrap().execute_batch("INSERT INTO plugins(id,name,version,display_name,entry_main,installed_path) VALUES('a','a','1.0.0','A','','C:/a'),('b','b','1.0.0','B','','C:/b');").unwrap();
        for (key, n) in [("entry:_a", 1), ("entry:_b", 2), ("entry:xc", 3)] {
            Storage::set(&db, "a", key, &json!(n)).unwrap();
            Storage::set(&db, "b", key, &json!(999)).unwrap();
        }
        let page = Storage::list(&db, "a", "entry:_", 1, None).unwrap();
        assert_eq!(page["items"], json!([{"key":"entry:_a","value":1}]));
        assert_eq!(page["nextCursor"], "entry:_a");
        let second = Storage::list(&db, "a", "entry:_", 1, Some("entry:_a")).unwrap();
        assert_eq!(second["items"], json!([{"key":"entry:_b","value":2}]));
        assert!(second["nextCursor"].is_null());
        db.conn().lock().unwrap().execute_batch("CREATE TRIGGER inject_next_batch BEFORE INSERT ON plugin_storage WHEN NEW.key='fail' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        let ops = vec![
            json!({"type":"set","key":"entry:_a","value":100}),
            json!({"type":"set","key":"fail","value":1}),
        ];
        assert_eq!(
            Storage::batch(&db, "a", &ops).unwrap_err(),
            "STORAGE_UNAVAILABLE"
        );
        assert_eq!(Storage::get(&db, "a", "entry:_a").unwrap(), Some(json!(1)));
        Storage::batch(
            &db,
            "a",
            &[
                json!({"type":"set","key":"entry:_a","value":4}),
                json!({"type":"delete","key":"entry:_b"}),
            ],
        )
        .unwrap();
        assert_eq!(Storage::get(&db, "a", "entry:_a").unwrap(), Some(json!(4)));
        assert_eq!(Storage::get(&db, "a", "entry:_b").unwrap(), None);
        assert_eq!(
            Storage::get(&db, "b", "entry:_b").unwrap(),
            Some(json!(999))
        );
        assert!(Storage::batch(
            &db,
            "a",
            &[json!({"type":"delete","key":"entry:_a","owner":"b"})]
        )
        .is_err());
        assert_eq!(Storage::get(&db, "a", "entry:_a").unwrap(), Some(json!(4)));
        Storage::delete(&db, "a", "entry:_a").unwrap();
        assert_eq!(
            Storage::get(&db, "b", "entry:_a").unwrap(),
            Some(json!(999))
        );
    }
    #[test]
    fn list_budget_pages_preserve_large_or_corrupt_legacy_values() {
        let dir = std::env::temp_dir().join(format!(
            "cb-next-list-budget-{}",
            crate::next_stream_storage::token().unwrap()
        ));
        std::fs::create_dir(&dir).unwrap();
        let db = Db::open(&dir.join("data.db")).unwrap();
        db.conn().lock().unwrap().execute_batch("INSERT INTO plugins(id,name,version,display_name,entry_main,installed_path) VALUES('a','a','1.0.0','A','','C:/a');").unwrap();
        for n in 0..3 {
            Storage::set(&db, "a", &format!("page:{n}"), &json!("x".repeat(25000))).unwrap();
        }
        let first = Storage::list(&db, "a", "page:", 100, None).unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(), 2);
        assert_eq!(first["nextCursor"], "page:1");
        let second = Storage::list(&db, "a", "page:", 100, Some("page:1")).unwrap();
        assert_eq!(second["items"].as_array().unwrap().len(), 1);
        assert!(second["nextCursor"].is_null());
        db.storage_set("a", "bad", "non-json-preserved").unwrap();
        assert_eq!(
            Storage::list(&db, "a", "bad", 1, None).unwrap_err(),
            "STORAGE_CORRUPT"
        );
        assert_eq!(
            db.storage_get("a", "bad").unwrap().as_deref(),
            Some("non-json-preserved")
        );
        let large = serde_json::to_string(&"x".repeat(70000)).unwrap();
        db.storage_set("a", "large", &large).unwrap();
        assert_eq!(
            Storage::list(&db, "a", "large", 1, None).unwrap_err(),
            "STORAGE_CORRUPT"
        );
        assert_eq!(db.storage_get("a", "large").unwrap(), Some(large));
    }
}

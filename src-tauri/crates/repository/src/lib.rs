pub mod management;
pub mod next_storage;
mod next_stream_storage;
pub mod task_projection;
// CrucibleBox DB 层（1.8.1）
// rusqlite (bundled) 对等迁移自 better-sqlite3 引擎（database/index.ts）。
// - 文件格式零迁移：SQLite 3.x 向后兼容，直接打开现有 openbox.db
// - bundled 默认 foreign_keys=ON；仍显式设置以保持可读性
// - journal_mode=WAL + busy_timeout 默认 5000ms（rusqlite 内置）

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Cheap repository handle sharing the same guarded connection. Cloning never
/// opens another connection or reruns migrations; long services can release the
/// outer application-state mutex before calling observers or filesystem work.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

/// Merge committed WAL pages before a caller copies the database file.
pub fn checkpoint_wal_before_copy(path: &Path) -> rusqlite::Result<()> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")
}

impl Db {
    /// 打开数据库并执行 v1-v10 迁移与日志清理。与 better-sqlite3 引擎语义对等：
    /// 迁移失败则抛错（调用方安全退出）；日志清理失败仅记录（不阻断）。
    /// 注意：调用方需确保父目录已创建（对等 TS getDbPath 的 mkdirSync）。
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // 行为变更记录（oracle L8）：better-sqlite3 引擎从未开启 FK，因此 ON DELETE CASCADE
        // 此前不生效；rusqlite bundled 默认 foreign_keys=ON。对存量孤儿行无回溯影响，
        // 卸载前在同一事务内保留配置、存储和迁移标记；cascade 只清理活动安装行。
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // rusqlite 新连接默认 busy_timeout=5000ms；显式声明保持可读性
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let db = Db {
            conn: Arc::new(Mutex::new(conn)),
        };
        run_migrations(&db)?;
        // 日志清理 best-effort（对等 TS 的 try/catch 忽略路径）
        if let Err(err) = cleanup_plugin_logs(&db) {
            eprintln!("[DB] cleanup plugin logs failed (ignored): {err}");
        }
        Ok(db)
    }

    /// 关闭数据库（进程退出时由 OS 兜底；此方法保留以供显式关闭路径）
    #[allow(dead_code)]
    pub fn close(&self) {
        // rusqlite Connection::close 需所有权；Mutex 包裹下无法移出。
        // 进程退出时 OS 自动释放，DB 事务由 WAL 保证一致性。
    }

    /// 暴露内部连接（调用方自行加锁；仅用于对等现有 TS 语义的命令层）
    fn conn(&self) -> &Mutex<Connection> {
        &self.conn
    }
    /// SQL access exists only for external failure-injection fixtures, never in a production build.
    #[cfg(feature = "test-fixtures")]
    pub fn fixture_connection(&self) -> &Mutex<Connection> {
        &self.conn
    }

    /// 读取 user_version（schema 版本）
    pub fn setting_get(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()
            .map(Option::flatten)
    }
    pub fn setting_set(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute("INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
        Ok(())
    }
    pub fn settings_all(&self) -> rusqlite::Result<Vec<(String, String)>> {
        let connection = self.conn.lock().unwrap();
        let mut statement = connection.prepare("SELECT key,value FROM settings")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }
    pub fn version(&self) -> rusqlite::Result<i64> {
        let guard = self.conn.lock().unwrap();
        pragma_i64(&guard, "user_version")
    }

    /// 返回数据库自检信息（供前端诊断/基准）
    pub fn status(&self) -> rusqlite::Result<DbStatus> {
        let guard = self.conn.lock().unwrap();
        let version: i64 = pragma_i64(&guard, "user_version")?;
        let journal: String =
            guard.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?;
        let fk: i64 = pragma_i64(&guard, "foreign_keys")?;
        Ok(DbStatus {
            version,
            journal_mode: journal,
            foreign_keys: fk == 1,
        })
    }

    // -----------------------------------------------------------------------
    // 插件存储读写层（1.9.2-a，host 方法 storage.* 用；对等 pluginStorage.ts CRUD）
    // -----------------------------------------------------------------------

    /// storage.get：返回原始 JSON 字符串（不存在 → None）
    pub fn storage_get(&self, plugin_id: &str, key: &str) -> rusqlite::Result<Option<String>> {
        let guard = self.conn.lock().unwrap();
        guard
            .query_row(
                "SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![plugin_id, key],
                |row| row.get(0),
            )
            .optional()
    }

    /// storage.set：upsert
    pub fn storage_set(&self, plugin_id: &str, key: &str, value: &str) -> rusqlite::Result<()> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute(
                "INSERT INTO plugin_storage (plugin_id, key, value, updated_at)
                 VALUES (?1, ?2, ?3, datetime('now', 'localtime'))
                 ON CONFLICT(plugin_id, key) DO UPDATE SET
                   value = excluded.value, updated_at = excluded.updated_at",
                rusqlite::params![plugin_id, key, value],
            )
            .map(|_| ())
    }

    /// storage.delete
    pub fn storage_delete(&self, plugin_id: &str, key: &str) -> rusqlite::Result<()> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute(
                "DELETE FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![plugin_id, key],
            )
            .map(|_| ())
    }

    /// storage.list：prefix 为空则全量
    pub fn storage_list(
        &self,
        plugin_id: &str,
        prefix: &str,
    ) -> rusqlite::Result<Vec<(String, String)>> {
        let guard = self.conn.lock().unwrap();
        let rows: Vec<(String, String)> = if prefix.is_empty() {
            let mut stmt = guard.prepare(
                "SELECT key, value FROM plugin_storage WHERE plugin_id = ?1 ORDER BY key",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map([plugin_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        } else {
            let mut stmt = guard.prepare(
                "SELECT key, value FROM plugin_storage
                 WHERE plugin_id = ?1 AND substr(key, 1, ?2) = ?3 ORDER BY key",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map(
                    rusqlite::params![plugin_id, prefix.len() as i64, prefix],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            collected
        };
        Ok(rows)
    }

    /// storage.batch：事务内原子执行 1..=64 条 set/delete
    pub fn storage_batch(
        &self,
        plugin_id: &str,
        mutations: &[(bool, String, Option<String>)],
    ) -> rusqlite::Result<()> {
        if mutations.is_empty() || mutations.len() > 64 {
            return Err(rusqlite::Error::InvalidParameterCount(0, 64));
        }
        let guard = self.conn.lock().unwrap();
        guard.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> rusqlite::Result<()> {
            for (is_set, key, value) in mutations {
                if *is_set {
                    let v = value.as_deref().unwrap_or("null");
                    guard
                        .execute(
                            "INSERT INTO plugin_storage (plugin_id, key, value, updated_at)
                             VALUES (?1, ?2, ?3, datetime('now', 'localtime'))
                             ON CONFLICT(plugin_id, key) DO UPDATE SET
                               value = excluded.value, updated_at = excluded.updated_at",
                            rusqlite::params![plugin_id, key, v],
                        )
                        .map(|_| ())?;
                } else {
                    guard
                        .execute(
                            "DELETE FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
                            rusqlite::params![plugin_id, key],
                        )
                        .map(|_| ())?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                guard.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                let _ = guard.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// 将已安装旧插件的数据复制到 2.1 综合插件。复制使用独立前缀并写完成标记，
    /// 旧命名空间保持不变；重复安装或升级不会制造重复记录。
    pub fn migrate_consolidated_plugin_data(
        &self,
        target_plugin_id: &str,
        target_name: &str,
    ) -> Result<Vec<(String, i64)>, String> {
        let sources: &[&str] = match target_name {
            "diary" => &["productivity-toolkit"],
            "clipboard-manager" => &["productivity-toolkit"],
            "document-engine" => &["archive-extractor"],
            "media-toolkit" => &["gif-editor"],
            "developer-toolkit" => &["json-toolkit"],
            "productivity-toolkit" => &[
                "diary",
                "clipboard-manager",
                "dice-roller",
                "turntable",
                "exchange-rates",
            ],
            _ => return Ok(Vec::new()),
        };
        let guard = self.conn.lock().unwrap();
        guard
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let result = (|| -> rusqlite::Result<Vec<(String, i64)>> {
            let mut summary = Vec::new();
            for source_name in sources {
                let Some(source_id) = plugin_id_by_name(&guard, source_name)? else {
                    summary.push((source_name.to_string(), 0));
                    continue;
                };
                let already: Option<i64> = guard
                    .query_row(
                        "SELECT copied_records FROM plugin_consolidation_migrations
                         WHERE target_plugin_id = ?1 AND source_plugin_id = ?2",
                        params![target_plugin_id, source_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(count) = already {
                    summary.push((source_name.to_string(), count));
                    continue;
                }
                let copied = guard.execute(
                    "INSERT OR IGNORE INTO plugin_storage(plugin_id, key, value, updated_at)
                     SELECT ?1, 'legacy:' || ?2 || ':' || key, value, updated_at
                     FROM plugin_storage WHERE plugin_id = ?3",
                    params![target_plugin_id, source_name, source_id],
                )? as i64;

                if target_name == "diary" && *source_name == "productivity-toolkit" {
                    guard.execute(
                        "INSERT OR IGNORE INTO plugin_storage(plugin_id, key, value, updated_at)
                         SELECT ?1, 'notes', value, updated_at FROM plugin_storage
                         WHERE plugin_id = ?2 AND key = 'notes'",
                        params![target_plugin_id, source_id],
                    )?;
                }
                if target_name == "clipboard-manager" && *source_name == "productivity-toolkit" {
                    let clips: Option<String> = guard.query_row(
                        "SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = 'clips'",
                        [&source_id], |row| row.get(0)
                    ).optional()?;
                    if let Some(clips) = clips {
                        let items = serde_json::from_str::<serde_json::Value>(&clips).ok()
                            .and_then(|value| value.as_array().cloned()).unwrap_or_default()
                            .into_iter().map(|item| serde_json::json!({
                                "id": item["id"], "text": item["text"],
                                "timestamp": item.get("createdAt").or_else(|| item.get("timestamp")).cloned().unwrap_or_else(|| serde_json::json!(0)),
                                "pinned": item["pinned"].as_bool().unwrap_or(false)
                            })).collect::<Vec<_>>();
                        insert_migrated_value(
                            &guard,
                            target_plugin_id,
                            "history",
                            &serde_json::Value::Array(items),
                        )?;
                    }
                }

                if target_name == "productivity-toolkit" && *source_name == "diary" {
                    let mut statement = guard.prepare(
                        "SELECT value FROM plugin_storage
                         WHERE plugin_id = ?1 AND key LIKE 'entry:%' ORDER BY key",
                    )?;
                    let notes = statement
                        .query_map([&source_id], |row| row.get::<_, String>(0))?
                        .filter_map(Result::ok)
                        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                        .enumerate()
                        .map(|(index, entry)| {
                            let date = entry["entry_date"].as_str().unwrap_or_default();
                            serde_json::json!({
                                "id": format!("legacy-diary-{date}-{index}"),
                                "title": entry["title"].as_str().unwrap_or("日记"),
                                "content": entry["content"].as_str().unwrap_or_default(),
                                "tags": ["日记", "已迁移"],
                                "date": date,
                                "updatedAt": 0,
                                "versions": []
                            })
                        })
                        .collect::<Vec<_>>();
                    if !notes.is_empty() {
                        insert_migrated_value(
                            &guard,
                            target_plugin_id,
                            "notes",
                            &serde_json::Value::Array(notes),
                        )?;
                    }
                }
                if target_name == "productivity-toolkit" && *source_name == "clipboard-manager" {
                    let history: Option<String> = guard
                        .query_row(
                            "SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = 'history'",
                            [&source_id],
                            |row| row.get(0),
                        )
                        .optional()?;
                    if let Some(history) = history {
                        let clips = serde_json::from_str::<serde_json::Value>(&history)
                            .ok()
                            .and_then(|value| value.as_array().cloned())
                            .unwrap_or_default()
                            .into_iter()
                            .map(|item| serde_json::json!({
                                "id": item["id"],
                                "text": item["text"],
                                "createdAt": item.get("createdAt").or_else(|| item.get("timestamp")).cloned().unwrap_or_else(|| serde_json::json!(0)),
                                "pinned": item["pinned"].as_bool().unwrap_or(false),
                                "tags": item["tags"].as_array().cloned().unwrap_or_default()
                            }))
                            .collect::<Vec<_>>();
                        insert_migrated_value(
                            &guard,
                            target_plugin_id,
                            "clips",
                            &serde_json::Value::Array(clips),
                        )?;
                    }
                }
                guard.execute(
                    "INSERT INTO plugin_consolidation_migrations
                     (target_plugin_id, source_plugin_id, copied_records) VALUES (?1, ?2, ?3)",
                    params![target_plugin_id, source_id, copied],
                )?;
                summary.push((source_name.to_string(), copied));
            }
            Ok(summary)
        })();
        match result {
            Ok(summary) => {
                guard.execute_batch("COMMIT").map_err(|e| e.to_string())?;
                Ok(summary)
            }
            Err(error) => {
                let _ = guard.execute_batch("ROLLBACK");
                Err(error.to_string())
            }
        }
    }

    /// log.write：插件日志入库
    pub fn log_write(&self, plugin_id: &str, level: &str, message: &str) -> rusqlite::Result<()> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute(
                "INSERT INTO plugin_logs (plugin_id, level, message) VALUES (?1, ?2, ?3)",
                rusqlite::params![plugin_id, level, message],
            )
            .map(|_| ())
    }

    /// 查询单个插件记录（host 方法用：permissions/enabled/installed_path/entry_main）
    pub fn plugin_backend_record(
        &self,
        plugin_id: &str,
    ) -> rusqlite::Result<Option<PluginBackendRecord>> {
        let guard = self.conn.lock().unwrap();
        guard
            .query_row(
                "SELECT enabled, permissions, installed_path, entry_main, name, config_schema, config_data
                 FROM plugins WHERE id = ?1",
                [plugin_id],
                |row| {
                    Ok(PluginBackendRecord {
                        enabled: row.get::<_, i64>(0)? == 1,
                        permissions: row.get::<_, String>(1)?,
                        installed_path: row.get::<_, String>(2)?,
                        entry_main: row.get::<_, String>(3)?,
                        name: row.get::<_, String>(4)?,
                        config_schema: row.get::<_, String>(5)?,
                        config_data: row.get::<_, String>(6)?,
                    })
                },
            )
            .optional()
    }

    /// 返回所有已启用插件的 backend 记录；启动时用于恢复需要宿主监控的插件。
    pub fn enabled_plugin_backend_records(
        &self,
    ) -> rusqlite::Result<Vec<(String, PluginBackendRecord)>> {
        let guard = self.conn.lock().unwrap();
        let mut stmt = guard.prepare(
            "SELECT id, enabled, permissions, installed_path, entry_main, name, config_schema, config_data
             FROM plugins WHERE enabled = 1 ORDER BY sort_order ASC, id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    PluginBackendRecord {
                        enabled: row.get::<_, i64>(1)? == 1,
                        permissions: row.get::<_, String>(2)?,
                        installed_path: row.get::<_, String>(3)?,
                        entry_main: row.get::<_, String>(4)?,
                        name: row.get::<_, String>(5)?,
                        config_schema: row.get::<_, String>(6)?,
                        config_data: row.get::<_, String>(7)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 持久化插件启用状态（崩溃隔离时置 disabled；对等 Electron 的持久化隔离结果）
    pub fn set_plugin_enabled(&self, plugin_id: &str, enabled: bool) -> rusqlite::Result<()> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute(
                "UPDATE plugins SET enabled = ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
                rusqlite::params![if enabled { 1 } else { 0 }, plugin_id],
            )
            .map(|_| ())
    }

    /// 事务化重排所有插件（对等 PluginRepository.reorder：完整排列校验 + 原子提交）
    pub fn plugin_reorder(&self, ordered_ids: &[String]) -> Result<Vec<String>, String> {
        let guard = self.conn.lock().unwrap();
        // 校验：必须是全部已安装插件 ID 的完整排列
        let existing: Vec<String> = {
            let ids: Vec<String> = guard
                .prepare("SELECT id FROM plugins")
                .map_err(|e| e.to_string())?
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            ids
        };
        if ordered_ids.len() != existing.len() {
            return Err("排序列表必须包含全部已安装插件".into());
        }
        let existing_set: HashSet<&str> = existing.iter().map(|s| s.as_str()).collect();
        let mut seen = HashSet::new();
        for id in ordered_ids {
            if id.is_empty() || seen.contains(id.as_str()) || !existing_set.contains(id.as_str()) {
                return Err("排序列表包含重复、缺失或未知的插件 ID".into());
            }
            seen.insert(id.as_str());
        }
        guard
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), String> {
            for (index, id) in ordered_ids.iter().enumerate() {
                guard
                    .execute(
                        "UPDATE plugins SET sort_order = ?1 WHERE id = ?2",
                        rusqlite::params![(index + 1) as i64, id],
                    )
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                guard.execute_batch("COMMIT").map_err(|e| e.to_string())?;
                Ok(ordered_ids.to_vec())
            }
            Err(e) => {
                let _ = guard.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn plugin_config_get(&self, owner: &str) -> Result<serde_json::Value, String> {
        let record = self
            .plugin_backend_record(owner)
            .map_err(|_| "STORAGE_UNAVAILABLE")?
            .ok_or("SESSION_DENIED")?;
        if !record.enabled {
            return Err("SESSION_DENIED".into());
        }
        record
            .resolved_config()
            .map_err(|_| "STORAGE_UNAVAILABLE".into())
    }
    pub fn plugin_config_patch(
        &self,
        owner: &str,
        values: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let values = values.as_object().ok_or("INVALID_REQUEST")?;
        if values.len() > 100
            || values.keys().any(|key| {
                key.is_empty()
                    || key.len() > 128
                    || key.contains('\0')
                    || ["__proto__", "prototype", "constructor"].contains(&key.as_str())
            })
        {
            return Err("INVALID_REQUEST".into());
        }
        let mut conn = self.conn.lock().map_err(|_| "STORAGE_UNAVAILABLE")?;
        let tx = conn.transaction().map_err(|_| "STORAGE_UNAVAILABLE")?;
        let saved: Option<(String, String)> = tx
            .query_row(
                "SELECT config_schema,config_data FROM plugins WHERE id=?1 AND enabled=1",
                [owner],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| "STORAGE_UNAVAILABLE")?;
        let (schema, saved) = saved.ok_or("SESSION_DENIED")?;
        let mut saved: serde_json::Value =
            serde_json::from_str(&saved).map_err(|_| "STORAGE_UNAVAILABLE")?;
        saved
            .as_object_mut()
            .ok_or("STORAGE_UNAVAILABLE")?
            .extend(values.clone());
        let raw = serde_json::to_string(&saved).map_err(|_| "INVALID_REQUEST")?;
        if raw.len() > 65536 {
            return Err("BUDGET_EXCEEDED".into());
        }
        let schema: serde_json::Value =
            serde_json::from_str(&schema).map_err(|_| "STORAGE_UNAVAILABLE")?;
        let mut resolved = serde_json::Map::new();
        for (key, field) in schema.as_object().ok_or("STORAGE_UNAVAILABLE")? {
            if let Some(default) = field.get("default") {
                resolved.insert(key.clone(), default.clone());
            }
        }
        resolved.extend(saved.as_object().unwrap().clone());
        let resolved = serde_json::Value::Object(resolved);
        if serde_json::to_vec(&resolved)
            .map_err(|_| "INVALID_REQUEST")?
            .len()
            > 65536
        {
            return Err("BUDGET_EXCEEDED".into());
        }
        tx.execute("UPDATE plugins SET config_data=?1,updated_at=datetime('now','localtime') WHERE id=?2 AND enabled=1",params![raw,owner]).map_err(|_| "STORAGE_UNAVAILABLE")?;
        tx.commit().map_err(|_| "STORAGE_UNAVAILABLE")?;
        Ok(resolved)
    }

    /// 更新插件配置（对等 updateConfig）
    pub fn plugin_update_config(&self, plugin_id: &str, config: &str) -> rusqlite::Result<()> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute(
                "UPDATE plugins SET config_data = ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
                rusqlite::params![config, plugin_id],
            )
            .map(|_| ())
    }

    /// 查询插件日志（对等 getLogs：pluginId/level/limit 过滤）
    pub fn plugin_logs(
        &self,
        plugin_id: Option<&str>,
        level: Option<&str>,
        limit: i64,
    ) -> Result<Vec<PluginLogEntry>, String> {
        let guard = self.conn.lock().unwrap();
        let (where_clause, params): (String, Vec<Box<dyn rusqlite::ToSql>>) =
            match (plugin_id, level) {
                (Some(pid), Some(lv)) => (
                    "WHERE plugin_id = ?1 AND level = ?2".to_string(),
                    vec![Box::new(pid.to_string()), Box::new(lv.to_string())],
                ),
                (Some(pid), None) => (
                    "WHERE plugin_id = ?1".to_string(),
                    vec![Box::new(pid.to_string())],
                ),
                (None, Some(lv)) => (
                    "WHERE level = ?1".to_string(),
                    vec![Box::new(lv.to_string())],
                ),
                (None, None) => ("".to_string(), vec![]),
            };
        let sql = format!(
            "SELECT id, plugin_id, level, message, timestamp FROM plugin_logs {} \
             ORDER BY id DESC LIMIT ?{}",
            where_clause,
            params.len() + 1
        );
        let mut stmt = guard.prepare(&sql).map_err(|e| e.to_string())?;
        let mut q_params: Vec<Box<dyn rusqlite::ToSql>> = params;
        q_params.push(Box::new(limit));
        let rows = stmt
            .query_map(rusqlite::params_from_iter(q_params.iter()), |row| {
                Ok(PluginLogEntry {
                    id: row.get(0)?,
                    plugin_id: row.get(1)?,
                    level: row.get(2)?,
                    message: row.get(3)?,
                    timestamp: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// 清空插件日志（对等 clearLogs：pluginId 可选）
    pub fn plugin_clear_logs(&self, plugin_id: Option<&str>) -> rusqlite::Result<()> {
        let guard = self.conn.lock().unwrap();
        match plugin_id {
            Some(pid) => guard
                .execute("DELETE FROM plugin_logs WHERE plugin_id = ?1", [pid])
                .map(|_| ()),
            None => guard.execute("DELETE FROM plugin_logs", []).map(|_| ()),
        }
    }

    // -----------------------------------------------------------------------
    // 插件安装链读写层（1.9.3，install.rs 编排层用）
    // -----------------------------------------------------------------------

    /// 按 name 查询插件全行（name 唯一）
    pub fn plugin_find_by_name(&self, name: &str) -> Result<Option<PluginRow>, String> {
        let guard = self.conn.lock().unwrap();
        let mut stmt = guard
            .prepare(&format!(
                "SELECT {PLUGIN_ROW_COLUMNS} FROM plugins WHERE name = ?1"
            ))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([name], row_to_plugin_row)
            .map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 按 id 查询插件全行
    pub fn plugin_find_by_id(&self, id: &str) -> Result<Option<PluginRow>, String> {
        let guard = self.conn.lock().unwrap();
        let mut stmt = guard
            .prepare(&format!(
                "SELECT {PLUGIN_ROW_COLUMNS} FROM plugins WHERE id = ?1"
            ))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([id], row_to_plugin_row)
            .map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 全部插件的基础三元组（id, name, installed_path）——插件根整理（1.9.14）用。
    pub fn plugin_all_roots(&self) -> Result<Vec<(String, String, String)>, String> {
        let guard = self.conn.lock().unwrap();
        let mut stmt = guard
            .prepare("SELECT id, name, installed_path FROM plugins")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }

    /// 改写 installed_path（插件根整理 1.9.14 专用；同时刷新 updated_at）。
    pub fn plugin_update_installed_path(
        &self,
        id: &str,
        installed_path: &str,
    ) -> rusqlite::Result<usize> {
        self.conn.lock().unwrap().execute(
            "UPDATE plugins SET installed_path = ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
            rusqlite::params![installed_path, id],
        )
    }

    /// 新建插件记录（enabled 恒 0；installed_at/updated_at 由 DB 生成）
    #[allow(dead_code)]
    pub fn plugin_create(&self, row: &PluginRow) -> Result<(), String> {
        let mut connection = self.conn.lock().unwrap();
        let guard = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        guard
            .execute(
                "INSERT INTO plugins (id, name, version, display_name, description, author, icon, \
                 entry_main, entry_renderer, permissions, config_schema, config_data, enabled, \
                 installed_path, installed_at, updated_at, sort_order) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, \
                 datetime('now','localtime'), datetime('now','localtime'), ?14)",
                rusqlite::params![
                    row.id,
                    row.name,
                    row.version,
                    row.display_name,
                    row.description,
                    row.author,
                    row.icon,
                    row.entry_main,
                    row.entry_renderer,
                    row.permissions,
                    row.config_schema,
                    row.config_data,
                    row.installed_path,
                    row.sort_order
                ],
            )
            .map_err(|e| e.to_string())?;
        guard.execute("UPDATE plugins SET config_data = (SELECT config_data FROM retained_plugin_data WHERE name=?1) WHERE id=?2 AND EXISTS(SELECT 1 FROM retained_plugin_data WHERE name=?1)", rusqlite::params![row.name,row.id]).map_err(|e| e.to_string())?;
        guard.execute("INSERT INTO plugin_storage(plugin_id,key,value,updated_at) SELECT ?1,key,value,updated_at FROM retained_plugin_storage WHERE name=?2", rusqlite::params![row.id,row.name]).map_err(|e| e.to_string())?;
        guard.execute("INSERT INTO plugin_storage_migrations(plugin_id,migration,applied_at) SELECT ?1,migration,applied_at FROM retained_plugin_migrations WHERE name=?2", rusqlite::params![row.id,row.name]).map_err(|e| e.to_string())?;
        guard
            .execute(
                "DELETE FROM retained_plugin_data WHERE name=?1",
                [&row.name],
            )
            .map_err(|e| e.to_string())?;
        guard.commit().map_err(|e| e.to_string())
    }

    /// 升级：更新版本相关字段（updated_at 刷新）
    pub fn plugin_update_version(&self, id: &str, fields: &VersionFields) -> Result<(), String> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute(
                "UPDATE plugins SET version = ?1, display_name = ?2, description = ?3, \
                 author = ?4, icon = ?5, entry_main = ?6, entry_renderer = ?7, \
                 permissions = ?8, config_schema = ?9, installed_path = ?10, \
                 updated_at = datetime('now','localtime') WHERE id = ?11",
                rusqlite::params![
                    fields.version,
                    fields.display_name,
                    fields.description,
                    fields.author,
                    fields.icon,
                    fields.entry_main,
                    fields.entry_renderer,
                    fields.permissions,
                    fields.config_schema,
                    fields.installed_path,
                    id
                ],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Remove installation metadata; retain exact user values for reinstall by stable plugin name.
    pub fn plugin_delete(&self, id: &str) -> Result<(), String> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let result = (|| -> rusqlite::Result<()> {
            // Copy bytes inside the same transaction before FK cascades. A conflicting
            // retained generation fails closed rather than silently overwriting it.
            guard.execute("INSERT INTO retained_plugin_data(name,source_id,config_data) SELECT name,id,config_data FROM plugins WHERE id=?1", [id])?;
            guard.execute("INSERT INTO retained_plugin_storage(name,key,value,updated_at) SELECT p.name,s.key,s.value,s.updated_at FROM plugin_storage s JOIN plugins p ON p.id=s.plugin_id WHERE p.id=?1", [id])?;
            guard.execute("INSERT INTO retained_plugin_migrations(name,migration,applied_at) SELECT p.name,m.migration,m.applied_at FROM plugin_storage_migrations m JOIN plugins p ON p.id=m.plugin_id WHERE p.id=?1", [id])?;
            guard.execute(
                "DELETE FROM plugin_consolidation_migrations WHERE target_plugin_id = ?1",
                [id],
            )?;
            guard.execute("DELETE FROM plugins WHERE id = ?1", [id])?;
            Ok(())
        })();
        match result {
            Ok(()) => guard.execute_batch("COMMIT").map_err(|e| e.to_string()),
            Err(error) => {
                let _ = guard.execute_batch("ROLLBACK");
                Err(error.to_string())
            }
        }
    }
}

#[derive(Serialize)]
pub struct DbStatus {
    pub version: i64,
    pub journal_mode: String,
    pub foreign_keys: bool,
}

/// 插件 backend 宿主侧所需记录（spawn sidecar 用）
pub struct PluginBackendRecord {
    pub enabled: bool,
    pub permissions: String,
    pub installed_path: String,
    pub entry_main: String,
    #[allow(dead_code)] // 1.9.2-b 日志/诊断用
    pub name: String,
    pub config_schema: String,
    pub config_data: String,
}

impl PluginBackendRecord {
    pub fn resolved_config(&self) -> Result<serde_json::Value, String> {
        let schema: serde_json::Value = serde_json::from_str(&self.config_schema)
            .map_err(|error| format!("invalid plugin config schema: {error}"))?;
        let saved: serde_json::Value = serde_json::from_str(&self.config_data)
            .map_err(|error| format!("invalid plugin config data: {error}"))?;
        let mut config = serde_json::Map::new();
        let schema = schema
            .as_object()
            .ok_or("plugin config schema must be an object")?;
        for (key, field) in schema {
            if let Some(default) = field.get("default") {
                config.insert(key.clone(), default.clone());
            }
        }
        let saved = saved
            .as_object()
            .ok_or("plugin config data must be an object")?;
        config.extend(saved.clone());
        Ok(serde_json::Value::Object(config))
    }
}

/// 插件日志条目（对等 PluginLogEntry）
#[derive(Serialize)]
pub struct PluginLogEntry {
    pub id: i64,
    pub plugin_id: String,
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

/// 插件全行记录（1.9.3 安装链读写用）
pub struct PluginRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub icon: String,
    pub entry_main: String,
    pub entry_renderer: String,
    pub permissions: String,
    pub config_schema: String,
    pub config_data: String,
    pub enabled: bool,
    pub installed_path: String,
    pub installed_at: String,
    pub updated_at: String,
    pub sort_order: i64,
}

/// 升级时更新的版本相关字段（1.9.3）
pub struct VersionFields {
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub icon: String,
    pub entry_main: String,
    pub entry_renderer: String,
    pub permissions: String,
    pub config_schema: String,
    pub installed_path: String,
}

const PLUGIN_ROW_COLUMNS: &str = "id, name, version, display_name, description, author, icon, \
     entry_main, entry_renderer, permissions, config_schema, config_data, enabled, \
     installed_path, installed_at, updated_at, sort_order";

fn row_to_plugin_row(row: &rusqlite::Row) -> rusqlite::Result<PluginRow> {
    Ok(PluginRow {
        id: row.get("id")?,
        name: row.get("name")?,
        version: row.get("version")?,
        display_name: row.get("display_name")?,
        description: row.get("description").unwrap_or_default(),
        author: row.get("author").unwrap_or_default(),
        icon: row.get("icon").unwrap_or_default(),
        entry_main: row.get("entry_main")?,
        entry_renderer: row.get("entry_renderer").unwrap_or_default(),
        permissions: row.get("permissions").unwrap_or_default(),
        config_schema: row.get("config_schema").unwrap_or_default(),
        config_data: row.get("config_data").unwrap_or_default(),
        enabled: row.get::<_, i64>("enabled").unwrap_or(1) == 1,
        installed_path: row.get("installed_path")?,
        installed_at: row.get("installed_at").unwrap_or_default(),
        updated_at: row.get("updated_at").unwrap_or_default(),
        sort_order: row.get("sort_order").unwrap_or(0),
    })
}

fn pragma_i64(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    conn.pragma_query_value(None, name, |row| row.get(0))
}

// ---------------------------------------------------------------------------
// 迁移（对等 database/index.ts MIGRATIONS 数组，v1..=v3）
// ---------------------------------------------------------------------------

const MIGRATIONS_COUNT: i64 = 10;

fn run_migrations(db: &Db) -> rusqlite::Result<()> {
    let mut version = db.version().unwrap_or(0);
    while version < MIGRATIONS_COUNT {
        {
            let guard = db.conn.lock().unwrap();
            guard.execute_batch("BEGIN IMMEDIATE")?;
        }
        match migrate_one(db, version) {
            Ok(()) => {
                let guard = db.conn.lock().unwrap();
                guard.pragma_update(None, "user_version", version + 1)?;
                guard.execute_batch("COMMIT")?;
                version += 1;
            }
            Err(err) => {
                let guard = db.conn.lock().unwrap();
                let _ = guard.execute_batch("ROLLBACK");
                return Err(err);
            }
        }
    }
    Ok(())
}

fn migrate_one(db: &Db, version: i64) -> rusqlite::Result<()> {
    let guard = db.conn.lock().unwrap();
    match version {
        0 => migrate_v1(&guard),
        1 => migrate_v2(&guard),
        2 => migrate_v3(&guard),
        3 => migrate_v4(&guard),
        4 => migrate_v5(&guard),
        5 => migrate_v6(&guard),
        6 => migrate_v7(&guard),
        7 => migrate_v8(&guard),
        8 => guard.execute_batch("CREATE TABLE IF NOT EXISTS host_task_executor_revisions (task_id TEXT PRIMARY KEY, owner TEXT NOT NULL, core_sequence INTEGER NOT NULL CHECK(core_sequence >= 0));"),
        9 => guard.execute_batch("CREATE TABLE IF NOT EXISTS retained_plugin_data (name TEXT PRIMARY KEY, source_id TEXT NOT NULL, config_data TEXT NOT NULL, retained_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP); CREATE TABLE IF NOT EXISTS retained_plugin_storage (name TEXT NOT NULL REFERENCES retained_plugin_data(name) ON DELETE CASCADE, key TEXT NOT NULL, value TEXT NOT NULL, updated_at TEXT, PRIMARY KEY(name,key)); CREATE TABLE IF NOT EXISTS retained_plugin_migrations (name TEXT NOT NULL REFERENCES retained_plugin_data(name) ON DELETE CASCADE, migration TEXT NOT NULL, applied_at TEXT, PRIMARY KEY(name,migration));"),
        _ => Ok(()),
    }
}

/// v1：plugins / settings / plugin_logs + 索引
fn migrate_v1(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS plugins (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          version TEXT NOT NULL,
          display_name TEXT NOT NULL,
          description TEXT DEFAULT '',
          author TEXT DEFAULT '',
          icon TEXT DEFAULT '',
          entry_main TEXT NOT NULL,
          entry_renderer TEXT DEFAULT '',
          permissions TEXT DEFAULT '[]',
          config_schema TEXT DEFAULT '{}',
          config_data TEXT DEFAULT '{}',
          enabled INTEGER DEFAULT 1,
          installed_path TEXT NOT NULL,
          installed_at DATETIME DEFAULT (datetime('now', 'localtime')),
          updated_at DATETIME DEFAULT (datetime('now', 'localtime'))
        );
        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS plugin_logs (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          plugin_id TEXT NOT NULL,
          level TEXT NOT NULL DEFAULT 'info',
          message TEXT NOT NULL,
          timestamp DATETIME DEFAULT (datetime('now', 'localtime')),
          FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_plugin_logs_plugin_id ON plugin_logs(plugin_id);
        CREATE INDEX IF NOT EXISTS idx_plugin_logs_timestamp ON plugin_logs(timestamp);
        "#,
    )
}

/// v2：plugin_storage / plugin_storage_migrations + legacy sql.js 存储迁移
fn migrate_v2(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS plugin_storage (
          plugin_id TEXT NOT NULL,
          key TEXT NOT NULL,
          value TEXT NOT NULL,
          updated_at DATETIME DEFAULT (datetime('now', 'localtime')),
          PRIMARY KEY (plugin_id, key),
          FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS plugin_storage_migrations (
          plugin_id TEXT NOT NULL,
          migration TEXT NOT NULL,
          applied_at DATETIME DEFAULT (datetime('now', 'localtime')),
          PRIMARY KEY (plugin_id, migration),
          FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
        );
        "#,
    )?;
    migrate_legacy_plugin_storage(conn)
}

/// v3：plugins.sort_order ALTER + 稳定回填
fn migrate_v3(conn: &Connection) -> rusqlite::Result<()> {
    // 检查 sort_order 是否已存在（与 TS 的 PRAGMA table_info 等值）
    let has_sort_order: bool = {
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(plugins)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        cols.iter().any(|c| c == "sort_order")
    };
    if !has_sort_order {
        conn.execute_batch("ALTER TABLE plugins ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0")?;
    }
    // 稳定回填：installed_at DESC, id ASC（无 installed_at 则 id ASC）
    let has_installed_at: bool = {
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(plugins)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        cols.iter().any(|c| c == "installed_at")
    };
    let order_by = if has_installed_at {
        "ORDER BY installed_at DESC, id ASC"
    } else {
        "ORDER BY id ASC"
    };
    let ids: Vec<String> = {
        let sql = format!("SELECT id FROM plugins {}", order_by);
        let rows: Vec<String> = conn
            .prepare(&sql)?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        rows
    };
    for (index, id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE plugins SET sort_order = ?1 WHERE id = ?2",
            params![(index + 1) as i64, id],
        )?;
    }
    Ok(())
}

/// v4：修复 L3 数据目录迁移（openbox→cruciblebox）后 plugins.installed_path 的残留。
/// data_dir::migrate 只搬文件不改写 DB 内的绝对路径，迁移过的旧安装全部
/// renderer 会话/后端启动都会 "failed to read plugin manifest"（Bug G）。
/// 按路径段精确替换（两侧带分隔符），幂等；对未受影响的行零副作用。
fn migrate_v4(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        r"UPDATE plugins
         SET installed_path = REPLACE(installed_path, '\openbox\', '\cruciblebox\')
         WHERE installed_path LIKE '%\openbox\%'",
        [],
    )?;
    conn.execute(
        "UPDATE plugins
         SET installed_path = REPLACE(installed_path, '/openbox/', '/cruciblebox/')
         WHERE installed_path LIKE '%/openbox/%'",
        [],
    )?;
    Ok(())
}

/// v5：记录 2.1 综合插件的数据迁移状态。旧插件数据仍保留在原命名空间，
/// 新插件安装完成后由安装事务复制，因而中断后可以继续执行。
fn migrate_v5(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS plugin_consolidation_migrations (
          target_plugin_id TEXT NOT NULL,
          source_plugin_id TEXT NOT NULL,
          copied_records INTEGER NOT NULL DEFAULT 0,
          applied_at DATETIME DEFAULT (datetime('now', 'localtime')),
          PRIMARY KEY (target_plugin_id, source_plugin_id)
        );
        "#,
    )?;
    // 主题与系统信息已经并入宿主；保留旧设置快照供兼容期导出。
    for source in ["theme-manager", "system-info"] {
        let Some(source_id) = plugin_id_by_name(conn, source)? else {
            continue;
        };
        conn.execute(
            "INSERT OR IGNORE INTO settings(key, value)
             SELECT 'legacyPluginData:' || ?1 || ':' || key, value
             FROM plugin_storage WHERE plugin_id = ?2",
            params![source, source_id],
        )?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM plugin_storage WHERE plugin_id = ?1",
            [&source_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO plugin_consolidation_migrations
             (target_plugin_id, source_plugin_id, copied_records) VALUES ('host', ?1, ?2)",
            params![source_id, count],
        )?;
    }
    Ok(())
}

/// v6: user-owned tags are separate from manifest names and categories.
fn migrate_v6(conn: &Connection) -> rusqlite::Result<()> {
    // Move beta.1 notes back into the dedicated diary-and-notes plugin while
    // retaining the original productivity namespace as a read-only source.
    if let (Some(source_id), Some(target_id)) = (
        plugin_id_by_name(conn, "productivity-toolkit")?,
        plugin_id_by_name(conn, "diary")?,
    ) {
        conn.execute(
            "INSERT OR IGNORE INTO plugin_storage(plugin_id, key, value, updated_at)
             SELECT ?1, 'notes', value, updated_at
             FROM plugin_storage WHERE plugin_id = ?2 AND key = 'notes'",
            params![target_id, source_id],
        )?;
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_plugin_tags (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           name TEXT NOT NULL UNIQUE COLLATE NOCASE
         );
         CREATE TABLE IF NOT EXISTS user_plugin_tag_links (
           plugin_id TEXT NOT NULL,
           tag_id INTEGER NOT NULL,
           PRIMARY KEY (plugin_id, tag_id),
           FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE,
           FOREIGN KEY (tag_id) REFERENCES user_plugin_tags(id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_user_plugin_tag_links_tag ON user_plugin_tag_links(tag_id);",
    )?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_sources (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           name TEXT NOT NULL,
           url TEXT NOT NULL UNIQUE,
           enabled INTEGER NOT NULL DEFAULT 1
         );
         CREATE TABLE IF NOT EXISTS plugin_marketplace_origins (
           plugin_id TEXT PRIMARY KEY,
           source_id INTEGER NOT NULL,
           source_url TEXT NOT NULL,
           FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
         );",
    )
}

/// v7: host-owned task journal and independent favorite/recent metadata.
fn migrate_v7(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS host_tasks (
           id TEXT PRIMARY KEY,
           owner TEXT NOT NULL,
           kind TEXT NOT NULL,
           source TEXT NOT NULL,
           title TEXT NOT NULL,
           detail TEXT NOT NULL DEFAULT '',
           status TEXT NOT NULL CHECK(status IN
             ('queued','running','paused','waiting-user','completed','failed','cancelled')),
           stage TEXT NOT NULL DEFAULT '',
           progress INTEGER NOT NULL DEFAULT 0 CHECK(progress BETWEEN 0 AND 100),
           sequence INTEGER NOT NULL DEFAULT 1,
           created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL,
           error TEXT,
           result_refs TEXT NOT NULL DEFAULT '[]',
           executor_key TEXT,
           checkpoint_ref TEXT,
           parent_task_id TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_host_tasks_updated ON host_tasks(updated_at DESC);
         CREATE INDEX IF NOT EXISTS idx_host_tasks_status ON host_tasks(status);
         CREATE TABLE IF NOT EXISTS plugin_favorites (
           plugin_id TEXT PRIMARY KEY,
           created_at INTEGER NOT NULL,
           FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS plugin_recent_use (
           plugin_id TEXT PRIMARY KEY,
           last_used_at INTEGER NOT NULL,
           use_count INTEGER NOT NULL DEFAULT 1,
           FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS marketplace_catalog_cache (
           source_key TEXT NOT NULL,
           channel TEXT NOT NULL,
           catalog_json TEXT NOT NULL,
           catalog_url TEXT NOT NULL,
           fetched_at INTEGER NOT NULL,
           PRIMARY KEY (source_key, channel)
         );",
    )
}

/// v8: bounded, session-scoped Next streaming storage staging. User rows stay in plugin_storage.
fn migrate_v8(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS next_storage_write_transactions (
           transaction_id TEXT PRIMARY KEY,
           plugin_id TEXT NOT NULL,
           session_hash TEXT NOT NULL,
           writes_json TEXT NOT NULL,
           deletes_json TEXT NOT NULL,
           total_bytes INTEGER NOT NULL CHECK(total_bytes BETWEEN 0 AND 8388608),
           expires_at INTEGER NOT NULL,
           FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS next_storage_write_chunks (
           transaction_id TEXT NOT NULL,
           key TEXT NOT NULL,
           chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
           bytes BLOB NOT NULL CHECK(length(bytes) BETWEEN 1 AND 24576),
           PRIMARY KEY (transaction_id, key, chunk_index),
           FOREIGN KEY (transaction_id) REFERENCES next_storage_write_transactions(transaction_id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS next_storage_read_sessions (
           read_id TEXT PRIMARY KEY,
           plugin_id TEXT NOT NULL,
           session_hash TEXT NOT NULL,
           key TEXT NOT NULL,
           raw_value BLOB NOT NULL,
           expires_at INTEGER NOT NULL,
           FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_next_storage_writes_owner
           ON next_storage_write_transactions(plugin_id, session_hash, expires_at);
         CREATE INDEX IF NOT EXISTS idx_next_storage_reads_owner
           ON next_storage_read_sessions(plugin_id, session_hash, expires_at);",
    )
}

// ---------------------------------------------------------------------------
// legacy plugin storage 迁移（对等 database/pluginStorage.ts）
// ---------------------------------------------------------------------------
fn migrate_legacy_plugin_storage(conn: &Connection) -> rusqlite::Result<()> {
    if let Some(diary_id) = plugin_id_by_name(conn, "diary")? {
        migrate_legacy_for_plugin(conn, &diary_id, "diary")?;
    }
    if let Some(turntable_id) = plugin_id_by_name(conn, "turntable")? {
        migrate_legacy_for_plugin(conn, &turntable_id, "turntable")?;
    }
    Ok(())
}

/// 已知缺口（oracle L9）：TS 侧在 diary/turntable 插件安装/加载时也会补迁 legacy 数据
/// （ensureLegacyPluginStorageMigrated）；Rust 侧仅在 v2 迁移时做一次。对既有库（v2 时已迁移）
/// 无影响；仅影响"迁移后再全新安装 diary/turntable 插件"的库。1.8.2 sidecar 落地时补齐。
fn migrate_legacy_for_plugin(
    conn: &Connection,
    plugin_id: &str,
    plugin_name: &str,
) -> rusqlite::Result<()> {
    let migration = format!("legacy:{}:v1", plugin_name);
    let applied: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM plugin_storage_migrations WHERE plugin_id = ?1 AND migration = ?2",
            params![plugin_id, migration],
            |row| row.get(0),
        )
        .optional()?;
    if applied.is_some() {
        return Ok(());
    }

    if plugin_name == "diary" && table_exists(conn, "diary_entries")? {
        let mut stmt = conn
            .prepare("SELECT entry_date, title, content FROM diary_entries ORDER BY entry_date")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (entry_date, title, content) = row?;
            let value =
                serde_json::json!({ "entry_date": entry_date, "title": title, "content": content });
            insert_migrated_value(conn, plugin_id, &format!("entry:{}", entry_date), &value)?;
        }
    }

    if plugin_name == "turntable" && table_exists(conn, "turntable_items")? {
        let mut stmt =
            conn.prepare("SELECT * FROM turntable_items ORDER BY sort_order ASC, id ASC")?;
        let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
        let rows = stmt.query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, col) in cols.iter().enumerate() {
                map.insert(col.clone(), value_ref_to_json(&row.get_ref(i)?)?);
            }
            Ok(map)
        })?;
        let items: Vec<serde_json::Value> = rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(serde_json::Value::Object)
            .collect();
        insert_migrated_value(conn, plugin_id, "items", &serde_json::Value::Array(items))?;
    }

    conn.execute(
        "INSERT OR IGNORE INTO plugin_storage_migrations (plugin_id, migration) VALUES (?1, ?2)",
        params![plugin_id, migration],
    )?;
    Ok(())
}

fn insert_migrated_value(
    conn: &Connection,
    plugin_id: &str,
    key: &str,
    value: &serde_json::Value,
) -> rusqlite::Result<()> {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT OR IGNORE INTO plugin_storage (plugin_id, key, value) VALUES (?1, ?2, ?3)",
        params![plugin_id, key, serialized],
    )
    .map(|_| ())
}

/// 把 rusqlite ValueRef 转成 serde_json，语义对齐 TS `JSON.stringify`：
/// - 数字保真、布尔/字符串直通、NULL→null
/// - BLOB → `{"type":"Buffer","data":[...]}`（对等 better-sqlite3 读出 Buffer 后
///   JSON.stringify 的产物；base64 字符串会与 TS 版本结构不同，不可用）
fn value_ref_to_json(v: &rusqlite::types::ValueRef) -> rusqlite::Result<serde_json::Value> {
    use rusqlite::types::ValueRef;
    Ok(match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => serde_json::Value::Number((*i).into()),
        ValueRef::Real(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        ValueRef::Text(s) => serde_json::Value::String(String::from_utf8_lossy(s).into_owned()),
        ValueRef::Blob(b) => serde_json::json!({
            "type": "Buffer",
            "data": b.iter().copied().map(u64::from).collect::<Vec<_>>(),
        }),
    })
}

fn plugin_id_by_name(conn: &Connection, name: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT id FROM plugins WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )
    .optional()
}

fn table_exists(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    let found: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

// ---------------------------------------------------------------------------
// 日志保留（对等 LOG_RETENTION_DAYS=30）
// ---------------------------------------------------------------------------

fn cleanup_plugin_logs(db: &Db) -> rusqlite::Result<()> {
    let guard = db.conn.lock().unwrap();
    guard.execute(
        "DELETE FROM plugin_logs WHERE timestamp < datetime('now', 'localtime', '-30 day')",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
fn random_fixture_token() -> String {
    let mut bytes = [0u8; 12];
    getrandom::getrandom(&mut bytes).unwrap();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn uninstall_retains_raw_values_and_reinstall_restores_atomically() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("retention.sqlite");
        let db = Db::open(&path).unwrap();
        db.conn.lock().unwrap().execute("INSERT INTO plugins(id,name,version,display_name,entry_main,entry_renderer,installed_path,config_data) VALUES ('old-id','diary','1.0.0','Diary','dist/main.js','dist/renderer.js','test',?1)", ["invalid legacy JSON 保留"]).unwrap();
        let raw = format!("  {{ \"text\": \"{}\" }}  ", "用户🌏".repeat(300_000));
        db.storage_set("old-id", "entry:2026-10-08", &raw).unwrap();
        db.conn.lock().unwrap().execute("INSERT INTO plugin_storage_migrations(plugin_id,migration) VALUES ('old-id','legacy-once')", []).unwrap();
        let mut replacement = db.plugin_find_by_id("old-id").unwrap().unwrap();
        db.conn.lock().unwrap().execute_batch("CREATE TRIGGER injected_retention_failure BEFORE INSERT ON retained_plugin_storage BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(db.plugin_delete("old-id").is_err());
        assert!(db.plugin_find_by_id("old-id").unwrap().is_some());
        assert_eq!(
            db.storage_get("old-id", "entry:2026-10-08").unwrap(),
            Some(raw.clone())
        );
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER injected_retention_failure")
            .unwrap();
        db.plugin_delete("old-id").unwrap();
        assert!(db.plugin_find_by_id("old-id").unwrap().is_none());
        drop(db);
        let db = Db::open(&path).unwrap();
        replacement.id = "new-id".to_string();
        replacement.config_data = "{}".to_string();
        db.conn.lock().unwrap().execute_batch("CREATE TRIGGER injected_restore_failure BEFORE INSERT ON plugin_storage BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(db.plugin_create(&replacement).is_err());
        assert!(db.plugin_find_by_id("new-id").unwrap().is_none());
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER injected_restore_failure")
            .unwrap();
        db.plugin_create(&replacement).unwrap();
        assert_eq!(
            db.storage_get("new-id", "entry:2026-10-08").unwrap(),
            Some(raw)
        );
        assert_eq!(
            db.plugin_find_by_id("new-id").unwrap().unwrap().config_data,
            "invalid legacy JSON 保留"
        );
        let conn = db.conn.lock().unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM plugin_storage_migrations WHERE plugin_id='new-id' AND migration='legacy-once'", [], |r| r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM retained_plugin_data", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn config_patch_is_owner_scoped_atomic_and_preserves_legacy_values() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(&root.path().join("config.sqlite")).unwrap();
        for owner in ["diary", "turntable"] {
            db.conn.lock().unwrap().execute(
                "INSERT INTO plugins(id,name,version,display_name,entry_main,entry_renderer,installed_path,enabled,config_schema,config_data) VALUES (?1,?1,'1.0.0',?1,'dist/main.js','dist/renderer.js','test',1,?2,?3)",
                params![owner, r#"{"fontSize":{"default":14}}"#, r#"{"legacy":{"text":"用户原文🌏"},"untouched":true}"#],
            ).unwrap();
        }
        let first = db.clone();
        let second = db.clone();
        let a = std::thread::spawn(move || {
            first
                .plugin_config_patch("diary", &serde_json::json!({"fontSize":16}))
                .unwrap()
        });
        let b = std::thread::spawn(move || {
            second
                .plugin_config_patch("diary", &serde_json::json!({"other":23}))
                .unwrap()
        });
        a.join().unwrap();
        b.join().unwrap();
        let saved = db.plugin_config_get("diary").unwrap();
        assert_eq!(saved["fontSize"], 16);
        assert_eq!(saved["other"], 23);
        assert_eq!(saved["legacy"]["text"], "用户原文🌏");
        let other = db.plugin_config_get("turntable").unwrap();
        assert_eq!(other["fontSize"], 14);
        assert!(other.get("other").is_none());
        let before: String = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT config_data FROM plugins WHERE id='diary'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_config BEFORE UPDATE OF config_data ON plugins BEGIN SELECT RAISE(ABORT,'injected config failure'); END;").unwrap();
        assert!(db
            .plugin_config_patch("diary", &serde_json::json!({"fontSize":99}))
            .is_err());
        let after: String = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT config_data FROM plugins WHERE id='diary'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after);
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_config")
            .unwrap();
        assert!(db
            .plugin_config_patch("diary", &serde_json::json!({"constructor":"denied"}))
            .is_err());
        drop(db);
        let reopened = Db::open(&root.path().join("config.sqlite")).unwrap();
        assert_eq!(reopened.plugin_config_get("diary").unwrap(), saved);
    }

    #[test]
    fn malformed_legacy_config_is_retained_and_disabled_config_is_denied() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(&root.path().join("config.sqlite")).unwrap();
        db.conn.lock().unwrap().execute("INSERT INTO plugins(id,name,version,display_name,entry_main,entry_renderer,installed_path,enabled,config_data) VALUES ('diary','diary','1.0.0','diary','main.js','renderer.js','test',1,'{legacy-invalid')",[]).unwrap();
        assert!(db
            .plugin_config_patch("diary", &serde_json::json!({"new":true}))
            .is_err());
        let raw: String = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT config_data FROM plugins WHERE id='diary'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, "{legacy-invalid");
        db.conn
            .lock()
            .unwrap()
            .execute("UPDATE plugins SET enabled=0 WHERE id='diary'", [])
            .unwrap();
        assert_eq!(db.plugin_config_get("diary").unwrap_err(), "SESSION_DENIED");
    }

    use super::*;
    use serde_json::Value;

    fn temp_db(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cruciblebox-db-test-{}", name));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openbox.db");
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn fresh_db_migrates_to_current_schema() {
        let path = temp_db("fresh");
        let db = Db::open(&path).unwrap();
        let status = db.status().unwrap();
        assert_eq!(status.version, MIGRATIONS_COUNT);
        assert_eq!(status.journal_mode.to_lowercase(), "wal");
        assert!(status.foreign_keys);
    }

    #[test]
    #[ignore = "requires a coherent online backup in CRUCIBLEBOX_LEGACY_DB_FIXTURE"]
    fn real_legacy_snapshot_preserves_data_and_recovers_failed_migration() {
        let source = std::path::PathBuf::from(
            std::env::var_os("CRUCIBLEBOX_LEGACY_DB_FIXTURE").expect("legacy backup fixture"),
        );
        let directory = std::env::temp_dir().join(format!(
            "cb-legacy-upgrade-{}-{}",
            std::process::id(),
            random_fixture_token()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("openbox.db");
        std::fs::copy(&source, &target).unwrap();
        let conn = Connection::open(&target).unwrap();
        let old_version = pragma_i64(&conn, "user_version").unwrap();
        assert!(
            (5..7).contains(&old_version),
            "fixture must be a beta1/beta2 backup"
        );
        let queries = [
            "SELECT id,name,version,config_data,installed_path FROM plugins ORDER BY id",
            "SELECT plugin_id,key,value FROM plugin_storage ORDER BY plugin_id,key",
            "SELECT key,value FROM settings ORDER BY key",
        ];
        let capture = |connection: &Connection, sql: &str| -> Vec<Value> {
            let mut statement = connection.prepare(sql).unwrap();
            let columns = statement.column_count();
            statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| value_ref_to_json(&row.get_ref(index)?))
                        .collect::<rusqlite::Result<Vec<_>>>()
                        .map(Value::Array)
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        let before: Vec<_> = queries.iter().map(|sql| capture(&conn, sql)).collect();
        // Inject a real SQL failure in v7 after CREATE TABLE has begun. The
        // migration transaction must roll back both schema and version marker.
        conn.execute_batch("CREATE TABLE host_tasks(id TEXT PRIMARY KEY)")
            .unwrap();
        drop(conn);
        assert!(Db::open(&target).is_err());
        let conn = Connection::open(&target).unwrap();
        assert_eq!(pragma_i64(&conn, "user_version").unwrap(), 6);
        let favorites: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='plugin_favorites'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(favorites, 0, "failed v7 must not leave partial tables");
        conn.execute_batch("DROP TABLE host_tasks").unwrap();
        drop(conn);
        let mut migrated = Vec::new();
        for attempt in 0..2 {
            let db = Db::open(&target).expect("upgrade and repeated startup");
            assert_eq!(db.version().unwrap(), MIGRATIONS_COUNT);
            let conn = db.conn().lock().unwrap();
            assert_eq!(
                conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                    .unwrap(),
                "ok"
            );
            for (index, sql) in queries.iter().enumerate() {
                let after = capture(&conn, sql);
                assert!(
                    before[index].iter().all(|row| after.contains(row)),
                    "legacy rows changed in {sql}"
                );
                if attempt == 0 {
                    migrated.push(after);
                } else {
                    assert_eq!(after, migrated[index], "repeated migration changed data");
                }
            }
            eprintln!("legacy upgrade: v{old_version}->v{}; attempt={attempt}; plugins={}; storage={}; settings={}; integrity=ok", MIGRATIONS_COUNT, before[0].len(), before[1].len(), before[2].len());
        }
        // Rollback uses the coherent old backup, never an old program on v7.
        let restored = directory.join("rollback.db");
        std::fs::copy(&source, &restored).unwrap();
        let conn = Connection::open(&restored).unwrap();
        assert_eq!(pragma_i64(&conn, "user_version").unwrap(), old_version);
        for (index, sql) in queries.iter().enumerate() {
            assert_eq!(capture(&conn, sql), before[index]);
        }
        drop(conn);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn v4_repairs_stale_installed_path_after_data_dir_migration() {
        // Bug G：L3 迁移搬走 %APPDATA%\openbox → cruciblebox，但 DB 里的
        // installed_path 未改写，renderer/后端全部 "failed to read"。
        let path = temp_db("v4-path-repair");
        {
            // 先按当前代码建到 v4，再手动降级模拟"迁移前旧库"
            let db = Db::open(&path).unwrap();
            drop(db);
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 3).unwrap();
            conn.execute_batch(
                r#"
                DELETE FROM plugins;
                INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path)
                  VALUES ('a', 'diary', '1.0.0', 'Diary', 'dist/main.js',
                          'C:\Users\u1\AppData\Roaming\openbox\plugins\diary'),
                         ('b', 'unienv', '1.0.0', 'UniEnv', 'dist/main.js',
                          'C:/Users/u1/AppData/Roaming/openbox/plugins/unienv'),
                         ('c', 'ok', '1.0.0', 'Ok', 'dist/main.js',
                          'C:\fresh\cruciblebox\plugins\ok');
                "#,
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.status().unwrap().version, MIGRATIONS_COUNT);
        let guard = db.conn.lock().unwrap();
        let get = |id: &str| -> String {
            guard
                .query_row(
                    "SELECT installed_path FROM plugins WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert_eq!(
            get("a"),
            r"C:\Users\u1\AppData\Roaming\cruciblebox\plugins\diary"
        );
        assert_eq!(
            get("b"),
            "C:/Users/u1/AppData/Roaming/cruciblebox/plugins/unienv"
        );
        // 未含 openbox 段的路径零改动；'openbox' 出现在文件名中（无分隔符边界）也不误伤
        assert_eq!(get("c"), r"C:\fresh\cruciblebox\plugins\ok");
    }

    #[test]
    fn open_fails_when_parent_dir_missing() {
        // C1 场景：Db::open 依赖调用方先创建父目录（对等 TS getDbPath 的 mkdirSync）。
        // 本测试固化该契约：父目录不存在 → 返回 Err 而非 panic。
        let dir = std::env::temp_dir().join(format!(
            "cruciblebox-db-test-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("data").join("openbox.db");
        assert!(Db::open(&path).is_err());
        // 调用方补建目录后成功（main.rs 的 create_dir_all 路径）
        std::fs::create_dir_all(dir.join("data")).unwrap();
        assert!(Db::open(&path).is_ok());
    }

    #[test]
    fn idempotent_reopen_keeps_current_schema() {
        let path = temp_db("reopen");
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        assert_eq!(db.status().unwrap().version, MIGRATIONS_COUNT);
    }

    #[test]
    fn settings_roundtrip() {
        let path = temp_db("settings");
        let db = Db::open(&path).unwrap();
        {
            let guard = db.conn.lock().unwrap();
            guard
                .execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                    rusqlite::params!["updateChannel", "stable"],
                )
                .unwrap();
        }
        let guard = db.conn.lock().unwrap();
        let v: String = guard
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                ["updateChannel"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "stable");
    }

    #[test]
    fn recovered_diary_copies_beta1_notes_on_install_without_changing_source() {
        let db = Db::open(&temp_db("recovered-diary-notes")).unwrap();
        {
            let guard = db.conn.lock().unwrap();
            guard.execute_batch(
                "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path)
                 VALUES ('productivity-toolkit', 'productivity-toolkit', '0.1.0', '旧综合插件', 'dist/main.js', 'C:/plugins/productivity-toolkit'),
                        ('diary', 'diary', '0.6.0', '日记与笔记', 'dist/main.js', 'C:/plugins/diary');
                 INSERT INTO plugin_storage (plugin_id, key, value)
                 VALUES ('productivity-toolkit', 'notes', '[{\"id\":\"one\",\"title\":\"测试\"}]');"
            ).unwrap();
        }
        db.migrate_consolidated_plugin_data("diary", "diary")
            .unwrap();
        db.migrate_consolidated_plugin_data("diary", "diary")
            .unwrap();
        let guard = db.conn.lock().unwrap();
        let restored: String = guard
            .query_row(
                "SELECT value FROM plugin_storage WHERE plugin_id='diary' AND key='notes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let source: String = guard.query_row("SELECT value FROM plugin_storage WHERE plugin_id='productivity-toolkit' AND key='notes'", [], |row| row.get(0)).unwrap();
        assert_eq!(restored, source);
    }

    #[test]
    fn recovered_clipboard_imports_beta1_history() {
        let db = Db::open(&temp_db("recovered-clipboard-history")).unwrap();
        {
            let guard = db.conn.lock().unwrap();
            guard.execute_batch(
                "INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path)
                 VALUES ('productivity-toolkit', 'productivity-toolkit', '0.1.0', '旧综合插件', 'dist/main.js', 'C:/plugins/productivity-toolkit'),
                        ('clipboard-manager', 'clipboard-manager', '0.4.0', '剪贴板管理', 'dist/main.js', 'C:/plugins/clipboard-manager');
                 INSERT INTO plugin_storage (plugin_id, key, value)
                 VALUES ('productivity-toolkit', 'clips', '[{\"id\":\"one\",\"text\":\"test\",\"createdAt\":123,\"pinned\":true}]');"
            ).unwrap();
        }
        db.migrate_consolidated_plugin_data("clipboard-manager", "clipboard-manager")
            .unwrap();
        let guard = db.conn.lock().unwrap();
        let history: String = guard.query_row("SELECT value FROM plugin_storage WHERE plugin_id='clipboard-manager' AND key='history'", [], |row| row.get(0)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&history).unwrap();
        assert_eq!(parsed[0]["timestamp"], 123);
        assert_eq!(parsed[0]["pinned"], true);
    }

    #[test]
    fn legacy_turntable_migration() {
        let path = temp_db("legacy-turntable");
        let db = Db::open(&path).unwrap();
        {
            let guard = db.conn.lock().unwrap();
            guard
                .execute_batch(
                    r#"
                    CREATE TABLE turntable_items (
                      id INTEGER PRIMARY KEY,
                      name TEXT NOT NULL,
                      sort_order INTEGER DEFAULT 0
                    );
                    INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path)
                      VALUES ('p1', 'turntable', '1.0.0', 'Turntable', 'index.js', 'C:/plugins/turntable');
                    INSERT INTO turntable_items (name, sort_order) VALUES ('a', 1), ('b', 2);
                    "#,
                )
                .unwrap();
            // 模拟旧库已有 user_version=3 但未迁移 legacy（避免重复跑 v1-v3）
            guard.pragma_update(None, "user_version", 3).unwrap();
        }
        // 直接调用 legacy 迁移（模拟 ensureLegacyPluginStorageMigrated 语义）
        migrate_legacy_plugin_storage(&db.conn.lock().unwrap()).unwrap();
        let guard = db.conn.lock().unwrap();
        let stored: String = guard
            .query_row(
                "SELECT value FROM plugin_storage WHERE plugin_id = 'p1' AND key = 'items'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn plugin_logs_filter_combinations() {
        let path = temp_db("plugin-logs-filters");
        let db = Db::open(&path).unwrap();
        {
            let guard = db.conn.lock().unwrap();
            guard
                .execute_batch(
                    r#"
                    INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path)
                      VALUES ('p1', 'p1', '1.0.0', 'P1', 'index.js', 'C:/plugins/p1'),
                             ('p2', 'p2', '1.0.0', 'P2', 'index.js', 'C:/plugins/p2');
                    "#,
                )
                .unwrap();
        }
        db.log_write("p1", "info", "hello").unwrap();
        db.log_write("p1", "error", "boom").unwrap();
        db.log_write("p2", "warn", "careful").unwrap();

        // 无过滤（默认进入日志页时的路径：修复前 LIMIT ?3 参数错位报错）
        let all = db.plugin_logs(None, None, 10).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].plugin_id, "p2"); // id DESC

        // 仅 plugin_id 过滤
        let p1 = db.plugin_logs(Some("p1"), None, 10).unwrap();
        assert_eq!(p1.len(), 2);
        assert!(p1.iter().all(|l| l.plugin_id == "p1"));

        // plugin_id + level 过滤
        let p1_err = db.plugin_logs(Some("p1"), Some("error"), 10).unwrap();
        assert_eq!(p1_err.len(), 1);
        assert_eq!(p1_err[0].level, "error");

        // 仅 level 过滤
        let warns = db.plugin_logs(None, Some("warn"), 10).unwrap();
        assert_eq!(warns.len(), 1);
        assert_eq!(warns[0].plugin_id, "p2");

        // limit 生效
        assert_eq!(db.plugin_logs(None, None, 2).unwrap().len(), 2);
    }
}

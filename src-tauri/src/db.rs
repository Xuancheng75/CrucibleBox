// CrucibleBox DB 层（1.8.1）
// rusqlite (bundled) 对等迁移自 better-sqlite3 引擎（database/index.ts）。
// - 文件格式零迁移：SQLite 3.x 向后兼容，直接打开现有 openbox.db
// - bundled 默认 foreign_keys=ON；仍显式设置以保持可读性
// - journal_mode=WAL + busy_timeout 默认 5000ms（rusqlite 内置）

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// 打开数据库并执行 v3 迁移与日志清理。与 better-sqlite3 引擎语义对等：
    /// 迁移失败则抛错（调用方安全退出）；日志清理失败仅记录（不阻断）。
    /// 注意：调用方需确保父目录已创建（对等 TS getDbPath 的 mkdirSync）。
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // 行为变更记录（oracle L8）：better-sqlite3 引擎从未开启 FK，因此 ON DELETE CASCADE
        // 此前不生效；rusqlite bundled 默认 foreign_keys=ON。对存量孤儿行无回溯影响，
        // 但插件卸载时 cascade 删除 plugin_logs/plugin_storage 从此真正生效（属预期改进）。
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // rusqlite 新连接默认 busy_timeout=5000ms；显式声明保持可读性
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let db = Db {
            conn: Mutex::new(conn),
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
    pub fn conn(&self) -> &Mutex<Connection> {
        &self.conn
    }

    /// 读取 user_version（schema 版本）
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
                "SELECT enabled, permissions, installed_path, entry_main, name
                 FROM plugins WHERE id = ?1",
                [plugin_id],
                |row| {
                    Ok(PluginBackendRecord {
                        enabled: row.get::<_, i64>(0)? == 1,
                        permissions: row.get::<_, String>(1)?,
                        installed_path: row.get::<_, String>(2)?,
                        entry_main: row.get::<_, String>(3)?,
                        name: row.get::<_, String>(4)?,
                    })
                },
            )
            .optional()
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

    /// 新建插件记录（enabled 恒 0；installed_at/updated_at 由 DB 生成）
    pub fn plugin_create(&self, row: &PluginRow) -> Result<(), String> {
        let guard = self.conn.lock().unwrap();
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
            .map(|_| ())
            .map_err(|e| e.to_string())
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

    /// 删除插件记录（级联清理 plugin_logs/plugin_storage）
    pub fn plugin_delete(&self, id: &str) -> Result<(), String> {
        let guard = self.conn.lock().unwrap();
        guard
            .execute("DELETE FROM plugins WHERE id = ?1", [id])
            .map(|_| ())
            .map_err(|e| e.to_string())
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

const MIGRATIONS_COUNT: i64 = 3;

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
mod tests {
    use super::*;

    fn temp_db(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cruciblebox-db-test-{}", name));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("openbox.db");
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn fresh_db_migrates_to_v3() {
        let path = temp_db("fresh");
        let db = Db::open(&path).unwrap();
        let status = db.status().unwrap();
        assert_eq!(status.version, 3);
        assert_eq!(status.journal_mode.to_lowercase(), "wal");
        assert!(status.foreign_keys);
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
    fn idempotent_reopen_keeps_v3() {
        let path = temp_db("reopen");
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        assert_eq!(db.status().unwrap().version, 3);
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

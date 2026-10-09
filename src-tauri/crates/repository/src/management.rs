//! Typed plugin metadata, tags and marketplace repository operations.
use crate::Db;
use rusqlite::OptionalExtension;
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMetaDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub icon: String,
    pub entry_main: String,
    pub entry_renderer: String,
    pub permissions: Vec<String>,
    pub config_schema: serde_json::Value,
    pub config_data: serde_json::Value,
    pub enabled: bool,
    pub installed_path: String,
    pub installed_at: String,
    pub updated_at: String,
    pub sort_order: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPluginTagDto {
    pub id: i64,
    pub name: String,
    pub plugin_ids: Vec<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSourceDto {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub enabled: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketplaceOriginDto {
    pub plugin_id: String,
    pub source_id: i64,
    pub source_url: String,
}
fn json_or_empty(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({}))
}

fn row_to_meta(row: &rusqlite::Row) -> rusqlite::Result<PluginMetaDto> {
    Ok(PluginMetaDto {
        id: row.get("id")?,
        name: row.get("name")?,
        version: row.get("version")?,
        display_name: row.get("display_name")?,
        description: row.get("description").unwrap_or_default(),
        author: row.get("author").unwrap_or_default(),
        icon: row.get("icon").unwrap_or_default(),
        entry_main: row.get("entry_main")?,
        entry_renderer: row.get("entry_renderer").unwrap_or_default(),
        permissions: serde_json::from_str(&row.get::<_, String>("permissions").unwrap_or_default())
            .unwrap_or_default(),
        config_schema: json_or_empty(&row.get::<_, String>("config_schema").unwrap_or_default()),
        config_data: json_or_empty(&row.get::<_, String>("config_data").unwrap_or_default()),
        enabled: row.get::<_, i64>("enabled").unwrap_or(1) == 1,
        installed_path: row.get("installed_path")?,
        installed_at: row.get("installed_at").unwrap_or_default(),
        updated_at: row.get("updated_at").unwrap_or_default(),
        sort_order: row.get("sort_order").unwrap_or(0),
    })
}

const PLUGIN_COLUMNS: &str = "id, name, version, display_name, description, author, icon, \
     entry_main, entry_renderer, permissions, config_schema, config_data, enabled, \
     installed_path, installed_at, updated_at, sort_order";

impl Db {
    pub fn marketplace_cache_read(
        &self,
        source: &str,
        channel: &str,
    ) -> Result<Option<(String, String, i64)>, String> {
        self.conn.lock().map_err(|e|e.to_string())?.query_row("SELECT catalog_json,catalog_url,fetched_at FROM marketplace_catalog_cache WHERE source_key=?1 AND channel=?2",rusqlite::params![source,channel],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(|e|e.to_string())
    }
    pub fn marketplace_cache_write(
        &self,
        source: &str,
        channel: &str,
        json: &str,
        url: &str,
        fetched_at: u64,
    ) -> Result<(), String> {
        let fetched_at = i64::try_from(fetched_at).map_err(|_| "invalid catalog timestamp")?;
        self.conn.lock().map_err(|e|e.to_string())?.execute("INSERT INTO marketplace_catalog_cache(source_key,channel,catalog_json,catalog_url,fetched_at) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(source_key,channel) DO UPDATE SET catalog_json=excluded.catalog_json,catalog_url=excluded.catalog_url,fetched_at=excluded.fetched_at",rusqlite::params![source,channel,json,url,fetched_at]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn marketplace_origin_set(
        &self,
        plugin: &str,
        source_id: i64,
        url: &str,
    ) -> Result<(), String> {
        self.conn.lock().map_err(|e|e.to_string())?.execute("INSERT INTO plugin_marketplace_origins(plugin_id,source_id,source_url) VALUES (?1,?2,?3) ON CONFLICT(plugin_id) DO UPDATE SET source_id=excluded.source_id,source_url=excluded.source_url",rusqlite::params![plugin,source_id,url]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn metadata_list(&self) -> Result<Vec<PluginMetaDto>, String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        // 对等 plugin.repository.ts 排序：sort_order ASC, installed_at DESC
        let mut stmt = guard
            .prepare(&format!(
                "SELECT {} FROM plugins ORDER BY sort_order ASC, installed_at DESC, id ASC",
                PLUGIN_COLUMNS
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_meta).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }
    pub fn metadata_get(&self, id: String) -> Result<Option<PluginMetaDto>, String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        let sql = format!("SELECT {} FROM plugins WHERE id = ?1", PLUGIN_COLUMNS);
        let mut stmt = guard.prepare(&sql).map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map([id], row_to_meta)
            .map_err(|e| e.to_string())?;
        let first = rows.next().transpose().map_err(|e| e.to_string())?;
        Ok(first)
    }
    pub fn tags_list(&self) -> Result<Vec<UserPluginTagDto>, String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        let mut stmt = guard
            .prepare("SELECT id, name FROM user_plugin_tags ORDER BY name COLLATE NOCASE")
            .map_err(|e| e.to_string())?;
        let tags = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let mut links = guard
            .prepare(
                "SELECT plugin_id FROM user_plugin_tag_links WHERE tag_id = ?1 ORDER BY plugin_id",
            )
            .map_err(|e| e.to_string())?;
        tags.into_iter()
            .map(|(id, name)| {
                let plugin_ids = links
                    .query_map([id], |row| row.get::<_, String>(0))
                    .map_err(|e| e.to_string())?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?;
                Ok(UserPluginTagDto {
                    id,
                    name,
                    plugin_ids,
                })
            })
            .collect()
    }
    pub fn tag_create(&self, name: String) -> Result<i64, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 32 {
            return Err("标签名称须为 1—32 个字符".into());
        }
        let db = self;
        let guard = db.conn().lock().unwrap();
        guard
            .execute("INSERT INTO user_plugin_tags(name) VALUES (?1)", [name])
            .map_err(|e| e.to_string())?;
        Ok(guard.last_insert_rowid())
    }
    pub fn tag_rename(&self, id: i64, name: String) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 32 {
            return Err("标签名称须为 1—32 个字符".into());
        }
        let db = self;
        let guard = db.conn().lock().unwrap();
        guard
            .execute(
                "UPDATE user_plugin_tags SET name = ?1 WHERE id = ?2",
                rusqlite::params![name, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn tag_delete(&self, id: i64) -> Result<(), String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        guard
            .execute("DELETE FROM user_plugin_tags WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn tags_assign(&self, plugin_ids: Vec<String>, tag_ids: Vec<i64>) -> Result<(), String> {
        let db = self;
        let mut guard = db.conn().lock().unwrap();
        let tx = guard.transaction().map_err(|e| e.to_string())?;
        for plugin_id in &plugin_ids {
            tx.execute(
                "DELETE FROM user_plugin_tag_links WHERE plugin_id = ?1",
                [plugin_id],
            )
            .map_err(|e| e.to_string())?;
            for tag_id in &tag_ids {
                tx.execute(
                    "INSERT INTO user_plugin_tag_links(plugin_id, tag_id) VALUES (?1, ?2)",
                    rusqlite::params![plugin_id, tag_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn marketplace_origins(&self) -> Result<Vec<PluginMarketplaceOriginDto>, String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        let mut stmt = guard
            .prepare("SELECT plugin_id, source_id, source_url FROM plugin_marketplace_origins")
            .map_err(|error| error.to_string())?;
        let origins = stmt
            .query_map([], |row| {
                Ok(PluginMarketplaceOriginDto {
                    plugin_id: row.get(0)?,
                    source_id: row.get(1)?,
                    source_url: row.get(2)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        Ok(origins)
    }
    pub fn marketplace_sources(&self) -> Result<Vec<MarketplaceSourceDto>, String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        let mut stmt = guard
            .prepare("SELECT id, name, url, enabled FROM marketplace_sources ORDER BY id")
            .map_err(|error| error.to_string())?;
        let sources = stmt
            .query_map([], |row| {
                Ok(MarketplaceSourceDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    url: row.get(2)?,
                    enabled: row.get::<_, i64>(3)? != 0,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        Ok(sources)
    }
    pub fn marketplace_source_add(&self, name: String, url: String) -> Result<i64, String> {
        let name = name.trim();
        let url = url.trim();
        if name.is_empty() || !url.starts_with("https://") {
            return Err("请填写目录名称和 HTTPS 地址".into());
        }
        let db = self;
        let guard = db.conn().lock().unwrap();
        guard
            .execute(
                "INSERT INTO marketplace_sources(name, url) VALUES (?1, ?2)",
                rusqlite::params![name, url],
            )
            .map_err(|error| error.to_string())?;
        Ok(guard.last_insert_rowid())
    }
    pub fn marketplace_source_enable(&self, id: i64, enabled: bool) -> Result<(), String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        guard
            .execute(
                "UPDATE marketplace_sources SET enabled = ?1 WHERE id = ?2",
                rusqlite::params![enabled, id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    pub fn marketplace_source_delete(&self, id: i64) -> Result<(), String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        guard
            .execute("DELETE FROM marketplace_sources WHERE id = ?1", [id])
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    pub fn marketplace_source(&self, id: i64) -> Result<MarketplaceSourceDto, String> {
        let db = self;
        let guard = db.conn().lock().unwrap();
        guard
            .query_row(
                "SELECT id, name, url, enabled FROM marketplace_sources WHERE id = ?1",
                [id],
                |row| {
                    Ok(MarketplaceSourceDto {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        url: row.get(2)?,
                        enabled: row.get::<_, i64>(3)? != 0,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "插件目录不存在".into())
    }
    pub fn enabled_plugin_installations(&self) -> Result<Vec<(String, String)>, String> {
        let guard = self.conn().lock().unwrap();
        let mut stmt = guard
            .prepare("SELECT id,installed_path FROM plugins WHERE enabled=1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())
    }
}

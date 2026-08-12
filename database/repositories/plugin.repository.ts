import { queryAll, queryOne, execute, getDatabase } from '../index'
import type { EngineDb } from '../index'
import type { PluginMeta, PluginConfig } from '@shared/types/plugin.types'

interface PluginRow {
  id: string
  name: string
  version: string
  display_name: string
  description: string
  author: string
  icon: string
  entry_main: string
  entry_renderer: string
  permissions: string
  config_schema: string
  config_data: string
  enabled: number
  installed_path: string
  installed_at: string
  updated_at: string
  sort_order: number
}

function parseJsonSafe<T>(json: string | null | undefined, fallback: T): T {
  if (json === null || json === undefined) return fallback
  try {
    const result = JSON.parse(json)
    return (result !== null ? result : fallback) as T
  } catch {
    return fallback
  }
}

function rowToMeta(row: PluginRow): PluginMeta {
  return {
    id: row.id,
    name: row.name,
    version: row.version,
    displayName: row.display_name,
    description: row.description,
    author: row.author,
    icon: row.icon || undefined,
    entryMain: row.entry_main,
    entryRenderer: row.entry_renderer,
    permissions: parseJsonSafe(row.permissions, []),
    configSchema: parseJsonSafe(row.config_schema, {}),
    configData: parseJsonSafe(row.config_data, {}),
    enabled: row.enabled === 1,
    installedAt: row.installed_at,
    updatedAt: row.updated_at,
    sortOrder: row.sort_order
  }
}

export const PluginRepository = {
  findAll(): PluginMeta[] {
    const rows = queryAll<PluginRow>(
      'SELECT * FROM plugins ORDER BY sort_order ASC, installed_at DESC'
    )
    return rows.map(rowToMeta)
  },

  findById(id: string): PluginMeta | null {
    const row = queryOne<PluginRow>('SELECT * FROM plugins WHERE id = ?', [id])
    return row ? rowToMeta(row) : null
  },

  findByName(name: string): PluginMeta | null {
    const row = queryOne<PluginRow>('SELECT * FROM plugins WHERE name = ?', [name])
    return row ? rowToMeta(row) : null
  },

  create(record: {
    id: string
    name: string
    version: string
    display_name: string
    description: string
    author: string
    icon: string
    entry_main: string
    entry_renderer: string
    permissions: string
    config_schema: string
    config_data: string
    enabled: number
    installed_path: string
  }): void {
    // New plugins always append to the end of the user's ordering. Computing the
    // next sort_order inside the INSERT keeps the read-and-write atomic.
    execute(
      `INSERT INTO plugins (id, name, version, display_name, description, author, icon,
        entry_main, entry_renderer, permissions, config_schema, config_data, enabled, installed_path,
        sort_order)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
        (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM plugins))`,
      [
        record.id, record.name, record.version, record.display_name,
        record.description, record.author, record.icon, record.entry_main,
        record.entry_renderer, record.permissions, record.config_schema,
        record.config_data, record.enabled, record.installed_path
      ]
    )
  },

  delete(id: string): void {
    execute('DELETE FROM plugins WHERE id = ?', [id])
  },

  updateEnabled(id: string, enabled: boolean): void {
    execute(
      "UPDATE plugins SET enabled = ?, updated_at = datetime('now', 'localtime') WHERE id = ?",
      [enabled ? 1 : 0, id]
    )
  },

  updateConfig(id: string, config: PluginConfig): void {
    execute(
      "UPDATE plugins SET config_data = ?, updated_at = datetime('now', 'localtime') WHERE id = ?",
      [JSON.stringify(config), id]
    )
  },

  updatePluginVersion(id: string, fields: {
    version: string
    display_name: string
    description: string
    author: string
    icon: string
    entry_main: string
    entry_renderer: string
    permissions: string
    config_schema: string
    installed_path: string
  }): void {
    execute(
      `UPDATE plugins SET
        version = ?, display_name = ?, description = ?, author = ?, icon = ?,
        entry_main = ?, entry_renderer = ?, permissions = ?, config_schema = ?,
        installed_path = ?, updated_at = datetime('now', 'localtime')
      WHERE id = ?`,
      [
        fields.version, fields.display_name, fields.description, fields.author, fields.icon,
        fields.entry_main, fields.entry_renderer, fields.permissions, fields.config_schema,
        fields.installed_path, id
      ]
    )
  },

  getConfig(id: string): PluginConfig {
    const row = queryOne<{ config_data: string }>(
      'SELECT config_data FROM plugins WHERE id = ?',
      [id]
    )
    if (!row) return {}
    try {
      return JSON.parse(row.config_data)
    } catch {
      return {}
    }
  },

  getEnabledPlugins(): PluginMeta[] {
    const rows = queryAll<PluginRow>(
      'SELECT * FROM plugins WHERE enabled = 1 ORDER BY sort_order ASC, installed_at DESC'
    )
    return rows.map(rowToMeta)
  },

  /**
   * Transactionally re-orders all installed plugins. `orderedIds` must be a
   * complete permutation of every installed plugin id — duplicates, missing ids
   * and unknown ids are rejected before anything is written, and any failure
   * while persisting rolls the transaction back. Returns the full plugin list
   * in the new order.
   */
  reorder(orderedIds: string[], db: EngineDb = getDatabase()): PluginMeta[] {
    const existing = db.all<{ id: string }>('SELECT id FROM plugins')
    const existingIds = new Set(existing.map((row) => row.id))
    if (!Array.isArray(orderedIds) || orderedIds.length !== existingIds.size) {
      throw new Error('排序列表必须包含全部已安装插件')
    }
    const seen = new Set<string>()
    for (const id of orderedIds) {
      if (typeof id !== 'string' || id.length === 0 || seen.has(id) || !existingIds.has(id)) {
        throw new Error('排序列表包含重复、缺失或未知的插件 ID')
      }
      seen.add(id)
    }
    db.exec('BEGIN IMMEDIATE')
    try {
      orderedIds.forEach((id, index) => {
        db.run('UPDATE plugins SET sort_order = ? WHERE id = ?', [index + 1, id])
      })
      db.exec('COMMIT')
    } catch (error) {
      try {
        db.exec('ROLLBACK')
      } catch {
        void 0
      }
      throw error
    }
    return this.findAll()
  }
}

import type { EngineDb } from './index'

const MAX_KEY_LENGTH = 256
const MAX_VALUE_BYTES = 1024 * 1024

function containsControlCharacter(value: string): boolean {
  return Array.from(value).some((character) => {
    const code = character.charCodeAt(0)
    return code <= 31 || code === 127
  })
}

export interface PluginStorageEntry<T = unknown> {
  key: string
  value: T
}

export type PluginStorageMutation =
  { type: 'set'; key: string; value: unknown } | { type: 'delete'; key: string }

function assertPluginId(pluginId: string): void {
  if (!pluginId || pluginId.length > 128 || containsControlCharacter(pluginId)) {
    throw new Error('Invalid plugin storage namespace')
  }
}

export function assertPluginStorageKey(key: string): void {
  if (!key || key.length > MAX_KEY_LENGTH || containsControlCharacter(key)) {
    throw new Error('Invalid plugin storage key')
  }
}

export function assertPluginStoragePrefix(prefix: string): void {
  if (prefix.length > MAX_KEY_LENGTH || containsControlCharacter(prefix)) {
    throw new Error('Invalid plugin storage prefix')
  }
}

function normalizeJsonValue(value: unknown, seen = new WeakSet<object>()): unknown {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('Plugin storage values must be finite JSON values')
    return value
  }
  if (typeof value !== 'object') throw new Error('Plugin storage values must be JSON values')
  if (seen.has(value)) throw new Error('Plugin storage values must not contain cycles')
  seen.add(value)
  try {
    if (Array.isArray(value)) return value.map((item) => normalizeJsonValue(item, seen))
    const prototype = Object.getPrototypeOf(value)
    if (prototype !== Object.prototype && prototype !== null) {
      throw new Error('Plugin storage values must contain only plain objects')
    }
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, normalizeJsonValue(item, seen)])
    )
  } finally {
    seen.delete(value)
  }
}

export function serializePluginStorageValue(value: unknown): string {
  const serialized = JSON.stringify(normalizeJsonValue(value))
  if (Buffer.byteLength(serialized, 'utf8') > MAX_VALUE_BYTES) {
    throw new Error('Plugin storage value exceeds 1 MiB')
  }
  return serialized
}

function parseStoredValue(value: string): unknown {
  try {
    return JSON.parse(value) as unknown
  } catch {
    throw new Error('Plugin storage contains invalid JSON')
  }
}

export function getPluginStorageValue<T = unknown>(
  database: EngineDb,
  pluginId: string,
  key: string
): T | null {
  assertPluginId(pluginId)
  assertPluginStorageKey(key)
  const row = database.get<{ value: string }>(
    'SELECT value FROM plugin_storage WHERE plugin_id = ? AND key = ?',
    [pluginId, key]
  )
  return row ? (parseStoredValue(row.value) as T) : null
}

export function setPluginStorageValue(
  database: EngineDb,
  pluginId: string,
  key: string,
  value: unknown
): void {
  assertPluginId(pluginId)
  assertPluginStorageKey(key)
  database.run(
    `INSERT INTO plugin_storage (plugin_id, key, value, updated_at)
     VALUES (?, ?, ?, datetime('now', 'localtime'))
     ON CONFLICT(plugin_id, key) DO UPDATE SET
       value = excluded.value,
       updated_at = excluded.updated_at`,
    [pluginId, key, serializePluginStorageValue(value)]
  )
}

export function deletePluginStorageValue(database: EngineDb, pluginId: string, key: string): void {
  assertPluginId(pluginId)
  assertPluginStorageKey(key)
  database.run('DELETE FROM plugin_storage WHERE plugin_id = ? AND key = ?', [pluginId, key])
}

export function listPluginStorageValues<T = unknown>(
  database: EngineDb,
  pluginId: string,
  prefix = ''
): PluginStorageEntry<T>[] {
  assertPluginId(pluginId)
  assertPluginStoragePrefix(prefix)
  const rows = prefix
    ? database.all<{ key: string; value: string }>(
        `SELECT key, value FROM plugin_storage
         WHERE plugin_id = ? AND substr(key, 1, ?) = ? ORDER BY key`,
        [pluginId, prefix.length, prefix]
      )
    : database.all<{ key: string; value: string }>(
        'SELECT key, value FROM plugin_storage WHERE plugin_id = ? ORDER BY key',
        [pluginId]
      )
  return rows.map((row) => ({ key: row.key, value: parseStoredValue(row.value) as T }))
}

export function applyPluginStorageBatch(
  database: EngineDb,
  pluginId: string,
  mutations: PluginStorageMutation[]
): void {
  assertPluginId(pluginId)
  if (mutations.length < 1 || mutations.length > 64) {
    throw new Error('Plugin storage batch must contain between 1 and 64 mutations')
  }
  const prepared = mutations.map((mutation) => {
    assertPluginStorageKey(mutation.key)
    return mutation.type === 'set'
      ? { ...mutation, serialized: serializePluginStorageValue(mutation.value) }
      : mutation
  })

  database.exec('BEGIN IMMEDIATE')
  try {
    for (const mutation of prepared) {
      if (mutation.type === 'set') {
        database.run(
          `INSERT INTO plugin_storage (plugin_id, key, value, updated_at)
           VALUES (?, ?, ?, datetime('now', 'localtime'))
           ON CONFLICT(plugin_id, key) DO UPDATE SET
             value = excluded.value,
             updated_at = excluded.updated_at`,
          [pluginId, mutation.key, mutation.serialized]
        )
      } else {
        database.run('DELETE FROM plugin_storage WHERE plugin_id = ? AND key = ?', [
          pluginId,
          mutation.key
        ])
      }
    }
    database.exec('COMMIT')
  } catch (error) {
    try {
      database.exec('ROLLBACK')
    } catch {
      void 0
    }
    throw error
  }
}

function tableExists(database: EngineDb, table: string): boolean {
  return Boolean(
    database.get('SELECT 1 FROM sqlite_master WHERE type = ? AND name = ?', ['table', table])
  )
}

function pluginIdByName(database: EngineDb, name: string): string | null {
  return database.get<{ id: string }>('SELECT id FROM plugins WHERE name = ?', [name])?.id ?? null
}

function insertMigratedValue(
  database: EngineDb,
  pluginId: string,
  key: string,
  value: unknown
): void {
  database.run(
    `INSERT OR IGNORE INTO plugin_storage (plugin_id, key, value)
     VALUES (?, ?, ?)`,
    [pluginId, key, serializePluginStorageValue(value)]
  )
}

function hasMigration(database: EngineDb, pluginId: string, migration: string): boolean {
  return Boolean(
    database.get('SELECT 1 FROM plugin_storage_migrations WHERE plugin_id = ? AND migration = ?', [
      pluginId,
      migration
    ])
  )
}

function markMigration(database: EngineDb, pluginId: string, migration: string): void {
  database.run(
    'INSERT OR IGNORE INTO plugin_storage_migrations (plugin_id, migration) VALUES (?, ?)',
    [pluginId, migration]
  )
}

function migrateLegacyPluginStorageForPlugin(
  database: EngineDb,
  pluginId: string,
  pluginName: string
): void {
  const migration = `legacy:${pluginName}:v1`
  if (hasMigration(database, pluginId, migration)) return

  if (pluginName === 'diary' && tableExists(database, 'diary_entries')) {
    const entries = database.all<{ entry_date: string; title: string; content: string }>(
      'SELECT entry_date, title, content FROM diary_entries ORDER BY entry_date'
    )
    for (const entry of entries) {
      insertMigratedValue(database, pluginId, `entry:${entry.entry_date}`, entry)
    }
  }

  if (pluginName === 'turntable' && tableExists(database, 'turntable_items')) {
    const items = database.all<Record<string, unknown>>(
      'SELECT * FROM turntable_items ORDER BY sort_order ASC, id ASC'
    )
    insertMigratedValue(database, pluginId, 'items', items)
  }

  markMigration(database, pluginId, migration)
}

export function migrateLegacyPluginStorage(database: EngineDb): void {
  const diaryPluginId = pluginIdByName(database, 'diary')
  if (diaryPluginId) migrateLegacyPluginStorageForPlugin(database, diaryPluginId, 'diary')

  const turntablePluginId = pluginIdByName(database, 'turntable')
  if (turntablePluginId) {
    migrateLegacyPluginStorageForPlugin(database, turntablePluginId, 'turntable')
  }
}

export function ensureLegacyPluginStorageMigrated(
  database: EngineDb,
  pluginId: string,
  pluginName: string
): void {
  if (pluginName !== 'diary' && pluginName !== 'turntable') return
  database.exec('BEGIN IMMEDIATE')
  try {
    migrateLegacyPluginStorageForPlugin(database, pluginId, pluginName)
    database.exec('COMMIT')
  } catch (error) {
    try {
      database.exec('ROLLBACK')
    } catch {
      void 0
    }
    throw error
  }
}

// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import Database from 'better-sqlite3'
import { join } from 'path'
import { existsSync, copyFileSync, mkdirSync } from 'fs'
import { app } from 'electron'
import { migrateLegacyPluginStorage } from './pluginStorage'

export interface EngineDb {
  close(): void
  version: number
  exec(sql: string): void
  run(sql: string, params?: unknown[]): void
  all<T = Record<string, unknown>>(sql: string, params?: unknown[]): T[]
  get<T = Record<string, unknown>>(sql: string, params?: unknown[]): T | undefined
}

let db: EngineDb | null = null

const LOG_RETENTION_DAYS = 30

export function getDbPath(): string {
  const userDataPath = app.getPath('userData')
  const dbDir = join(userDataPath, 'data')
  if (!existsSync(dbDir)) {
    mkdirSync(dbDir, { recursive: true })
  }
  return join(dbDir, 'openbox.db')
}

type BetterSqliteDb = InstanceType<typeof Database>

class BetterEngine implements EngineDb {
  private database: BetterSqliteDb

  constructor(path: string) {
    this.database = new Database(path)
    this.database.pragma('journal_mode = WAL')
  }

  get version(): number {
    return this.database.pragma('user_version', { simple: true }) as number
  }

  set version(value: number) {
    this.database.pragma(`user_version = ${value}`)
  }

  exec(sql: string): void {
    this.database.exec(sql)
  }

  run(sql: string, params?: unknown[]): void {
    this.database.prepare(sql).run(...(params ?? []))
  }

  all<T = Record<string, unknown>>(sql: string, params?: unknown[]): T[] {
    return this.database.prepare(sql).all(...(params ?? [])) as unknown as T[]
  }

  get<T = Record<string, unknown>>(sql: string, params?: unknown[]): T | undefined {
    return this.database.prepare(sql).get(...(params ?? [])) as T | undefined
  }

  close(): void {
    this.database.close()
  }
}

export async function initDatabase(dbPath?: string): Promise<EngineDb> {
  if (db) {
    return db
  }

  const path = dbPath || getDbPath()
  // Legacy sql.js-era databases are byte-identical SQLite files; keep a one-time
  // backup before the better-sqlite3 engine takes over so the original is never
  // lost if a migration ever needs forensic recovery.
  const backupPath = `${path}.bak-sqljs`
  if (existsSync(path) && !existsSync(backupPath)) {
    try {
      copyFileSync(path, backupPath)
      console.log(`[DB] sql.js 时代数据库已备份: ${backupPath}`)
    } catch (err) {
      console.error('[DB] create sqljs backup failed:', err)
    }
  }
  try {
    db = new BetterEngine(path)
    console.log('[DB] engine: better-sqlite3 (WAL)')
  } catch (err) {
    console.error('[DB] better-sqlite3 init failed:', err)
    throw new Error('better-sqlite3 initialization failed', { cause: err })
  }

  try {
    runMigrations(db)
    cleanupPluginLogs(db)
  } catch (err) {
    console.error('[DB] migrations failed:', err)
    const failedDatabase = db
    db = null
    try {
      failedDatabase.close()
    } catch (closeError) {
      console.error('[DB] failed to close after migration error:', closeError)
    }
    throw new Error('database migration failed', { cause: err })
  }

  return db
}

type Migration = (database: EngineDb) => void

const MIGRATIONS: Migration[] = [
  (database) => {
    database.run(`
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
      )
    `)
    database.run(`
      CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      )
    `)
    database.run(`
      CREATE TABLE IF NOT EXISTS plugin_logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        plugin_id TEXT NOT NULL,
        level TEXT NOT NULL DEFAULT 'info',
        message TEXT NOT NULL,
        timestamp DATETIME DEFAULT (datetime('now', 'localtime')),
        FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
      )
    `)
    database.run('CREATE INDEX IF NOT EXISTS idx_plugin_logs_plugin_id ON plugin_logs(plugin_id)')
    database.run('CREATE INDEX IF NOT EXISTS idx_plugin_logs_timestamp ON plugin_logs(timestamp)')
  },
  (database) => {
    database.run(`
      CREATE TABLE IF NOT EXISTS plugin_storage (
        plugin_id TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        updated_at DATETIME DEFAULT (datetime('now', 'localtime')),
        PRIMARY KEY (plugin_id, key),
        FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
      )
    `)
    database.run(`
      CREATE TABLE IF NOT EXISTS plugin_storage_migrations (
        plugin_id TEXT NOT NULL,
        migration TEXT NOT NULL,
        applied_at DATETIME DEFAULT (datetime('now', 'localtime')),
        PRIMARY KEY (plugin_id, migration),
        FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
      )
    `)
    migrateLegacyPluginStorage(database)
  },
  (database) => {
    const columns = database.all<{ name: string }>('PRAGMA table_info(plugins)')
    const columnNames = new Set(columns.map((column) => column.name))
    if (!columnNames.has('sort_order')) {
      database.run('ALTER TABLE plugins ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0')
    }
    // Stable backfill: preserve the pre-existing order (installed_at DESC, id ASC)
    // by assigning ascending sort_order in that same sequence. Reordering the list
    // after migration therefore starts from the exact order users already see.
    // Legacy tables that predate installed_at fall back to a deterministic id order.
    const orderBy = columnNames.has('installed_at')
      ? 'ORDER BY installed_at DESC, id ASC'
      : 'ORDER BY id ASC'
    const rows = database.all<{ id: string }>(`SELECT id FROM plugins ${orderBy}`)
    rows.forEach((row, index) => {
      database.run('UPDATE plugins SET sort_order = ? WHERE id = ?', [index + 1, row.id])
    })
  }
]

export function runMigrations(database: EngineDb): void {
  let version = 0
  try {
    version = database.version
  } catch {
    // ignore
  }
  for (let v = version; v < MIGRATIONS.length; v++) {
    database.exec('BEGIN IMMEDIATE')
    try {
      MIGRATIONS[v](database)
      database.version = v + 1
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
}

function cleanupPluginLogs(database: EngineDb): void {
  try {
    database.run("DELETE FROM plugin_logs WHERE timestamp < datetime('now', 'localtime', ?)", [
      `-${LOG_RETENTION_DAYS} day`
    ])
  } catch (err) {
    console.error('Failed to cleanup plugin logs:', err)
  }
}

export function getDatabase(): EngineDb {
  if (!db) {
    throw new Error('数据库未初始化')
  }
  return db
}

/** 仅测试使用：注入测试引擎作为全局数据库（生产路径永不调用）。 */
export function setDatabaseForTesting(engine: EngineDb): void {
  db = engine
}

export function closeDatabase(): void {
  if (db) {
    db.close()
    db = null
  }
}

export function queryAll<T = Record<string, unknown>>(sql: string, params?: unknown[]): T[] {
  return getDatabase().all<T>(sql, params)
}

export function queryOne<T = Record<string, unknown>>(sql: string, params?: unknown[]): T | null {
  return getDatabase().get<T>(sql, params) ?? null
}

export function execute(sql: string, params?: unknown[]): void {
  getDatabase().run(sql, params)
}

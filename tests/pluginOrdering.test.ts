// The test tsconfig narrows its include set, so pull in the host's local sql.js declaration.
// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="../shared/types/sql.js.d.ts" />

import initSqlJs, { type Database as SqlJsDatabase } from 'sql.js'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { EngineDb } from '../database/index'
import { closeDatabase, getDatabase, initDatabase, runMigrations } from '../database/index'
import { PluginRepository } from '../database/repositories/plugin.repository'

vi.mock('electron', () => ({
  app: {
    getAppPath: vi.fn(() => '.'),
    getPath: vi.fn(() => '.')
  }
}))

class MemoryEngine implements EngineDb {
  /** Optional hook to inject write failures for rollback tests. */
  onRun?: (sql: string, params?: unknown[]) => void

  constructor(private readonly database: SqlJsDatabase) {}

  get version(): number {
    return Number(this.database.exec('PRAGMA user_version')[0]?.values[0]?.[0] ?? 0)
  }

  set version(value: number) {
    this.database.run(`PRAGMA user_version = ${value}`)
  }

  exec(sql: string): void {
    this.database.run(sql)
  }

  run(sql: string, params?: unknown[]): void {
    this.onRun?.(sql, params)
    this.database.run(sql, params)
  }

  all<T = Record<string, unknown>>(sql: string, params?: unknown[]): T[] {
    const statement = this.database.prepare(sql)
    const rows: T[] = []
    if (params) statement.bind(params)
    while (statement.step()) rows.push(statement.getAsObject() as T)
    statement.free()
    return rows
  }

  get<T = Record<string, unknown>>(sql: string, params?: unknown[]): T | undefined {
    return this.all<T>(sql, params)[0]
  }

  close(): void {
    this.database.close()
  }

  exportBytes(): Uint8Array {
    return this.database.export()
  }
}

function createPlugin(id: string, name: string, enabled = 0): void {
  PluginRepository.create({
    id,
    name,
    version: '1.0.0',
    display_name: name,
    description: '',
    author: 'ordering tests',
    icon: '',
    entry_main: 'dist/main.js',
    entry_renderer: 'dist/renderer.js',
    permissions: '[]',
    config_schema: '{}',
    config_data: '{}',
    enabled,
    installed_path: `/plugins/${name}`
  })
}

function orderedRows(): { id: string; sort_order: number }[] {
  return getDatabase().all<{ id: string; sort_order: number }>(
    'SELECT id, sort_order FROM plugins ORDER BY sort_order ASC, installed_at DESC'
  )
}

describe('plugin ordering', () => {
  let directory: string
  let dbPath: string

  beforeEach(() => {
    closeDatabase()
    directory = mkdtempSync(join(tmpdir(), 'openbox-plugin-order-'))
    dbPath = join(directory, 'openbox.db')
  })

  afterEach(() => {
    closeDatabase()
    rmSync(directory, { recursive: true, force: true })
  })

  it('backfills sort_order during the v2→v3 migration without changing the existing order', async () => {
    const SQL = await initSqlJs()
    const legacy = new SQL.Database()
    legacy.run(`
      CREATE TABLE plugins (
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
    // Deliberately scrambled install order to prove deterministic backfill.
    const rows: Array<[string, string, string]> = [
      ['p-a', 'plugin-a', '2026-08-01 10:00:00'],
      ['p-b', 'plugin-b', '2026-08-03 10:00:00'],
      ['p-c', 'plugin-c', '2026-08-02 10:00:00'],
      ['p-d', 'plugin-d', '2026-08-03 09:00:00']
    ]
    for (const [id, name, installedAt] of rows) {
      legacy.run(
        `INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path, installed_at)
         VALUES (?, ?, '1.0.0', ?, 'dist/main.js', ?, ?)`,
        [id, name, name, `/plugins/${name}`, installedAt]
      )
    }
    legacy.run('PRAGMA user_version = 2')
    writeFileSync(dbPath, Buffer.from(legacy.export()))
    legacy.close()

    await initDatabase(dbPath, 'sqljs')

    expect(getDatabase().version).toBe(3)
    // installed_at DESC, then id ASC → p-b, p-d, p-c, p-a
    expect(PluginRepository.findAll().map((plugin) => plugin.id)).toEqual([
      'p-b',
      'p-d',
      'p-c',
      'p-a'
    ])
    const backfilled = orderedRows()
    expect(backfilled.map((row) => row.id)).toEqual(['p-b', 'p-d', 'p-c', 'p-a'])
    expect(backfilled.map((row) => row.sort_order)).toEqual([1, 2, 3, 4])
  })

  it('reorders plugins transactionally and returns the list in the new order', async () => {
    await initDatabase(dbPath, 'sqljs')
    createPlugin('p-a', 'plugin-a')
    createPlugin('p-b', 'plugin-b')
    createPlugin('p-c', 'plugin-c')

    const plugins = PluginRepository.reorder(['p-c', 'p-a', 'p-b'])

    expect(plugins.map((plugin) => plugin.id)).toEqual(['p-c', 'p-a', 'p-b'])
    expect(PluginRepository.findAll().map((plugin) => plugin.id)).toEqual(['p-c', 'p-a', 'p-b'])
    const rows = orderedRows()
    expect(rows.map((row) => row.id)).toEqual(['p-c', 'p-a', 'p-b'])
    expect(rows.map((row) => row.sort_order)).toEqual([1, 2, 3])

    // Enabling plugins keeps the activation order aligned with the list order.
    PluginRepository.updateEnabled('p-c', true)
    PluginRepository.updateEnabled('p-a', true)
    PluginRepository.updateEnabled('p-b', true)
    expect(PluginRepository.getEnabledPlugins().map((plugin) => plugin.id)).toEqual([
      'p-c',
      'p-a',
      'p-b'
    ])
  })

  it('rejects incomplete, duplicate or unknown permutations without changing order', async () => {
    await initDatabase(dbPath, 'sqljs')
    createPlugin('p-a', 'plugin-a')
    createPlugin('p-b', 'plugin-b')
    createPlugin('p-c', 'plugin-c')

    expect(() => PluginRepository.reorder(['p-a', 'p-b'])).toThrow(/全部已安装/)
    expect(() => PluginRepository.reorder(['p-a', 'p-a', 'p-b'])).toThrow(/重复|缺失|未知/)
    expect(() => PluginRepository.reorder(['p-a', 'p-b', 'p-c', 'p-c'])).toThrow(/全部已安装/)
    expect(() => PluginRepository.reorder(['p-a', 'p-b', 'unknown'])).toThrow(/重复|缺失|未知/)
    expect(() => PluginRepository.reorder('p-a,p-b,p-c' as unknown as string[])).toThrow(
      /全部已安装/
    )

    expect(PluginRepository.findAll().map((plugin) => plugin.id)).toEqual(['p-a', 'p-b', 'p-c'])
    expect(orderedRows().map((row) => row.sort_order)).toEqual([1, 2, 3])
  })

  it('rolls back the whole reorder when a write fails mid-transaction', async () => {
    const SQL = await initSqlJs()
    const engine = new MemoryEngine(new SQL.Database())
    runMigrations(engine)
    for (const [id, name, installedAt, sortOrder] of [
      ['p-a', 'plugin-a', '2026-08-01 10:00:00', 1],
      ['p-b', 'plugin-b', '2026-08-02 10:00:00', 2],
      ['p-c', 'plugin-c', '2026-08-03 10:00:00', 3]
    ] as Array<[string, string, string, number]>) {
      engine.run(
        `INSERT INTO plugins (id, name, version, display_name, entry_main, installed_path, installed_at, sort_order)
         VALUES (?, ?, '1.0.0', ?, 'dist/main.js', ?, ?, ?)`,
        [id, name, name, `/plugins/${name}`, installedAt, sortOrder]
      )
    }

    let writes = 0
    engine.onRun = (sql, _params) => {
      if (sql.startsWith('UPDATE plugins SET sort_order')) {
        writes += 1
        if (writes === 2) throw new Error('injected reorder failure')
      }
    }

    expect(() => PluginRepository.reorder(['p-c', 'p-b', 'p-a'], engine)).toThrow(
      /injected reorder failure/
    )
    const rows = engine.all<{ id: string; sort_order: number }>(
      'SELECT id, sort_order FROM plugins ORDER BY sort_order ASC'
    )
    expect(rows.map((row) => row.id)).toEqual(['p-a', 'p-b', 'p-c'])
    expect(rows.map((row) => row.sort_order)).toEqual([1, 2, 3])
    engine.close()
  })

  it('appends newly installed plugins to the end of the order', async () => {
    await initDatabase(dbPath, 'sqljs')
    createPlugin('p-a', 'plugin-a')
    createPlugin('p-b', 'plugin-b')

    PluginRepository.reorder(['p-b', 'p-a'])
    createPlugin('p-c', 'plugin-c')

    const plugins = PluginRepository.findAll()
    expect(plugins.map((plugin) => plugin.id)).toEqual(['p-b', 'p-a', 'p-c'])
    expect(plugins.find((plugin) => plugin.id === 'p-c')?.sortOrder).toBe(3)
  })

  it('persists the reorder across a restart', async () => {
    await initDatabase(dbPath, 'sqljs')
    createPlugin('p-a', 'plugin-a')
    createPlugin('p-b', 'plugin-b')
    createPlugin('p-c', 'plugin-c')
    PluginRepository.reorder(['p-c', 'p-a', 'p-b'])

    closeDatabase()
    await initDatabase(dbPath, 'sqljs')

    expect(PluginRepository.findAll().map((plugin) => plugin.id)).toEqual(['p-c', 'p-a', 'p-b'])
    expect(orderedRows().map((row) => row.sort_order)).toEqual([1, 2, 3])
  })
})

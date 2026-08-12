// The test tsconfig narrows its include set, so pull in the host's local sql.js declaration.
// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="../shared/types/sql.js.d.ts" />

import initSqlJs, { type Database as SqlJsDatabase } from 'sql.js'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import type { EngineDb } from '../database/index'
import {
  applyPluginStorageBatch,
  assertPluginStorageKey,
  assertPluginStoragePrefix,
  deletePluginStorageValue,
  ensureLegacyPluginStorageMigrated,
  getPluginStorageValue,
  listPluginStorageValues,
  serializePluginStorageValue,
  setPluginStorageValue
} from '../database/pluginStorage'

vi.mock('electron', () => ({
  app: {
    getAppPath: vi.fn(() => '.'),
    getPath: vi.fn(() => '.')
  }
}))

import { closeDatabase, getDatabase, initDatabase, runMigrations } from '../database/index'

class MemoryEngine implements EngineDb {
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
}

describe('plugin storage', () => {
  it('accepts bounded JSON values and rejects unsafe keys or values', () => {
    expect(serializePluginStorageValue({ value: ['ok', 1, true, null] })).toBe(
      '{"value":["ok",1,true,null]}'
    )
    expect(() => assertPluginStorageKey('entry:2026-08-10')).not.toThrow()
    expect(() => assertPluginStoragePrefix('')).not.toThrow()
    expect(() => assertPluginStorageKey('bad\nkey')).toThrow(/key/)
    expect(() => assertPluginStorageKey('')).toThrow(/key/)
    expect(() => serializePluginStorageValue({ value: undefined })).toThrow(/JSON/)
    expect(() => serializePluginStorageValue(Number.POSITIVE_INFINITY)).toThrow(/finite/)
    const cyclic: Record<string, unknown> = {}
    cyclic.self = cyclic
    expect(() => serializePluginStorageValue(cyclic)).toThrow(/cycles/)
  })

  it('commits storage batches atomically and rolls back an interrupted save', async () => {
    const SQL = await initSqlJs()
    const base = new MemoryEngine(new SQL.Database())
    base.run(`
      CREATE TABLE plugin_storage (
        plugin_id TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        PRIMARY KEY (plugin_id, key)
      )
    `)
    setPluginStorageValue(base, 'diary-id', 'entry:2026-08-11', { content: 'old' })
    setPluginStorageValue(base, 'diary-id', 'draft:2026-08-11', { content: 'draft' })

    applyPluginStorageBatch(base, 'diary-id', [
      { type: 'set', key: 'entry:2026-08-11', value: { content: 'saved' } },
      { type: 'delete', key: 'draft:2026-08-11' }
    ])
    expect(getPluginStorageValue(base, 'diary-id', 'entry:2026-08-11')).toEqual({
      content: 'saved'
    })
    expect(getPluginStorageValue(base, 'diary-id', 'draft:2026-08-11')).toBeNull()

    setPluginStorageValue(base, 'diary-id', 'draft:2026-08-11', { content: 'next draft' })
    const interrupted: EngineDb = {
      close: () => base.close(),
      get version() {
        return base.version
      },
      set version(value: number) {
        base.version = value
      },
      exec: (sql) => base.exec(sql),
      run(sql, params) {
        if (sql.startsWith('DELETE FROM plugin_storage') && params?.[1] === 'draft:2026-08-11') {
          throw new Error('injected power loss')
        }
        base.run(sql, params)
      },
      all: (sql, params) => base.all(sql, params),
      get: (sql, params) => base.get(sql, params)
    }
    expect(() =>
      applyPluginStorageBatch(interrupted, 'diary-id', [
        { type: 'set', key: 'entry:2026-08-11', value: { content: 'partial' } },
        { type: 'delete', key: 'draft:2026-08-11' }
      ])
    ).toThrow(/power loss/)
    expect(getPluginStorageValue(base, 'diary-id', 'entry:2026-08-11')).toEqual({
      content: 'saved'
    })
    expect(getPluginStorageValue(base, 'diary-id', 'draft:2026-08-11')).toEqual({
      content: 'next draft'
    })
    base.close()
  })

  it('copies legacy production plugin data transactionally without deleting old tables', async () => {
    const SQL = await initSqlJs()
    const engine = new MemoryEngine(new SQL.Database())
    engine.run('CREATE TABLE plugins (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE)')
    engine.run('INSERT INTO plugins (id, name) VALUES (?, ?)', ['diary-id', 'diary'])
    engine.run('INSERT INTO plugins (id, name) VALUES (?, ?)', ['turntable-id', 'turntable'])
    engine.run('CREATE TABLE diary_entries (entry_date TEXT PRIMARY KEY, title TEXT, content TEXT)')
    engine.run('INSERT INTO diary_entries VALUES (?, ?, ?)', ['2026-08-10', '原日记', '保留内容'])
    engine.run(`
      CREATE TABLE turntable_items (
        id INTEGER PRIMARY KEY,
        label TEXT,
        weight REAL,
        color TEXT,
        sort_order INTEGER,
        created_at TEXT
      )
    `)
    engine.run('INSERT INTO turntable_items VALUES (?, ?, ?, ?, ?, ?)', [
      7,
      '选项 A',
      2,
      '#ffffff',
      0,
      '2026-08-10'
    ])
    engine.version = 1

    runMigrations(engine)

    expect(engine.version).toBe(3)
    const diary = engine.get<{ value: string }>(
      'SELECT value FROM plugin_storage WHERE plugin_id = ? AND key = ?',
      ['diary-id', 'entry:2026-08-10']
    )
    expect(JSON.parse(diary?.value ?? '{}')).toEqual({
      entry_date: '2026-08-10',
      title: '原日记',
      content: '保留内容'
    })
    const turntable = engine.get<{ value: string }>(
      'SELECT value FROM plugin_storage WHERE plugin_id = ? AND key = ?',
      ['turntable-id', 'items']
    )
    expect(JSON.parse(turntable?.value ?? '[]')).toMatchObject([
      { id: 7, label: '选项 A', weight: 2, sort_order: 0 }
    ])
    expect(
      engine.get("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'diary_entries'")
    ).toBeTruthy()
    expect(
      engine.get("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'turntable_items'")
    ).toBeTruthy()

    deletePluginStorageValue(engine, 'diary-id', 'entry:2026-08-10')
    ensureLegacyPluginStorageMigrated(engine, 'diary-id', 'diary')
    expect(getPluginStorageValue(engine, 'diary-id', 'entry:2026-08-10')).toBeNull()

    setPluginStorageValue(engine, 'diary-id', 'shared-key', { owner: 'diary' })
    setPluginStorageValue(engine, 'turntable-id', 'shared-key', { owner: 'turntable' })
    expect(getPluginStorageValue(engine, 'diary-id', 'shared-key')).toEqual({ owner: 'diary' })
    expect(getPluginStorageValue(engine, 'turntable-id', 'shared-key')).toEqual({
      owner: 'turntable'
    })
    expect(listPluginStorageValues(engine, 'diary-id', 'shared')).toEqual([
      { key: 'shared-key', value: { owner: 'diary' } }
    ])
    deletePluginStorageValue(engine, 'diary-id', 'shared-key')
    expect(getPluginStorageValue(engine, 'diary-id', 'shared-key')).toBeNull()
    expect(getPluginStorageValue(engine, 'turntable-id', 'shared-key')).toEqual({
      owner: 'turntable'
    })
    engine.close()
  })

  it('rolls back and preserves the old user_version when a migration fails', () => {
    const calls: string[] = []
    const engine: EngineDb = {
      close: () => undefined,
      version: 0,
      exec(sql) {
        calls.push(sql)
      },
      run() {
        throw new Error('injected migration failure')
      },
      all: () => [],
      get: () => undefined
    }

    expect(() => runMigrations(engine)).toThrow(/injected/)
    expect(calls).toEqual(['BEGIN IMMEDIATE', 'ROLLBACK'])
    expect(engine.version).toBe(0)
  })

  it('closes the global database and preserves legacy bytes when initialization migration fails', async () => {
    const directory = mkdtempSync(join(tmpdir(), 'openbox-db-failure-'))
    const path = join(directory, 'openbox.db')
    const previousEngine = process.env.OPENBOX_DB_ENGINE
    const SQL = await initSqlJs()
    const legacy = new SQL.Database()
    legacy.run('CREATE TABLE plugins (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE)')
    legacy.run('INSERT INTO plugins VALUES (?, ?)', ['diary-id', 'diary'])
    legacy.run('CREATE TABLE plugin_storage (wrong_column TEXT)')
    legacy.run('CREATE TABLE diary_entries (entry_date TEXT PRIMARY KEY, title TEXT, content TEXT)')
    legacy.run('INSERT INTO diary_entries VALUES (?, ?, ?)', ['2026-08-10', 'title', 'content'])
    legacy.run('PRAGMA user_version = 1')
    writeFileSync(path, Buffer.from(legacy.export()))
    legacy.close()

    try {
      process.env.OPENBOX_DB_ENGINE = 'sqljs'
      await expect(initDatabase(path)).rejects.toThrow(/migration failed/)
      expect(() => getDatabase()).toThrow(/未初始化/)

      const persisted = new SQL.Database(readFileSync(path))
      const persistedEngine = new MemoryEngine(persisted)
      expect(persistedEngine.version).toBe(1)
      expect(
        persistedEngine.get('SELECT * FROM diary_entries WHERE entry_date = ?', ['2026-08-10'])
      ).toMatchObject({ title: 'title', content: 'content' })
      expect(
        persistedEngine.get(
          "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'plugin_storage_migrations'"
        )
      ).toBeUndefined()
      persistedEngine.close()
    } finally {
      closeDatabase()
      if (previousEngine === undefined) delete process.env.OPENBOX_DB_ENGINE
      else process.env.OPENBOX_DB_ENGINE = previousEngine
      rmSync(directory, { recursive: true, force: true })
    }
  })
})

import { spawn, spawnSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import initSqlJs from 'sql.js'
import { resolvePackagedExecutable } from './packaged-paths.mjs'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const executable = resolvePackagedExecutable(repositoryRoot)

const smokeDirectory = mkdtempSync(join(tmpdir(), 'openbox-smoke-'))
const smokePluginDirectory = join(smokeDirectory, 'fixture', 'dice-roller')
const smokePluginDistDirectory = join(smokePluginDirectory, 'dist')
const smokeUniEnvDirectory = join(smokeDirectory, 'fixture', 'unienv')
const smokeUniEnvDistDirectory = join(smokeUniEnvDirectory, 'dist')
const smokeDatabasePath = join(smokeDirectory, 'data', 'openbox.db')
const temporaryRoot = `${resolve(tmpdir())}${sep}`
let child

async function seedLegacyDatabase() {
  const SQL = await initSqlJs()
  const database = new SQL.Database()
  database.run(`
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
  database.run('CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)')
  database.run(`
    CREATE TABLE plugin_logs (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      plugin_id TEXT NOT NULL,
      level TEXT NOT NULL DEFAULT 'info',
      message TEXT NOT NULL,
      timestamp DATETIME DEFAULT (datetime('now', 'localtime'))
    )
  `)
  for (const [id, name, version] of [
    ['legacy-diary-id', 'diary', '0.4.8'],
    ['legacy-turntable-id', 'turntable', '0.1.7']
  ]) {
    database.run(
      `INSERT INTO plugins (
        id, name, version, display_name, entry_main, entry_renderer,
        permissions, installed_path, enabled
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)`,
      [
        id,
        name,
        version,
        name,
        'dist/main.js',
        'dist/renderer.js',
        '["database:read","database:write"]',
        join(smokeDirectory, 'legacy', name)
      ]
    )
  }
  database.run(`
    CREATE TABLE diary_entries (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      entry_date TEXT NOT NULL UNIQUE,
      title TEXT NOT NULL DEFAULT '',
      content TEXT NOT NULL DEFAULT ''
    )
  `)
  database.run('INSERT INTO diary_entries (entry_date, title, content) VALUES (?, ?, ?)', [
    '2026-08-10',
    '迁移日记',
    '旧数据必须保留'
  ])
  database.run(`
    CREATE TABLE turntable_items (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      label TEXT NOT NULL,
      weight REAL NOT NULL,
      color TEXT NOT NULL,
      sort_order INTEGER NOT NULL,
      created_at TEXT
    )
  `)
  database.run(
    `INSERT INTO turntable_items (id, label, weight, color, sort_order, created_at)
     VALUES (?, ?, ?, ?, ?, ?)`,
    [7, '迁移选项', 2, '#ffffff', 0, '2026-08-10']
  )
  database.run('PRAGMA user_version = 1')
  const bytes = Buffer.from(database.export())
  database.close()
  mkdirSync(dirname(smokeDatabasePath), { recursive: true })
  writeFileSync(smokeDatabasePath, bytes)
  return { SQL, bytes }
}

function readSingleValue(database, sql, params = []) {
  const statement = database.prepare(sql)
  statement.bind(params)
  const value = statement.step() ? statement.get()[0] : undefined
  statement.free()
  return value
}

function hasColumn(database, table, columnName) {
  const statement = database.prepare(`PRAGMA table_info(${table})`)
  let found = false
  while (statement.step()) {
    if (statement.getAsObject().name === columnName) {
      found = true
      break
    }
  }
  statement.free()
  return found
}

function verifyLegacyMigration(SQL, originalBytes) {
  const backupPath = `${smokeDatabasePath}.bak-sqljs`
  if (!existsSync(backupPath) || !readFileSync(backupPath).equals(originalBytes)) {
    throw new Error('Legacy database byte backup is missing or changed')
  }
  const database = new SQL.Database(readFileSync(smokeDatabasePath))
  try {
    const version = Number(readSingleValue(database, 'PRAGMA user_version'))
    const diary = JSON.parse(
      String(
        readSingleValue(
          database,
          'SELECT value FROM plugin_storage WHERE plugin_id = ? AND key = ?',
          ['legacy-diary-id', 'entry:2026-08-10']
        )
      )
    )
    const turntable = JSON.parse(
      String(
        readSingleValue(
          database,
          'SELECT value FROM plugin_storage WHERE plugin_id = ? AND key = ?',
          ['legacy-turntable-id', 'items']
        )
      )
    )
    const legacyTableCount = Number(
      readSingleValue(
        database,
        `SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name IN ('diary_entries', 'turntable_items')`
      )
    )
    const markerCount = Number(
      readSingleValue(database, 'SELECT COUNT(*) FROM plugin_storage_migrations')
    )
    const sortOrderColumnPresent = hasColumn(database, 'plugins', 'sort_order')
    const pluginRows =
      database.exec('SELECT id, sort_order FROM plugins ORDER BY sort_order ASC')[0]?.values ?? []
    const pluginSortOrders = new Map(
      pluginRows.map(([id, sortOrder]) => [String(id), Number(sortOrder)])
    )
    const legacyDiarySortOrder = pluginSortOrders.get('legacy-diary-id')
    const legacyTurntableSortOrder = pluginSortOrders.get('legacy-turntable-id')
    if (
      version !== 3 ||
      diary.title !== '迁移日记' ||
      diary.content !== '旧数据必须保留' ||
      !Array.isArray(turntable) ||
      turntable[0]?.id !== 7 ||
      legacyTableCount !== 2 ||
      markerCount !== 2 ||
      !sortOrderColumnPresent ||
      pluginSortOrders.size < 2 ||
      legacyDiarySortOrder !== 1 ||
      legacyTurntableSortOrder !== 2
    ) {
      throw new Error('Packaged legacy data migration produced unexpected results')
    }
  } finally {
    database.close()
  }
  console.log('[smoke] legacy plugin data migrated transactionally with byte backup')
}

try {
  const legacyDatabase = await seedLegacyDatabase()
  mkdirSync(smokePluginDistDirectory, { recursive: true })
  mkdirSync(smokeUniEnvDistDirectory, { recursive: true })
  copyFileSync(
    resolve(repositoryRoot, 'plugins', 'dice-roller', 'plugin.json'),
    join(smokePluginDirectory, 'plugin.json')
  )
  for (const filename of ['main.js', 'renderer.js']) {
    copyFileSync(
      resolve(repositoryRoot, 'plugins', 'dice-roller', 'dist', filename),
      join(smokePluginDistDirectory, filename)
    )
  }
  copyFileSync(
    resolve(repositoryRoot, 'plugins', 'unienv', 'plugin.json'),
    join(smokeUniEnvDirectory, 'plugin.json')
  )
  for (const filename of ['main.js', 'renderer.js']) {
    copyFileSync(
      resolve(repositoryRoot, 'plugins', 'unienv', 'dist', filename),
      join(smokeUniEnvDistDirectory, filename)
    )
  }
  child = spawn(executable, [`--user-data-dir=${smokeDirectory}`], {
    env: {
      ...process.env,
      OPENBOX_SMOKE_TEST: '1',
      OPENBOX_SMOKE_PLUGIN_PATH: smokePluginDirectory,
      OPENBOX_SMOKE_UNIENV_PATH: smokeUniEnvDirectory
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true
  })

  let stdout = ''
  let stderr = ''
  child.stdout?.on('data', (chunk) => {
    stdout += chunk.toString()
  })
  child.stderr?.on('data', (chunk) => {
    stderr += chunk.toString()
  })

  const exitCode = await new Promise((resolveExit, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error('Packaged smoke test timed out after 30 seconds'))
    }, 30000)

    child.once('error', (error) => {
      clearTimeout(timeout)
      reject(error)
    })
    child.once('exit', (code) => {
      clearTimeout(timeout)
      resolveExit(code)
    })
  })

  if (exitCode !== 0) {
    throw new Error(
      `Packaged app exited with code ${exitCode}\nstdout:\n${stdout}\nstderr:\n${stderr}`
    )
  }

  verifyLegacyMigration(legacyDatabase.SQL, legacyDatabase.bytes)

  for (const marker of [
    '[smoke] renderer loaded with sandboxed preload bridge',
    '[smoke] renderer-only plugin skipped backend process',
    '[smoke] UniEnv trusted host service responded through the pinned proxy',
    '[smoke] plugin renderer loaded in an isolated cross-origin frame'
  ]) {
    if (!stdout.includes(marker)) {
      throw new Error(
        `Packaged smoke marker is missing: ${marker}\nstdout:\n${stdout}\nstderr:\n${stderr}`
      )
    }
  }

  const startupReports = stdout
    .split(/\r?\n/u)
    .filter((line) => line.startsWith('[metrics] '))
    .map((line) => JSON.parse(line.slice('[metrics] '.length)))
  const interactiveReport = startupReports.find((report) => report.phase === 'renderer.ready')
  if (!interactiveReport) {
    throw new Error(`Packaged smoke startup metrics are missing\nstdout:\n${stdout}`)
  }
  if (
    interactiveReport.durationMs > 20_000 ||
    interactiveReport.processes.totalWorkingSetKiB > 1_048_576 ||
    interactiveReport.processes.pluginUtilityCount !== 1
  ) {
    throw new Error(
      `Packaged smoke exceeded its startup budget: ${JSON.stringify(interactiveReport)}`
    )
  }

  console.log(
    `[smoke] packaged application initialized in ${interactiveReport.durationMs}ms with ` +
      `${interactiveReport.processes.totalWorkingSetKiB}KiB working set`
  )
} finally {
  if (child?.pid && child.exitCode === null) {
    spawnSync('taskkill.exe', ['/pid', String(child.pid), '/t', '/f'], {
      stdio: 'ignore',
      windowsHide: true
    })
  }
  const resolvedSmokeDirectory = resolve(smokeDirectory)
  if (resolvedSmokeDirectory.startsWith(temporaryRoot)) {
    rmSync(resolvedSmokeDirectory, { recursive: true, force: true })
  }
}

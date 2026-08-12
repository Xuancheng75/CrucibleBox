const { randomUUID } = require('node:crypto')
const { rmSync } = require('node:fs')
const { tmpdir } = require('node:os')
const { join } = require('node:path')
const { app } = require('electron')
const Database = require('better-sqlite3')

const databasePath = join(tmpdir(), `openbox-native-${randomUUID()}.db`)

app.whenReady().then(() => {
  let database
  try {
    database = new Database(databasePath)
    database.pragma('journal_mode = WAL')
    database.exec('CREATE TABLE probe (value TEXT NOT NULL)')
    database.prepare('INSERT INTO probe (value) VALUES (?)').run('abi-ok')
    const row = database.prepare('SELECT value FROM probe').get()
    if (!row || row.value !== 'abi-ok') {
      throw new Error('Native database round-trip returned an unexpected value')
    }

    console.log(
      `[native-smoke] electron=${process.versions.electron} node=${process.versions.node} modules=${process.versions.modules}`
    )
    app.exit(0)
  } catch (error) {
    console.error('[native-smoke] failed', error)
    app.exit(1)
  } finally {
    database?.close()
    rmSync(databasePath, { force: true })
    rmSync(`${databasePath}-shm`, { force: true })
    rmSync(`${databasePath}-wal`, { force: true })
  }
})

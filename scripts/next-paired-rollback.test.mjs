import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, mkdir, writeFile, readFile, symlink } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { DatabaseSync } from 'node:sqlite'
import { capturePair, restorePair, verifyPair } from './next-paired-rollback.mjs'
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'cb-next-pair-'))
  const program = join(root, 'old-program'),
    data = join(root, 'old-data')
  await mkdir(program)
  await mkdir(join(data, 'plugin-data/diary/empty'), { recursive: true })
  await mkdir(join(data, 'data'))
  await writeFile(join(program, 'cruciblebox.exe'), 'paired old program')
  await writeFile(join(data, 'plugin-data/diary/note.txt'), '中文附件\n')
  return { root, program, data }
}
test('paired capture backs up a coherent WAL database and restores old program with all raw data to new directories', async () => {
  const f = await fixture()
  const database = join(f.data, 'data/openbox.db')
  const db = new DatabaseSync(database)
  db.exec(
    'PRAGMA journal_mode=WAL;PRAGMA user_version=5;CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT);'
  )
  const raw = ' {"notes": ["旧笔记"], "unknown":true} '
  db.prepare('INSERT INTO settings VALUES(?,?)').run('diary', raw)
  const pair = join(f.root, 'pair')
  const manifest = await capturePair(f.program, f.data, pair)
  assert.equal(
    manifest.data.files.find((item) => item.path === 'data/openbox.db').database.schema,
    5
  )
  assert(!manifest.data.files.some((item) => item.path.endsWith('-wal')))
  db.prepare('UPDATE settings SET value=?').run('new data')
  db.close()
  await writeFile(join(f.program, 'cruciblebox.exe'), 'new program')
  const restored = await restorePair(pair, join(f.root, 'restored'))
  assert.equal(
    await readFile(join(restored.program, 'cruciblebox.exe'), 'utf8'),
    'paired old program'
  )
  const old = new DatabaseSync(join(restored.data, 'data/openbox.db'), { readOnly: true })
  try {
    assert.equal(old.prepare('SELECT value FROM settings').get().value, raw)
    assert.equal(old.prepare('PRAGMA user_version').get().user_version, 5)
  } finally {
    old.close()
  }
  assert.equal(
    await readFile(join(restored.data, 'plugin-data/diary/note.txt'), 'utf8'),
    '中文附件\n'
  )
  assert(existsSync(join(restored.data, 'plugin-data/diary/empty')))
  assert.equal(await readFile(join(f.program, 'cruciblebox.exe'), 'utf8'), 'new program')
  await verifyPair(pair)
  await verifyPair(join(f.root, 'restored'))
})
test('tampered pair is refused before any restore destination is created; existing destinations are never overwritten', async () => {
  const f = await fixture()
  const pair = join(f.root, 'pair')
  await capturePair(f.program, f.data, pair)
  await assert.rejects(capturePair(f.program, f.data, pair), /new external/)
  await assert.rejects(restorePair(pair, f.program), /new external/)
  await writeFile(join(pair, 'program/cruciblebox.exe'), 'tampered')
  const destination = join(f.root, 'denied')
  await assert.rejects(restorePair(pair, destination), /hash mismatch/)
  assert(!existsSync(destination))
})
test('linked data inputs and nested destinations fail closed while arbitrary non-SQLite data stays byte-exact', async () => {
  const f = await fixture()
  await writeFile(join(f.data, 'attachment.db'), 'not a SQLite database')
  await assert.rejects(capturePair(f.program, f.data, join(f.data, 'recursive')), /disjoint/)
  const pair = join(f.root, 'pair')
  await capturePair(f.program, f.data, pair)
  assert.equal(await readFile(join(pair, 'data/attachment.db'), 'utf8'), 'not a SQLite database')
  await symlink(f.program, join(f.data, 'redirect'), 'junction')
  await assert.rejects(
    capturePair(f.program, f.data, join(f.root, 'linked-pair')),
    /Linked|Redirected/
  )
  assert(!existsSync(join(f.root, 'linked-pair/pair.json')))
})

test('a closed WAL database remains coherently capturable when SQLite creates coordination sidecars', async () => {
  const f = await fixture()
  const file = join(f.data, 'data/closed.db')
  const db = new DatabaseSync(file)
  db.exec('PRAGMA journal_mode=WAL;CREATE TABLE notes(value TEXT)')
  db.prepare('INSERT INTO notes VALUES(?)').run('closed WAL value')
  db.close()
  const pair = join(f.root, 'closed-pair')
  await capturePair(f.program, f.data, pair)
  assert(!existsSync(join(pair, 'INCOMPLETE')))
  await verifyPair(pair)
  const backup = new DatabaseSync(join(pair, 'data/data/closed.db'), { readOnly: true })
  try {
    assert.equal(backup.prepare('SELECT value FROM notes').get().value, 'closed WAL value')
  } finally {
    backup.close()
  }
})

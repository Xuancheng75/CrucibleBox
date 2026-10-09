import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import { backup, DatabaseSync } from 'node:sqlite'

const [sourceArg, targetArg] = process.argv.slice(2)
if (!sourceArg || !targetArg) {
  throw new Error('Usage: node scripts/backup-tauri-db.mjs <source.db> <new-backup.db>')
}
const sourcePath = resolve(sourceArg)
const targetPath = resolve(targetArg)
if (sourcePath === targetPath || !existsSync(sourcePath) || existsSync(targetPath)) {
  throw new Error('Source must exist and backup target must be a different, unused path')
}

const source = new DatabaseSync(sourcePath, { readOnly: true })
try {
  await backup(source, targetPath)
} finally {
  source.close()
}

const restored = new DatabaseSync(targetPath, { readOnly: true })
try {
  const integrity = restored.prepare('PRAGMA integrity_check').get()
  if (integrity.integrity_check !== 'ok') {
    throw new Error(`Backup integrity check failed: ${integrity.integrity_check}`)
  }
  const version = restored.prepare('PRAGMA user_version').get().user_version
  console.log(`[tauri-db-backup] ${sourcePath} -> ${targetPath}; schema=${version}; integrity=ok`)
} finally {
  restored.close()
}

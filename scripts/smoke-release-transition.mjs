import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, sep } from 'node:path'
import initSqlJs from 'sql.js'
import { resolvePackagedExecutable } from './packaged-paths.mjs'

const previousRoot = resolve(process.argv[2] ?? '')
const candidateRoot = resolve(process.argv[3] ?? '')
if (!process.argv[2] || !process.argv[3]) {
  throw new Error(
    'Usage: smoke-release-transition.mjs <previous-repository> <candidate-repository>'
  )
}

const previousExecutable = resolvePackagedExecutable(previousRoot)
const candidateExecutable = resolvePackagedExecutable(candidateRoot)
const transitionDirectory = mkdtempSync(join(tmpdir(), 'openbox-release-transition-'))
const databasePath = join(transitionDirectory, 'data', 'openbox.db')
const markerKey = 'release-transition-marker'
const markerValue = `preserve-${Date.now()}`
const temporaryRoot = `${resolve(tmpdir())}${sep}`
let activeChild

async function runPackagedApp(executable, label) {
  activeChild = spawn(executable, [`--user-data-dir=${transitionDirectory}`], {
    env: { ...process.env, OPENBOX_SMOKE_TEST: '1' },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
    detached: process.platform !== 'win32'
  })
  let stdout = ''
  let stderr = ''
  activeChild.stdout?.on('data', (chunk) => {
    stdout += chunk.toString()
  })
  activeChild.stderr?.on('data', (chunk) => {
    stderr += chunk.toString()
  })
  const exitCode = await new Promise((resolveExit, reject) => {
    const timeout = setTimeout(
      () => reject(new Error(`${label} timed out after 45 seconds`)),
      45_000
    )
    activeChild.once('error', (error) => {
      clearTimeout(timeout)
      reject(error)
    })
    activeChild.once('exit', (code) => {
      clearTimeout(timeout)
      resolveExit(code)
    })
  })
  activeChild = undefined
  if (exitCode !== 0 || !stdout.includes('[smoke] renderer loaded with sandboxed preload bridge')) {
    throw new Error(`${label} failed with code ${exitCode}\nstdout:\n${stdout}\nstderr:\n${stderr}`)
  }
  console.log(`[transition] ${label} launched and exited cleanly`)
}

async function writeMarker() {
  if (!existsSync(databasePath))
    throw new Error('Previous release did not create the application database')
  const SQL = await initSqlJs()
  const database = new SQL.Database(readFileSync(databasePath))
  try {
    database.run('INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)', [
      markerKey,
      markerValue
    ])
    writeFileSync(databasePath, Buffer.from(database.export()))
  } finally {
    database.close()
  }
}

async function verifyMarker(label) {
  const SQL = await initSqlJs()
  const database = new SQL.Database(readFileSync(databasePath))
  try {
    const statement = database.prepare('SELECT value FROM settings WHERE key = ?')
    statement.bind([markerKey])
    const value = statement.step() ? statement.get()[0] : undefined
    statement.free()
    const versionResult = database.exec('PRAGMA user_version')
    const userVersion = Number(versionResult[0]?.values[0]?.[0] ?? 0)
    if (value !== markerValue || userVersion < 2) {
      throw new Error(`${label} did not preserve the release transition marker or database schema`)
    }
  } finally {
    database.close()
  }
  console.log(`[transition] ${label} preserved schema and user data`)
}

try {
  await runPackagedApp(previousExecutable, 'previous release')
  await writeMarker()
  await runPackagedApp(candidateExecutable, 'candidate upgrade')
  await verifyMarker('candidate upgrade')
  await runPackagedApp(previousExecutable, 'previous release rollback')
  await verifyMarker('previous release rollback')
} finally {
  if (activeChild?.pid && activeChild.exitCode === null) {
    if (process.platform === 'win32') {
      spawnSync('taskkill.exe', ['/pid', String(activeChild.pid), '/t', '/f'], {
        stdio: 'ignore',
        windowsHide: true
      })
    } else {
      process.kill(-activeChild.pid, 'SIGKILL')
    }
  }
  const resolved = resolve(transitionDirectory)
  if (resolved.startsWith(temporaryRoot)) rmSync(resolved, { recursive: true, force: true })
}

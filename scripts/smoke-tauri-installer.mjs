import { spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync } from 'node:fs'
import { rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve, sep } from 'node:path'

if (process.platform !== 'win32' || process.arch !== 'x64') {
  throw new Error('Tauri installer smoke requires Windows x64')
}

const installer = resolve(process.argv[2] ?? '')
if (!process.argv[2] || !existsSync(installer)) {
  throw new Error('Pass an existing NSIS installer path')
}

const root = mkdtempSync(join(tmpdir(), 'cruciblebox-tauri-smoke-'))
const installDirectory = join(root, 'installed')
const appData = join(root, 'appdata')
const logPath = join(appData, 'cruciblebox', 'logs', 'fault-history.log')
let app
let completed = false

function run(executable, args, label) {
  const result = spawnSync(executable, args, {
    timeout: 180_000,
    windowsHide: true,
    stdio: 'pipe'
  })
  if (result.error || result.status !== 0) {
    throw new Error(
      `${label} failed: ${result.error?.message ?? result.stderr?.toString() ?? result.status}`
    )
  }
}

async function waitForStartup() {
  const deadline = Date.now() + 30_000
  while (Date.now() < deadline) {
    if (app.exitCode !== null) throw new Error(`Application exited before startup: ${app.exitCode}`)
    if (existsSync(logPath)) {
      const log = readFileSync(logPath, 'utf8')
      if (
        ['startup', 'database-ready', 'plugins-ready'].every((stage) =>
          log.includes(`\t${stage}\t`)
        )
      ) {
        return
      }
    }
    await new Promise((done) => setTimeout(done, 250))
  }
  throw new Error('Application did not reach plugins-ready within 30 seconds')
}

try {
  run(installer, ['/S', `/D=${installDirectory}`], 'NSIS install')
  const executable = join(installDirectory, 'cruciblebox.exe')
  for (const path of [
    executable,
    join(installDirectory, 'cruciblebox-plugin-host.exe'),
    join(installDirectory, 'ocr-worker.exe'),
    join(installDirectory, 'onnxruntime.dll'),
    join(installDirectory, '7zip', '7za.exe'),
    join(installDirectory, 'formula-ocr', 'python', 'python.exe')
  ]) {
    if (!existsSync(path)) throw new Error(`Installed runtime missing: ${path}`)
  }
  app = spawn(executable, [], {
    cwd: installDirectory,
    env: { ...process.env, APPDATA: appData },
    windowsHide: true,
    stdio: 'ignore'
  })
  await waitForStartup()
  const exited = new Promise((done) => app.once('exit', done))
  if (!app.kill()) throw new Error('Could not stop installed application')
  await exited
  app = undefined
  run(join(installDirectory, 'uninstall.exe'), ['/S'], 'NSIS uninstall')
  const uninstallDeadline = Date.now() + 15_000
  while (existsSync(installDirectory) && Date.now() < uninstallDeadline) {
    await new Promise((done) => setTimeout(done, 250))
  }
  if (existsSync(installDirectory)) throw new Error('Installer directory remains after uninstall')
  completed = true
  console.log(`[tauri-installer-smoke] passed: ${installer}`)
} finally {
  if (app?.exitCode === null) app.kill()
  if (completed) {
    const resolvedRoot = resolve(root)
    if (!resolvedRoot.startsWith(`${resolve(tmpdir())}${sep}`)) {
      throw new Error('Refusing to clean outside the temporary directory')
    }
    await rm(resolvedRoot, { recursive: true, force: true })
  } else {
    console.error(`[tauri-installer-smoke] retained failure artifacts: ${root}`)
  }
}

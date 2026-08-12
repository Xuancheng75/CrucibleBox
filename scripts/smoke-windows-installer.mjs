import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readdirSync } from 'node:fs'
import { rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, join, resolve, sep } from 'node:path'

if (process.platform !== 'win32' || process.arch !== 'x64') {
  throw new Error(
    `Windows installer smoke requires Windows x64, received ${process.platform}/${process.arch}`
  )
}

const releaseDirectory = resolve(process.argv[2] ?? 'release')
const installers = readdirSync(releaseDirectory, { withFileTypes: true })
  .filter((entry) => entry.isFile() && entry.name.endsWith('-windows-x64-setup.exe'))
  .map((entry) => join(releaseDirectory, entry.name))
if (installers.length !== 1) {
  throw new Error(`Expected exactly one Windows x64 installer, found ${installers.length}`)
}

const probeRoot = mkdtempSync(join(tmpdir(), 'openbox-installer-smoke-'))
const installDirectory = join(probeRoot, 'CrucibleBox 测试安装')
const userDataDirectory = join(probeRoot, '用户数据')

function run(executable, args, label, capture = false) {
  const result = spawnSync(executable, args, {
    encoding: capture ? 'utf8' : undefined,
    env: { ...process.env, OPENBOX_SMOKE_TEST: '1' },
    maxBuffer: 2 * 1024 * 1024,
    timeout: 120_000,
    windowsHide: true
  })
  if (result.error || result.status !== 0) {
    throw new Error(
      `${label} failed (${result.status ?? 'no exit code'}): ${result.error?.message ?? result.stderr ?? ''}`
    )
  }
  return result
}

async function cleanupProbe(path) {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    try {
      await rm(path, { recursive: true, force: true })
      return
    } catch (error) {
      if (!['EBUSY', 'ENOTEMPTY', 'EPERM'].includes(error?.code) || attempt === 29) throw error
      await new Promise((resolveDelay) => setTimeout(resolveDelay, 500))
    }
  }
}

try {
  run(installers[0], ['/S', `/D=${installDirectory}`], 'NSIS installation')
  const application = join(installDirectory, 'CrucibleBox.exe')
  if (!existsSync(application)) throw new Error('NSIS installation did not create CrucibleBox.exe')

  const smoke = run(
    application,
    [`--user-data-dir=${userDataDirectory}`],
    'Installed application smoke',
    true
  )
  if (!smoke.stdout.includes('[smoke] renderer loaded with sandboxed preload bridge')) {
    throw new Error(
      `Installed application did not emit the smoke marker:\n${smoke.stdout}\n${smoke.stderr}`
    )
  }

  const uninstaller = join(installDirectory, 'Uninstall CrucibleBox.exe')
  if (!existsSync(uninstaller)) throw new Error('NSIS installation did not create the uninstaller')
  run(uninstaller, ['/S'], 'NSIS uninstallation')
  console.log(`[installer-smoke] ${basename(installers[0])} installed, launched and uninstalled`)
} finally {
  const resolvedProbe = resolve(probeRoot)
  const temporaryRoot = `${resolve(tmpdir())}${sep}`
  if (!resolvedProbe.startsWith(temporaryRoot)) throw new Error('Refusing to clean outside temp')
  await cleanupProbe(resolvedProbe)
}

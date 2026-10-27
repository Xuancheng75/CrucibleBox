import { existsSync, readFileSync, statSync, readdirSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { resolve, join } from 'node:path'

const args = process.argv.slice(2)
const staged = args.includes('--staged')
const profile = args.find((arg) => arg.startsWith('--profile='))?.slice(10) ?? 'full'
if (!['base', 'full'].includes(profile))
  throw new Error('Unknown Tauri resource profile: ' + profile)
const targetArg = args.find((arg) => !arg.startsWith('--'))
const target = resolve(targetArg ?? '.')
let hostTriple
if (staged) {
  try {
    hostTriple = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (\S+)$/m)?.[1]
  } catch {
    // Fall through to the known Windows candidate names below.
  }
}

function firstFile(candidates) {
  return candidates.find((candidate) => {
    try {
      return existsSync(candidate) && statSync(candidate).isFile()
    } catch {
      return false
    }
  })
}

const pluginHostCandidates = staged
  ? [
      ...(hostTriple
        ? [join(target, 'src-tauri', 'binaries', `cruciblebox-plugin-host-${hostTriple}.exe`)]
        : []),
      join(target, 'src-tauri', 'binaries', 'cruciblebox-plugin-host-x86_64-pc-windows-msvc.exe'),
      join(target, 'src-tauri', 'binaries', 'cruciblebox-plugin-host-x86_64-pc-windows-gnu.exe')
    ]
  : [
      join(target, 'cruciblebox-plugin-host.exe'),
      join(target, 'resources', 'cruciblebox-plugin-host.exe')
    ]
const pluginHost = firstFile(pluginHostCandidates)
if (!pluginHost)
  throw new Error(`Tauri runtime asset missing: cruciblebox-plugin-host.exe (${target})`)
// Installed host binaries are fixed; plugin executables belong in installed plugin directories.
if (!staged) {
  const binaries = new Set([
    'cruciblebox.exe',
    'cruciblebox-plugin-host.exe',
    'webview2loader.dll',
    'uninstall.exe',
    'uninstall cruciblebox.exe'
  ])
  for (const root of [target, join(target, 'resources')]) {
    if (!existsSync(root)) continue
    for (const entry of readdirSync(root, { withFileTypes: true })) {
      if (
        entry.isFile() &&
        /\.(exe|dll)$/i.test(entry.name) &&
        !binaries.has(entry.name.toLowerCase())
      )
        throw new Error(`Host package contains unexpected runtime: ${entry.name}`)
    }
  }
}
console.log(`[tauri-assets] Plugin host: ${pluginHost}`)
const releasePluginHost = join(
  target,
  'src-tauri',
  'cruciblebox-plugin-host',
  'target',
  'release',
  'cruciblebox-plugin-host.exe'
)
if (staged && existsSync(releasePluginHost)) {
  const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex')
  if (hash(pluginHost) !== hash(releasePluginHost)) {
    throw new Error(`Staged plugin host is stale; rebuild and stage the sidecar (${pluginHost})`)
  }
}
console.log('[tauri-assets] Plugin business resources are independently managed')

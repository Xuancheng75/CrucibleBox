import { existsSync, readFileSync, statSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { resolve, join } from 'node:path'

const args = process.argv.slice(2)
const staged = args.includes('--staged')
const profile = args.find((arg) => arg.startsWith('--profile='))?.slice(10) ?? 'full'
if (!['base', 'full'].includes(profile))
  throw new Error('Unknown Tauri resource profile: ' + profile)
const requiresOcr = profile === 'full'
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

const workerCandidates = staged
  ? [
      ...(hostTriple
        ? [join(target, 'src-tauri', 'binaries', `ocr-worker-${hostTriple}.exe`)]
        : []),
      join(target, 'src-tauri', 'binaries', 'ocr-worker-x86_64-pc-windows-msvc.exe'),
      join(target, 'src-tauri', 'binaries', 'ocr-worker-x86_64-pc-windows-gnu.exe'),
      join(target, 'src-tauri', 'binaries', 'ocr-worker.exe')
    ]
  : [
      join(target, 'ocr-worker.exe'),
      join(target, 'resources', 'ocr-worker.exe'),
      join(target, 'ocr-worker-x86_64-pc-windows-msvc.exe'),
      join(target, 'resources', 'ocr-worker-x86_64-pc-windows-msvc.exe')
    ]

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
const ortDirectories = staged
  ? [join(target, 'src-tauri', 'binaries')]
  : [target, join(target, 'resources'), join(target, 'binaries')]

const worker = firstFile(workerCandidates)
const ortDirectory = ortDirectories.find(
  (directory) =>
    firstFile([join(directory, 'onnxruntime.dll')]) &&
    firstFile([join(directory, 'onnxruntime_providers_shared.dll')])
)
const ort = ortDirectory
  ? [join(ortDirectory, 'onnxruntime.dll'), join(ortDirectory, 'onnxruntime_providers_shared.dll')]
  : [undefined, undefined]
const sevenZipDirectory = staged
  ? join(target, 'src-tauri', 'resources', '7zip')
  : ([join(target, '7zip'), join(target, 'resources', '7zip')].find((directory) =>
      ['7za.exe', '7za.dll', 'License.txt'].every((name) => firstFile([join(directory, name)]))
    ) ?? join(target, '7zip'))
const sevenZip = [
  firstFile([join(sevenZipDirectory, '7za.exe')]),
  firstFile([join(sevenZipDirectory, '7za.dll')]),
  firstFile([join(sevenZipDirectory, 'License.txt')])
]
if (!pluginHost || sevenZip.some((file) => !file) || (requiresOcr && (!worker || !ortDirectory))) {
  const missing = [
    requiresOcr && !worker && 'ocr-worker.exe',
    !pluginHost && 'cruciblebox-plugin-host.exe',
    requiresOcr && !ort[0] && 'onnxruntime.dll',
    requiresOcr && !ort[1] && 'onnxruntime_providers_shared.dll',
    !sevenZip[0] && '7zip/7za.exe',
    !sevenZip[1] && '7zip/7za.dll',
    !sevenZip[2] && '7zip/License.txt'
  ]
    .filter(Boolean)
    .join(', ')
  throw new Error(
    `Tauri runtime assets missing: ${missing}\n` +
      `checked root: ${target}\n` +
      `worker candidates: ${workerCandidates.join(', ')}\n` +
      `ONNX Runtime directories: ${ortDirectories.join(', ')}\n` +
      `7-Zip directory: ${sevenZipDirectory}`
  )
}

console.log(`[tauri-assets] OCR Worker: ${worker}`)
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
const releaseWorker = join(target, 'ocr-worker', 'target', 'release', 'ocr-worker.exe')
if (requiresOcr && staged && existsSync(releaseWorker)) {
  const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex')
  if (hash(worker) !== hash(releaseWorker)) {
    throw new Error(`Staged OCR worker is stale; run npm run prepare:ocr-worker (${worker})`)
  }
}
console.log(`[tauri-assets] ONNX Runtime: ${ort[0]}`)
console.log(`[tauri-assets] ONNX Runtime providers: ${ort[1]}`)
console.log(`[tauri-assets] 7-Zip: ${sevenZip[0]}`)

console.log('[tauri-assets] Resource profile: ' + profile)

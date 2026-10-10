import { createHash } from 'node:crypto'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
  renameSync
} from 'node:fs'
import { resolve, join } from 'node:path'
import { execFileSync } from 'node:child_process'

// Build the production feature set explicitly. Acceptance fault entry points never enter this artifact.
const args = process.argv.slice(2)
const option = (name) => {
  const prefix = `--${name}=`
  const inline = args.find((arg) => arg.startsWith(prefix))
  if (inline) return inline.slice(prefix.length)
  const index = args.indexOf(`--${name}`)
  if (index < 0) return undefined
  const value = args[index + 1]
  if (!value || value.startsWith('--')) throw new Error(`Missing value for --${name}`)
  return value
}
const root = resolve(import.meta.dirname, '..')
const output = resolve(option('output') ?? 'document-runtime-output')
const target = resolve(option('target-dir') ?? join(output, 'build'))
const version = option('version') ?? '0.1.0'
const pinSource = option('pin-source')
if (pinSource && pinSource !== 'shared/document-runtime-catalog.json')
  throw new Error('Only the document runtime catalog can be pinned')
if (!/^[A-Za-z0-9.-]{1,64}$/.test(version)) throw new Error('Invalid runtime version')
execFileSync(
  'cargo',
  [
    'build',
    '--offline',
    '--release',
    '--locked',
    '--manifest-path',
    join(root, 'workers/document/Cargo.toml'),
    '--no-default-features',
    '--features',
    'worker',
    '--target-dir',
    target
  ],
  { stdio: 'inherit' }
)
const directory = join(output, version)
if (existsSync(directory)) throw new Error('Refusing to overwrite an immutable runtime package')
mkdirSync(directory, { recursive: true })
const files = {}
for (const [name, source] of [
  ['document-worker.exe', join(target, 'release/document-worker.exe')],
  ['pdfium.dll', join(root, 'src-tauri/resources/pdfium.dll')]
]) {
  copyFileSync(source, join(directory, name))
  const bytes = readFileSync(join(directory, name))
  files[name] = { bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') }
}
if (readdirSync(directory).length !== 2) throw new Error('Unexpected runtime files')
const catalog = { version, wire_version: 1, files }
const catalogPath = join(output, `${version}.catalog.json`)
if (existsSync(catalogPath)) throw new Error('Refusing to replace an immutable catalog')
writeFileSync(catalogPath, JSON.stringify(catalog, null, 2) + '\n', { flag: 'wx' })
const triple = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (\S+)$/m)?.[1]
writeFileSync(
  join(output, `${version}.evidence.json`),
  JSON.stringify(
    {
      catalog,
      triple,
      directory,
      productionFeatures: ['worker'],
      totalBytes: Object.values(files).reduce((sum, file) => sum + file.bytes, 0),
      executableBytes: statSync(join(directory, 'document-worker.exe')).size
    },
    null,
    2
  ) + '\n',
  { flag: 'wx' }
)
if (pinSource) {
  const pinPath = resolve(root, pinSource)
  const temporaryPinPath = pinPath + '.' + process.pid + '.tmp'
  writeFileSync(temporaryPinPath, JSON.stringify(catalog, null, 2) + '\n', { flag: 'wx' })
  renameSync(temporaryPinPath, pinPath)
}
console.log('[document-runtime] ' + catalogPath)

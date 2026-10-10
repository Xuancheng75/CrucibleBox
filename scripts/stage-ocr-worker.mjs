import { execFileSync, spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const rustc = execFileSync('rustc', ['-vV'], { encoding: 'utf8' })
const target = rustc.match(/^host: (x86_64-pc-windows-(?:gnu|msvc))$/m)?.[1]
if (!target) throw new Error('OCR worker staging supports Windows x64 GNU/MSVC only')

const result = spawnSync(
  'cargo',
  ['build', '--release', '--manifest-path', join(root, 'ocr-worker', 'Cargo.toml')],
  {
    cwd: root,
    stdio: 'inherit'
  }
)
if (result.error) throw result.error
if (result.status !== 0) throw new Error(`OCR worker build failed: ${result.status}`)

const source = join(root, 'ocr-worker', 'target', 'release', 'ocr-worker.exe')
if (!existsSync(source)) throw new Error(`OCR worker build did not create ${source}`)
const staged = join(root, 'src-tauri', 'binaries', `ocr-worker-${target}.exe`)
mkdirSync(dirname(staged), { recursive: true })
copyFileSync(source, staged)
console.log(`[ocr-worker] staged ${source} -> ${staged}`)

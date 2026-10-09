import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(
  /^host: (x86_64-pc-windows-(?:gnu|msvc))$/m
)?.[1]
if (!target) throw new Error('Plugin host staging requires Windows x64 GNU/MSVC')
if (process.env.CARGO_BUILD_TARGET && process.env.CARGO_BUILD_TARGET !== target)
  throw new Error('Cross-target staging requires an explicit matching build; refusing host guess')
const source = join(
  root,
  'src-tauri/cruciblebox-plugin-host/target/release/cruciblebox-plugin-host.exe'
)
if (!existsSync(source)) throw new Error('Build the release plugin host before staging')
const destination = join(root, 'src-tauri/binaries', `cruciblebox-plugin-host-${target}.exe`)
mkdirSync(dirname(destination), { recursive: true })
copyFileSync(source, destination)
const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex')
if (hash(source) !== hash(destination)) throw new Error('Plugin host staging digest mismatch')
console.log(`[plugin-host] staged ${target}: ${hash(destination)}`)

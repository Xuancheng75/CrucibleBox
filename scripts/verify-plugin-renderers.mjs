import { readFile, stat } from 'node:fs/promises'
import { resolve } from 'node:path'
import { validateManifest } from '../packages/cruciblebox-next-api/src/index.mjs'

const PLUGINS = JSON.parse(
  await readFile(new URL('./next-plugin-catalog.json', import.meta.url), 'utf8')
)
  .filter((plugin) => plugin.runtimeFiles.includes('dist/renderer.js'))
  .map((plugin) => plugin.id)
const FORBIDDEN = [
  ['CommonJS require', /\brequire\s*\(/],
  ['ES module import', /(^|[;\n])\s*import(?:\s|\()/m],
  ['eval', /\beval\s*\(/],
  ['Function constructor', /\bnew\s+Function\s*\(/]
]

let failed = false

for (const plugin of PLUGINS) {
  const rendererPath = resolve('plugins', plugin, 'dist', 'renderer.js')
  const [source, metadata] = await Promise.all([readFile(rendererPath, 'utf8'), stat(rendererPath)])
  const issues = []

  for (const [label, pattern] of FORBIDDEN) {
    if (pattern.test(source)) issues.push(label)
  }
  const manifestRaw = await readFile(resolve('plugins', plugin, 'plugin.json'), 'utf8')
  if (JSON.parse(manifestRaw).manifestVersion === 5) {
    validateManifest(manifestRaw)
    if (!/\bexport\s*(?:function\s+mount|\{[^}]*\b(?:as\s+)?mount\b)/.test(source))
      issues.push('Next mount export')
    if (source.includes('__OPENBOX_PLUGIN_RUNTIME__')) issues.push('legacy runtime registration')
  } else {
    if (!source.includes('__OPENBOX_PLUGIN_RUNTIME__')) issues.push('runtime registration marker')
    if (!source.includes('.mount(')) issues.push('runtime mount call')
  }
  if (metadata.size < 100_000) issues.push('bundled React/ReactDOM payload')

  if (issues.length > 0) {
    failed = true
    console.error(`${plugin}: invalid renderer (${issues.join(', ')})`)
  } else {
    console.log(`${plugin}: ${metadata.size} bytes, self-contained browser bundle`)
  }
}

if (failed) process.exitCode = 1

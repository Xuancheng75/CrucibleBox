import { readFile, stat } from 'node:fs/promises'
import { resolve } from 'node:path'

const PLUGINS = [
  'diary',
  'dice-roller',
  'gif-editor',
  'theme-manager',
  'turntable',
  'unienv',
  'document-engine'
]
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
  if (!source.includes('__OPENBOX_PLUGIN_RUNTIME__')) issues.push('runtime registration marker')
  if (!source.includes('.mount(')) issues.push('runtime mount call')
  if (metadata.size < 100_000) issues.push('bundled React/ReactDOM payload')

  if (issues.length > 0) {
    failed = true
    console.error(`${plugin}: invalid renderer (${issues.join(', ')})`)
  } else {
    console.log(`${plugin}: ${metadata.size} bytes, self-contained browser bundle`)
  }
}

if (failed) process.exitCode = 1

import { spawnSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const policy = JSON.parse(
  readFileSync(resolve(root, 'contracts/next/official-plugins.json'), 'utf8')
)
const legacy = JSON.parse(readFileSync(resolve(root, 'scripts/plugin-catalog.json'), 'utf8'))
const allowed = policy.officialPlugins.map(({ id }) => id)
if (allowed.length !== 7 || new Set(allowed).size !== allowed.length)
  throw Error('Next official scope must contain seven unique plugins')
const selected = allowed.map((id) => {
  const entry = legacy.find((plugin) => plugin.id === id)
  if (!entry) throw Error(`Missing preserved packaging metadata: ${id}`)
  return entry
})
if (policy.stage !== 'frozen') throw Error('Next official plugin catalog must be frozen')
const excluded = policy.excludedFromNextOfficial
if (
  new Set([...allowed, ...excluded]).size !== legacy.length ||
  legacy.some(({ id }) => !allowed.includes(id) && !excluded.includes(id))
)
  throw Error('Legacy catalog needs an explicit preservation decision')
const formatted = spawnSync(
  process.execPath,
  [
    resolve(root, 'node_modules/prettier/bin/prettier.cjs'),
    '--stdin-filepath',
    'next-plugin-catalog.json'
  ],
  { input: JSON.stringify(selected), encoding: 'utf8', windowsHide: true }
)
if (formatted.status !== 0) throw Error(formatted.stderr || 'Formatter unavailable')
const generated = formatted.stdout.replace(/\r\n/g, '\n')
const target = resolve(root, 'scripts/next-plugin-catalog.json')
if (process.argv.includes('--check')) {
  if (readFileSync(target, 'utf8') !== generated) throw Error('Next catalog drift')
} else writeFileSync(target, generated)
console.log(
  'Frozen Next official catalog: ' + allowed.join(', ') + '; legacy packages and data preserved'
)

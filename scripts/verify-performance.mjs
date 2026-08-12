import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, extname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const budgets = JSON.parse(
  readFileSync(resolve(scriptDirectory, 'performance-budgets.json'), 'utf8')
)

function listFiles(directory) {
  if (!existsSync(directory)) throw new Error(`Performance input is missing: ${directory}`)
  const files = []
  for (const name of readdirSync(directory)) {
    const path = resolve(directory, name)
    if (statSync(path).isDirectory()) files.push(...listFiles(path))
    else files.push(path)
  }
  return files
}

function measureJavaScript(directory) {
  return listFiles(directory)
    .filter((path) => extname(path) === '.js')
    .reduce((total, path) => total + statSync(path).size, 0)
}

function measureRendererEntryJavaScript(directory, additionalEntries = []) {
  const manifestPath = resolve(directory, '.vite', 'manifest.json')
  if (!existsSync(manifestPath)) throw new Error(`Renderer manifest is missing: ${manifestPath}`)
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
  const entries = Object.entries(manifest).filter(([, value]) => value.isEntry)
  if (entries.length !== 1) {
    throw new Error(`Expected one renderer entry in Vite manifest, received ${entries.length}`)
  }

  const files = new Set()
  const visited = new Set()
  function visit(key) {
    if (visited.has(key)) return
    visited.add(key)
    const chunk = manifest[key]
    if (!chunk) throw new Error(`Renderer manifest import is missing: ${key}`)
    if (extname(chunk.file) === '.js') files.add(resolve(directory, chunk.file))
    for (const importedKey of chunk.imports ?? []) visit(importedKey)
  }
  for (const key of [entries[0][0], ...additionalEntries]) visit(key)
  return [...files].reduce((total, path) => total + statSync(path).size, 0)
}

function assertBudget(label, actualBytes, maximumBytes) {
  if (!Number.isSafeInteger(maximumBytes) || maximumBytes <= 0) {
    throw new Error(`Invalid performance budget for ${label}`)
  }
  if (actualBytes > maximumBytes) {
    throw new Error(`${label} is ${actualBytes} bytes; budget is ${maximumBytes} bytes`)
  }
  console.log(`[performance] ${label}: ${actualBytes}/${maximumBytes} bytes`)
}

assertBudget(
  'host main JavaScript',
  measureJavaScript(resolve(repositoryRoot, 'out', 'main')),
  budgets.hostMainJavaScriptBytes
)
assertBudget(
  'host renderer JavaScript',
  measureJavaScript(resolve(repositoryRoot, 'out', 'renderer')),
  budgets.hostRendererJavaScriptBytes
)
assertBudget(
  'host renderer entry JavaScript',
  measureRendererEntryJavaScript(resolve(repositoryRoot, 'out', 'renderer')),
  budgets.hostRendererEntryJavaScriptBytes
)
assertBudget(
  'host renderer startup JavaScript',
  measureRendererEntryJavaScript(resolve(repositoryRoot, 'out', 'renderer'), [
    'src/pages/Home.tsx'
  ]),
  budgets.hostRendererStartupJavaScriptBytes
)
assertBudget(
  'plugin frame runtime',
  statSync(resolve(repositoryRoot, 'out', 'plugin-frame', 'runtime.js')).size,
  budgets.pluginFrameRuntimeBytes
)

for (const [pluginId, maximumBytes] of Object.entries(budgets.pluginRendererBytes)) {
  const rendererPath = resolve(repositoryRoot, 'plugins', pluginId, 'dist', 'renderer.js')
  assertBudget(`${pluginId} renderer`, statSync(rendererPath).size, maximumBytes)
}

import { rmSync } from 'node:fs'
import { dirname, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const pluginsRoot = resolve(scriptDirectory, '..', 'plugins')
const pluginDirectory = resolve(process.cwd())
const expectedPrefix = `${pluginsRoot}${sep}`

if (!pluginDirectory.startsWith(expectedPrefix)) {
  throw new Error(`Refusing to clean outside the plugins directory: ${pluginDirectory}`)
}

const distDirectory = resolve(pluginDirectory, 'dist')
if (!distDirectory.startsWith(`${pluginDirectory}${sep}`)) {
  throw new Error(`Resolved dist directory escaped plugin root: ${distDirectory}`)
}

rmSync(distDirectory, { recursive: true, force: true })

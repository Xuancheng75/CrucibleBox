import { readFile, stat } from 'node:fs/promises'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repositoryRoot = resolve(fileURLToPath(new URL('..', import.meta.url)))
const runtimePath = resolve(repositoryRoot, 'out', 'plugin-frame', 'runtime.js')

let source
let metadata
try {
  ;[source, metadata] = await Promise.all([readFile(runtimePath, 'utf8'), stat(runtimePath)])
} catch (error) {
  throw new Error(`plugin frame runtime is missing: ${runtimePath}\n${error}`)
}

if (!metadata.isFile() || metadata.size === 0) {
  throw new Error(`plugin frame runtime is empty or not a regular file: ${runtimePath}`)
}

const requiredMarkers = ['__OPENBOX_PLUGIN_RUNTIME__', 'onFilesDropped', 'host.filesDropped']
const missing = requiredMarkers.filter((marker) => !source.includes(marker))
if (missing.length > 0) {
  throw new Error(
    `plugin frame runtime is stale or incomplete (${missing.join(', ')} missing): ${runtimePath}`
  )
}

console.log(`[plugin-frame] ${metadata.size} bytes, renderer API markers verified`)

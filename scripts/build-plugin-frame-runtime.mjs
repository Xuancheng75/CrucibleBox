import { build } from 'esbuild'
import { mkdir } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = dirname(fileURLToPath(import.meta.url))
const repositoryRoot = resolve(scriptDirectory, '..')
const outputDirectory = resolve(repositoryRoot, 'out', 'plugin-frame')
const production = process.argv.includes('--production')

await mkdir(outputDirectory, { recursive: true })
await build({
  entryPoints: [resolve(repositoryRoot, 'src', 'plugin-runtime', 'frame-entry.ts')],
  outfile: resolve(outputDirectory, 'runtime.js'),
  bundle: true,
  platform: 'browser',
  format: 'iife',
  target: ['chrome150'],
  minify: production,
  sourcemap: production ? false : 'inline',
  legalComments: 'none',
  logLevel: 'info'
})

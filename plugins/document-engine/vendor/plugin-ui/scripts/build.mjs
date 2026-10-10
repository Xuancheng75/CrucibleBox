import { build } from 'esbuild'
import { copyFile, mkdir, readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const styles = await readFile(resolve(root, 'src/style.css'), 'utf8')
await mkdir(resolve(root, 'dist'), { recursive: true })
await build({
  entryPoints: [resolve(root, 'src/index.tsx')],
  outfile: resolve(root, 'dist/index.js'),
  bundle: true,
  format: 'esm',
  platform: 'browser',
  target: 'es2022',
  external: ['react', 'react/jsx-runtime'],
  define: { __CBX_PLUGIN_UI_STYLES__: JSON.stringify(styles) },
  logLevel: 'info'
})
await build({
  entryPoints: [resolve(root, 'src/index.tsx')],
  outfile: resolve(root, 'dist/index.cjs'),
  bundle: true,
  format: 'cjs',
  platform: 'node',
  target: 'node18',
  external: ['react', 'react/jsx-runtime'],
  define: { __CBX_PLUGIN_UI_STYLES__: JSON.stringify(styles) },
  logLevel: 'info'
})
await copyFile(resolve(root, 'src/style.css'), resolve(root, 'dist/style.css'))

import esbuild from 'esbuild'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildPluginRenderer } from './scripts/build-plugin-renderer.mjs'

const isWatch = process.argv.includes('--watch')
const projectRoot = dirname(fileURLToPath(import.meta.url))

/** @type {esbuild.BuildOptions} */
const mainConfig = {
  absWorkingDir: projectRoot,
  entryPoints: [resolve(projectRoot, 'src/main.ts')],
  outfile: resolve(projectRoot, 'dist/main.js'),
  bundle: true,
  format: 'cjs',
  platform: 'node',
  target: 'es2022',
  logLevel: 'info',
  external: ['cruciblebox-plugin-api']
}

if (isWatch) {
  const mainContext = await esbuild.context(mainConfig)
  await Promise.all([mainContext.watch(), buildPluginRenderer({ projectRoot, watch: true })])
} else {
  await Promise.all([esbuild.build(mainConfig), buildPluginRenderer({ projectRoot })])
}

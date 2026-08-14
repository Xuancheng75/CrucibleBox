import esbuild from 'esbuild'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildPluginRenderer } from '../../scripts/build-plugin-renderer.mjs'

const isWatch = process.argv.includes('--watch')
const projectRoot = dirname(fileURLToPath(import.meta.url))
const residueWorkerEntry = resolve(projectRoot, 'src/workers/residue.worker.ts')

const residueWorkerSourcePlugin = {
  name: 'residue-worker-source',
  setup(build) {
    build.onResolve({ filter: /^openbox-residue-worker-source$/ }, () => ({
      path: 'openbox-residue-worker-source',
      namespace: 'residue-worker-source'
    }))
    build.onLoad({ filter: /.*/, namespace: 'residue-worker-source' }, async () => {
      const result = await esbuild.build({
        absWorkingDir: projectRoot,
        entryPoints: [residueWorkerEntry],
        bundle: true,
        format: 'iife',
        legalComments: 'none',
        minify: true,
        platform: 'browser',
        target: 'es2022',
        write: false
      })
      const source = result.outputFiles?.[0]?.text
      if (!source) throw new Error('residue worker build did not produce JavaScript')
      return {
        contents: `export default ${JSON.stringify(source)}`,
        loader: 'js',
        watchFiles: [residueWorkerEntry]
      }
    })
  }
}

/** @type {esbuild.BuildOptions} */
const mainConfig = {
  absWorkingDir: projectRoot,
  entryPoints: [resolve(projectRoot, 'src/main.ts')],
  outfile: resolve(projectRoot, 'dist/main.js'),
  bundle: true,
  format: 'cjs',
  platform: 'node',
  target: 'node18',
  external: ['cruciblebox-plugin-api']
}

if (isWatch) {
  const mainCtx = await esbuild.context(mainConfig)
  await Promise.all([
    mainCtx.watch(),
    buildPluginRenderer({
      projectRoot,
      watch: true,
      additionalPlugins: [residueWorkerSourcePlugin]
    })
  ])
  console.log('监听中...')
} else {
  await Promise.all([
    esbuild.build(mainConfig),
    buildPluginRenderer({ projectRoot, additionalPlugins: [residueWorkerSourcePlugin] })
  ])
  console.log('构建完成')
}

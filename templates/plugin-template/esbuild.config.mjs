import { build, context } from 'esbuild'

const watch = process.argv.includes('--watch')
const common = {
  bundle: true,
  sourcemap: false,
  target: 'es2022',
  logLevel: 'info'
}

const builds = [
  {
    ...common,
    entryPoints: ['src/main.ts'],
    outfile: 'dist/main.js',
    platform: 'node',
    format: 'cjs',
    external: ['openbox-plugin-api']
  },
  {
    ...common,
    entryPoints: ['src/renderer-entry.tsx'],
    outfile: 'dist/renderer.js',
    platform: 'browser',
    format: 'iife',
    jsx: 'automatic',
    define: { 'process.env.NODE_ENV': '"production"' }
  }
]

if (watch) {
  const contexts = await Promise.all(builds.map((options) => context(options)))
  await Promise.all(contexts.map((buildContext) => buildContext.watch()))
} else {
  await Promise.all(builds.map((options) => build(options)))
}

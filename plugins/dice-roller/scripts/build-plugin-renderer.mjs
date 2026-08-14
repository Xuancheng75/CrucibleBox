import esbuild from 'esbuild'
import { readFile } from 'node:fs/promises'
import { dirname, extname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const CSS_URL_PATTERN = /url\(\s*(['"]?)([^'"\)]+)\1\s*\)/g

const MIME_TYPES = {
  '.eot': 'application/vnd.ms-fontobject',
  '.otf': 'font/otf',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2'
}

function inlineCssAssetsPlugin() {
  return {
    name: 'inline-css-assets',
    setup(build) {
      build.onLoad({ filter: /\.css$/ }, async ({ path }) => {
        const source = await readFile(path, 'utf8')
        const matches = [...source.matchAll(CSS_URL_PATTERN)]
        let contents = source

        for (const match of matches) {
          const assetReference = match[2].trim()
          if (/^(?:data:|https?:|#)/i.test(assetReference)) continue

          const assetPath = resolve(dirname(path), assetReference.split(/[?#]/, 1)[0])
          const asset = await readFile(assetPath)
          const mimeType =
            MIME_TYPES[extname(assetPath).toLowerCase()] ?? 'application/octet-stream'
          const dataUrl = `data:${mimeType};base64,${asset.toString('base64')}`
          contents = contents.replace(match[0], `url("${dataUrl}")`)
        }

        return {
          contents: `export default ${JSON.stringify(contents)};`,
          loader: 'js'
        }
      })
    }
  }
}

export async function buildPluginRenderer({ projectRoot, watch = false, additionalPlugins = [] }) {
  const buildOptions = {
    absWorkingDir: projectRoot,
    entryPoints: [resolve(projectRoot, 'src/renderer-entry.tsx')],
    outfile: resolve(projectRoot, 'dist/renderer.js'),
    bundle: true,
    define: {
      'process.env.NODE_ENV': '"production"'
    },
    format: 'iife',
    jsx: 'automatic',
    legalComments: 'none',
    minify: true,
    platform: 'browser',
    plugins: [inlineCssAssetsPlugin(), ...additionalPlugins],
    target: 'es2022'
  }

  if (!watch) {
    await esbuild.build(buildOptions)
    return
  }

  const context = await esbuild.context(buildOptions)
  await context.watch()
  return context
}

const scriptPath = fileURLToPath(import.meta.url)
const invokedPath = process.argv[1] ? fileURLToPath(pathToFileURL(resolve(process.argv[1]))) : ''

if (scriptPath === invokedPath) {
  const projectArgument = process.argv.find(
    (argument, index) => index > 1 && !argument.startsWith('--')
  )
  const projectRoot = resolve(process.cwd(), projectArgument ?? '.')
  await buildPluginRenderer({ projectRoot, watch: process.argv.includes('--watch') })
}

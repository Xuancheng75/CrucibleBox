#!/usr/bin/env node
import { readFile, mkdir, stat, realpath, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { resolve, join, relative, dirname, isAbsolute } from 'node:path'
import * as installedEsbuild from 'esbuild'
import { pathToFileURL } from 'node:url'
import AdmZip from 'adm-zip'
import { validateManifest } from '@cruciblebox/next-api'

const VERSION = JSON.parse(
  await readFile(new URL('./package.json', import.meta.url), 'utf8')
).version
const args = process.argv.slice(2)
const workspaceFlag = args.find((x) => x.startsWith('--workspace-root='))
const [command, directory = '.', output] = args.filter((x) => x !== workspaceFlag)
if (command === '--version') {
  console.log(VERSION)
  process.exit(0)
}
if (!['build', 'pack'].includes(command))
  throw Error('Usage: cruciblebox-plugin-build <build|pack> [plugin-dir] [new-package.zip]')
const root = await realpath(resolve(directory))
const manifest = validateManifest(await readFile(join(root, 'plugin.json'), 'utf8'))
let workspaceModules
if (workspaceFlag) {
  const workspace = await realpath(resolve(workspaceFlag.slice('--workspace-root='.length)))
  const metadata = JSON.parse(await readFile(join(workspace, 'package.json'), 'utf8'))
  if (
    relative(workspace, root).replaceAll('\\', '/') !== 'plugins/' + manifest.id ||
    !metadata.workspaces?.includes('plugins/*')
  )
    throw Error('Plugin is not a declared direct workspace')
  workspaceModules = await realpath(join(workspace, 'node_modules'))
}
const compiler = workspaceModules
  ? await import(pathToFileURL(join(workspaceModules, 'esbuild/lib/main.js')).href)
  : installedEsbuild
if (command === 'build' && compiler.version !== '0.25.12')
  throw Error('Build requires pinned esbuild 0.25.12')
const build = compiler.build
async function inputFile(full) {
  const actual = await realpath(full)
  const pluginRel = relative(root, actual)
  const moduleRel = workspaceModules && relative(workspaceModules, actual)
  if (
    (!isAbsolute(pluginRel) && !pluginRel.startsWith('..')) ||
    (moduleRel !== undefined && !isAbsolute(moduleRel) && !moduleRel.startsWith('..'))
  ) {
    if ((await stat(actual)).isFile()) return actual
  }
  throw Error('Dependency escapes build boundary: ' + full)
}
function object(value, keys, label) {
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    Object.keys(value).some((k) => !keys.includes(k))
  )
    throw Error('Invalid ' + label)
}
function path(value, prefix) {
  if (
    typeof value !== 'string' ||
    value.length > 240 ||
    !/^[A-Za-z0-9_.\/-]+$/.test(value) ||
    !value.startsWith(prefix) ||
    value
      .split('/')
      .some(
        (x) =>
          !x ||
          x === '.' ||
          x === '..' ||
          x.endsWith('.') ||
          /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(x)
      )
  )
    throw Error('Invalid package-relative path: ' + value)
  return value
}
async function file(value) {
  const full = await realpath(join(root, value))
  const rel = relative(root, full)
  if (isAbsolute(rel) || rel.startsWith('..') || !(await stat(full)).isFile())
    throw Error('File escapes plugin root: ' + value)
  return full
}
async function entry(value, base) {
  if (value !== undefined) return path(value, 'src/')
  const candidates = ['mjs', 'ts', 'tsx', 'js', 'jsx']
    .map((ext) => `src/${base}.${ext}`)
    .filter((x) => existsSync(join(root, x)))
  if (candidates.length !== 1) throw Error('Expected exactly one source entry: ' + base)
  return candidates[0]
}
let config = { schemaVersion: 1 }
if (existsSync(join(root, 'plugin.build.json')))
  config = JSON.parse(await readFile(await file('plugin.build.json'), 'utf8'))
object(
  config,
  ['schemaVersion', 'renderer', 'backend', 'workers', 'assets', 'css', 'jsx', 'aliases'],
  'build configuration'
)
if (config.schemaVersion !== 1 || (!manifest.backend && config.backend !== undefined))
  throw Error('Build configuration/manifest mismatch')
if (config.css !== undefined && !['style', 'string'].includes(config.css))
  throw Error('Invalid CSS mode')
if (config.jsx !== undefined && config.jsx !== 'automatic') throw Error('Invalid JSX mode')
const aliases = {}
if (config.aliases !== undefined) {
  object(config.aliases, Object.keys(config.aliases), 'aliases')
  if (Object.keys(config.aliases).length > 32) throw Error('Too many aliases')
  for (const [name, value] of Object.entries(config.aliases)) {
    if (!/^(@[a-z0-9-]+\/)?[a-z0-9-]+$/.test(name)) throw Error('Invalid alias name')
    aliases[name] = await file(path(value, 'vendor/'))
  }
}
const jobs = [
  {
    source: await entry(config.renderer, 'renderer'),
    target: manifest.renderer,
    platform: 'browser',
    format: 'esm'
  }
]
if (manifest.backend)
  jobs.push({
    source: await entry(config.backend, 'main'),
    target: manifest.backend,
    platform: 'neutral',
    format: 'cjs'
  })
for (const [key, max] of [
  ['workers', 32],
  ['assets', 256]
]) {
  if (config[key] !== undefined && (!Array.isArray(config[key]) || config[key].length > max))
    throw Error('Invalid ' + key)
}
for (const worker of config.workers ?? []) {
  object(worker, ['source', 'target'], 'worker')
  const target = path(worker.target, 'dist/workers/')
  if (!target.endsWith('.js')) throw Error('Worker target must be JavaScript')
  jobs.push({ source: path(worker.source, 'src/'), target, platform: 'browser', format: 'iife' })
}
const assets = (config.assets ?? []).map((asset) => {
  object(asset, ['source', 'target'], 'asset')
  return { source: path(asset.source, 'assets/'), target: path(asset.target, 'dist/assets/') }
})
const targets = [...jobs, ...assets].map((x) => x.target)
if (new Set(targets.map((x) => x.toLowerCase())).size !== targets.length)
  throw Error('Duplicate output path')
for (const target of targets) {
  path(target, 'dist/')
  if (
    targets.some(
      (other) => other !== target && other.toLowerCase().startsWith(target.toLowerCase() + '/')
    )
  )
    throw Error('Output path collision')
  // An existing symlink or junction must never redirect writes outside the plugin.
  let parent = join(root, target)
  while (!existsSync(parent)) parent = dirname(parent)
  const rel = relative(root, await realpath(parent))
  if (isAbsolute(rel) || rel.startsWith('..')) throw Error('Output escapes plugin root')
}
if (command === 'build') {
  const prepared = []
  const inlineCss = {
    name: 'inline-renderer-css',
    setup(builder) {
      builder.onLoad({ filter: /\.css$/ }, async (args) => {
        await inputFile(args.path)
        let css = await readFile(args.path, 'utf8')
        if (/@import\b/i.test(css)) throw Error('CSS imports require explicit application handling')
        if (config.css !== 'string' && /url\s*\(/i.test(css))
          throw Error('CSS URLs require string mode')
        let total = Buffer.byteLength(css)
        for (const match of [...css.matchAll(/url\(\s*(['"]?)([^'"\)]+)\1\s*\)/g)]) {
          const reference = match[2].trim()
          if (reference.startsWith('#')) continue
          if (reference.startsWith('data:')) {
            if (
              !/^data:(font\/(woff2?|ttf|otf)|application\/(font-woff|vnd.ms-fontobject)|image\/svg\+xml);base64,[A-Za-z0-9+/]*={0,2}$/.test(
                reference
              )
            )
              throw Error('Unsupported CSS data URL')
            continue
          }
          if (/^[a-z][a-z0-9+.-]*:|^[/\\]/i.test(reference)) throw Error('External CSS URL denied')
          const resource = await inputFile(
            resolve(dirname(args.path), reference.split(/[?#]/, 1)[0])
          )
          const mime = {
            '.eot': 'application/vnd.ms-fontobject',
            '.otf': 'font/otf',
            '.svg': 'image/svg+xml',
            '.ttf': 'font/ttf',
            '.woff': 'font/woff',
            '.woff2': 'font/woff2'
          }[resource.slice(resource.lastIndexOf('.')).toLowerCase()]
          if (!mime || (await stat(resource)).size > 4 * 1024 * 1024)
            throw Error('Unsupported or oversized CSS resource')
          const bytes = await readFile(resource)
          total += bytes.length
          if (total > 16 * 1024 * 1024) throw Error('CSS resource budget exceeded')
          css = css.replace(
            match[0],
            'url("data:' + mime + ';base64,' + bytes.toString('base64') + '")'
          )
        }
        return {
          loader: 'js',
          contents:
            config.css === 'string'
              ? 'export default ' + JSON.stringify(css) + ';'
              : `const style=document.createElement('style');style.textContent=${JSON.stringify(css)};document.head.appendChild(style);`
        }
      })
    }
  }
  for (const job of jobs) {
    const result = await build({
      absWorkingDir: root,
      entryPoints: [await file(job.source)],
      outfile: join(root, job.target),
      bundle: true,
      format: job.format,
      platform: job.platform,
      target: 'es2022',
      alias: aliases,
      ...(config.jsx ? { jsx: config.jsx } : {}),
      define: { 'process.env.NODE_ENV': '"production"' },
      minify: true,
      write: false,
      metafile: true,
      legalComments: 'none',
      plugins: job.target === manifest.renderer ? [inlineCss] : []
    })
    for (const input of Object.keys(result.metafile.inputs)) {
      if (!input.startsWith('<')) await inputFile(resolve(root, input))
    }
    if (result.outputFiles.length !== 1) throw Error('Unexpected undeclared build output')
    prepared.push({ target: job.target, bytes: result.outputFiles[0].contents })
  }
  for (const asset of assets) {
    const source = await file(asset.source)
    if ((await stat(source)).size > 64 * 1024 * 1024) throw Error('Asset exceeds 64 MiB')
    prepared.push({ target: asset.target, bytes: await readFile(source) })
  }
  if (prepared.reduce((n, x) => n + x.bytes.length, 0) > 256 * 1024 * 1024)
    throw Error('Build exceeds 256 MiB')
  // Compile and validate every entry before touching existing output; no clean command.
  for (const item of prepared) {
    await mkdir(dirname(join(root, item.target)), { recursive: true })
    await writeFile(join(root, item.target), item.bytes)
  }
  console.log(`Built ${manifest.id}: ${prepared.length} declared files`)
} else {
  if (!output) throw Error('Pack requires a new output zip path')
  const target = resolve(output)
  if (existsSync(target)) throw Error('Refusing to replace existing package')
  const zip = new AdmZip()
  let size = 0
  for (const name of ['plugin.json', ...targets].sort()) {
    const full = await file(name)
    const bytes = (await stat(full)).size
    if (bytes > 64 * 1024 * 1024 || (size += bytes) > 256 * 1024 * 1024)
      throw Error('Package budget exceeded')
    zip.addFile(name, await readFile(full), '', 0o644)
    zip.getEntry(name).header.time = new Date(2000, 0, 1)
  }
  await mkdir(dirname(target), { recursive: true })
  await writeFile(target, zip.toBuffer(), { flag: 'wx' })
  console.log(`Packed ${manifest.id}: ${target}`)
}

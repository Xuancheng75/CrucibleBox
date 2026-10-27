#!/usr/bin/env node
import { readFile, mkdir, stat, realpath, writeFile, copyFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { resolve, join, relative, dirname, basename, isAbsolute } from 'node:path'
import * as installedEsbuild from 'esbuild'
import { pathToFileURL } from 'node:url'
import AdmZip from 'adm-zip'
import { createHash, createPrivateKey, sign } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import { validateManifest, validateResourceManifest } from '@cruciblebox/next2-api'

const VERSION = JSON.parse(
  await readFile(new URL('./package.json', import.meta.url), 'utf8')
).version
const args = process.argv.slice(2)
const workspaceFlag = args.find((x) => x.startsWith('--workspace-root='))
const sdkFlag = args.find((x) => x.startsWith('--native-sdk='))
const idFlag = args.find((x) => x.startsWith('--id='))
const keyFlag = args.find((x) => x.startsWith('--key-id='))
const [command, directory = '.', output] = args.filter(
  (x) => ![workspaceFlag, sdkFlag, idFlag, keyFlag].includes(x)
)
if (command === '--version') {
  console.log(VERSION)
  process.exit(0)
}
if (command === 'init') {
  const root = resolve(directory),
    kind = output ?? 'frontend'
  const id = idFlag?.slice('--id='.length) ?? basename(root)
  if (!/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/.test(id) || !['frontend', 'js', 'native'].includes(kind))
    throw Error('Choose a valid plugin ID and frontend|js|native kind')
  if (existsSync(root)) throw Error('Init requires a new directory')
  if (kind === 'native' && !sdkFlag)
    throw Error('Native init requires --native-sdk=<unpacked public Rust SDK>')
  const manifest = {
    manifestVersion: 6,
    sdkApiVersion: 6,
    id,
    version: '1.0.0',
    displayName: id,
    description: 'Independent plugin example',
    publisher: { id: 'local-developer', displayName: 'Local developer' },
    minHostVersion: '2.1.0-beta.4',
    sdk: { min: 6, maxExclusive: 7 },
    renderer: 'dist/renderer.js',
    permissions: ['storage:read', 'storage:write', 'tasks:read', 'tasks:control', 'tasks:execute'],
    background: false,
    contributes: { navigation: { title: id }, commands: [] },
    exports: [],
    dependencies: [],
    resources: []
  }
  if (kind === 'js') manifest.backend = { kind: 'js', entry: 'dist/main.js' }
  if (kind === 'native')
    manifest.backend = {
      kind: 'native',
      entry: 'dist/native/backend.exe',
      os: 'windows',
      arch: 'x86_64',
      trust: 'trusted-local-code'
    }
  const config = { schemaVersion: 1, renderer: 'src/renderer.mjs' }
  if (kind === 'js') config.backend = 'src/main.mjs'
  if (kind === 'native') config.native = { manifest: 'native/Cargo.toml', binary: 'plugin-backend' }
  await mkdir(join(root, 'src'), { recursive: true })
  await writeFile(join(root, 'plugin.source.json'), JSON.stringify(manifest, null, 2) + '\n')
  await writeFile(join(root, 'plugin.build.json'), JSON.stringify(config, null, 2) + '\n')
  await writeFile(
    join(root, 'package.json'),
    JSON.stringify(
      {
        name: 'plugin-' + id,
        version: '1.0.0',
        private: true,
        type: 'module',
        scripts: {
          build: 'cruciblebox-next2 build .',
          pack: 'cruciblebox-next2 pack . ' + id + '-1.0.0.zip'
        },
        dependencies: { '@cruciblebox/next2-api': '6.0.0-alpha.2' },
        devDependencies: { '@cruciblebox/next2-build': VERSION }
      },
      null,
      2
    ) + '\n'
  )
  const operation =
    kind === 'frontend'
      ? "await api.platform('storage.set',{key:'example.value',value:{numbers:[7,19]}}); const result = await api.platform('storage.get',{key:'example.value'})"
      : "const result = await api.backend('sum',{numbers:[7,19]})"
  await writeFile(
    join(root, 'src/renderer.mjs'),
    `import {createClient} from '@cruciblebox/next2-api'\nexport function mount(context) {\n const api=createClient(context), button=document.createElement('button'), status=document.createElement('p'); button.textContent='Run example'; status.setAttribute('role','status'); context.root.replaceChildren(button,status);\n const run=async()=>{button.disabled=true;try{${operation};status.textContent=JSON.stringify(result)}catch(error){status.textContent=String(error)}finally{button.disabled=false}};button.addEventListener('click',run);return()=>{button.removeEventListener('click',run);context.root.replaceChildren()};\n}\n`
  )
  if (kind === 'js')
    await writeFile(
      join(root, 'src/main.mjs'),
      "export function activate(){return {sum(params){if(!Array.isArray(params.numbers)||!params.numbers.every(Number.isSafeInteger))throw Object.assign(Error('Expected integers'),{code:'INVALID_NUMBERS'});const sum=params.numbers.reduce((a,b)=>a+b,0);if(!Number.isSafeInteger(sum))throw Object.assign(Error('Sum overflow'),{code:'OVERFLOW'});return sum}}}\n"
    )
  if (kind === 'native') {
    const sdk = await realpath(resolve(sdkFlag.slice('--native-sdk='.length)))
    const target = join(root, 'native/vendor/cruciblebox-next2-native')
    for (const name of [
      'Cargo.toml',
      'Cargo.lock',
      'LICENSE',
      'README.md',
      'src/lib.rs',
      'examples/arithmetic.rs'
    ]) {
      await mkdir(dirname(join(target, name)), { recursive: true })
      await copyFile(join(sdk, name), join(target, name))
    }
    await mkdir(join(root, 'native/src'), { recursive: true })
    await copyFile(join(sdk, 'examples/arithmetic.rs'), join(root, 'native/src/main.rs'))
    await writeFile(
      join(root, 'native/Cargo.toml'),
      '[package]\nname="plugin-backend"\nversion="1.0.0"\nedition="2021"\n[workspace]\n[dependencies]\ncruciblebox-next2-native={path="vendor/cruciblebox-next2-native"}\nserde_json="1"\n[profile.release]\nstrip=true\npanic="abort"\n'
    )
    const lock = spawnSync(
      'cargo',
      ['generate-lockfile', '--offline', '--manifest-path', join(root, 'native/Cargo.toml')],
      { stdio: 'inherit', windowsHide: true }
    )
    if (lock.error) throw lock.error
    if (lock.status !== 0)
      throw Error('Resolve native dependencies before building; project files retained')
  }
  console.log(`Created ${kind} plugin: ${root}`)
  process.exit(0)
}
if (command === 'sign') {
  if (!output || !keyFlag) throw Error('sign <package-dir> <private-key.pem> --key-id=<id>')
  const raw = await readFile(resolve(directory, 'plugin.json'))
  const manifest = validateManifest(raw.toString('utf8'))
  const key = createPrivateKey(await readFile(resolve(output)))
  if (key.asymmetricKeyType !== 'ed25519') throw Error('Signing requires Ed25519')
  await writeFile(
    resolve(directory, 'signature.json'),
    JSON.stringify(
      {
        signatureVersion: 1,
        publisherId: manifest.publisher.id,
        keyId: keyFlag.slice('--key-id='.length),
        signature: sign(null, raw, key).toString('base64')
      },
      null,
      2
    ) + '\n',
    { flag: 'wx' }
  )
  process.exit(0)
}
if (command === 'pack-resource') {
  if (!output) throw Error('pack-resource <resource-dir> <new-resource.zip>')
  const root = await realpath(resolve(directory))
  const raw = await readFile(join(root, 'resource.json'))
  const manifest = validateResourceManifest(raw.toString('utf8'))
  const zip = new AdmZip()
  zip.addFile('resource.json', raw)
  for (const entry of manifest.files) {
    const path = await realpath(join(root, entry.path))
    const rel = relative(root, path)
    if (isAbsolute(rel) || rel.startsWith('..')) throw Error('Resource file escapes package')
    const data = await readFile(path)
    if (
      data.length !== entry.bytes ||
      createHash('sha256').update(data).digest('hex') !== entry.sha256
    )
      throw Error('Resource inventory mismatch: ' + entry.path)
    zip.addFile(entry.path, data)
  }
  await writeFile(resolve(output), zip.toBuffer(), { flag: 'wx' })
  process.exit(0)
}
if (command === 'catalog') {
  if (!output) throw Error('catalog <release-spec.json> <new-catalog.json>')
  const spec = JSON.parse(await readFile(resolve(directory), 'utf8'))
  const plugins = []
  const ids = new Set()
  for (const entry of spec.plugins) {
    const data = await readFile(resolve(dirname(resolve(directory)), entry.package))
    const manifest = validateManifest(new AdmZip(data).readAsText('plugin.json'))
    if (ids.has(manifest.id)) throw Error('Duplicate plugin ID')
    ids.add(manifest.id)
    const url = new URL(entry.url)
    if (url.protocol !== 'https:' || url.username || url.password)
      throw Error('Release URL requires HTTPS without credentials')
    plugins.push({
      id: manifest.id,
      version: manifest.version,
      displayName: manifest.displayName,
      publisher: manifest.publisher.displayName,
      description: manifest.description,
      category: entry.category || '其他',
      tags: entry.tags || [],
      keywords: entry.keywords || [],
      highlights: entry.highlights || [],
      minHostVersion: manifest.minHostVersion,
      artifact: `${manifest.id}-${manifest.version}.zip`,
      size: data.length,
      sha256: createHash('sha256').update(data).digest('hex'),
      url: url.href
    })
  }
  await writeFile(resolve(output), JSON.stringify({ schemaVersion: 3, plugins }, null, 2) + '\n', {
    flag: 'wx'
  })
  process.exit(0)
}
if (!['build', 'pack'].includes(command))
  throw Error('Usage: cruciblebox-plugin-build <build|pack> [plugin-dir] [new-package.zip]')
const root = await realpath(resolve(directory))
const manifestFile = existsSync(join(root, 'plugin.source.json'))
  ? 'plugin.source.json'
  : 'plugin.json'
let manifest = JSON.parse(await readFile(join(root, manifestFile), 'utf8'))
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
  ['schemaVersion', 'renderer', 'backend', 'native', 'workers', 'assets', 'css', 'jsx', 'aliases'],
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
if (manifest.backend?.kind === 'js')
  jobs.push({
    source: await entry(config.backend, 'main'),
    target: manifest.backend.entry,
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
let native
if (manifest.backend?.kind === 'native') {
  object(config.native, ['manifest', 'binary'], 'native build')
  if (!/^[a-z][a-z0-9-]*$/.test(config.native.binary)) throw Error('Invalid native binary name')
  const cargoManifest = await file(path(config.native.manifest, 'native/'))
  native = {
    manifest: cargoManifest,
    source: join(dirname(cargoManifest), 'target/release', config.native.binary + '.exe'),
    target: manifest.backend.entry
  }
}
const targets = [...jobs, ...assets, ...(native ? [native] : [])].map((x) => x.target)
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
  validateManifest(
    JSON.stringify({
      ...manifest,
      files: targets.map((path) => ({ path, bytes: 0, sha256: '0'.repeat(64) }))
    })
  )
  if (native) {
    const compiler = spawnSync('rustc', ['-vV'], { cwd: root, encoding: 'utf8', windowsHide: true })
    if (compiler.error) throw compiler.error
    if (compiler.status !== 0) throw Error(compiler.stderr || 'rustc version failed')
    const target = process.env.CARGO_BUILD_TARGET || compiler.stdout.match(/^host: (.+)$/m)?.[1]
    if (!target) throw Error('Rust target unavailable')
    const metadata = spawnSync(
      'cargo',
      [
        'metadata',
        '--locked',
        '--offline',
        '--format-version=1',
        '--filter-platform',
        target,
        '--manifest-path',
        native.manifest
      ],
      { cwd: root, encoding: 'utf8', windowsHide: true }
    )
    if (metadata.error) throw metadata.error
    if (metadata.status !== 0) throw Error(metadata.stderr || 'Cargo metadata failed')
    for (const dependency of JSON.parse(metadata.stdout).packages) {
      if (dependency.source !== null) continue
      const location = await realpath(dirname(dependency.manifest_path))
      const rel = relative(root, location)
      if (isAbsolute(rel) || rel.startsWith('..'))
        throw Error('Local Cargo dependency escapes plugin: ' + location)
    }
    const result = spawnSync(
      'cargo',
      [
        'rustc',
        '--release',
        '--locked',
        '--offline',
        '--message-format=json-render-diagnostics',
        '--manifest-path',
        native.manifest,
        '--bin',
        config.native.binary,
        ...(target.endsWith('-windows-msvc') ? ['--', '-C', 'link-arg=/Brepro'] : [])
      ],
      {
        cwd: root,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'inherit'],
        windowsHide: true,
        maxBuffer: 16 * 1024 * 1024
      }
    )
    if (result.error) throw result.error
    if (result.status !== 0) throw Error('Native build failed')
    const artifacts = result.stdout
      .trim()
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line))
    native.source = artifacts.find(
      (item) =>
        item.reason === 'compiler-artifact' &&
        item.target.name === config.native.binary &&
        item.executable
    )?.executable
    if (!native.source) throw Error('Cargo did not produce the declared native executable')
  }
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
  if (native)
    prepared.push({
      target: native.target,
      bytes: await readFile(await file(relative(root, native.source).replaceAll('\\', '/')))
    })
  if (prepared.reduce((n, x) => n + x.bytes.length, 0) > 256 * 1024 * 1024)
    throw Error('Build exceeds 256 MiB')
  const files = prepared.map((item) => ({
    path: item.target,
    bytes: item.bytes.length,
    sha256: createHash('sha256').update(item.bytes).digest('hex')
  }))
  manifest = validateManifest(JSON.stringify({ ...manifest, files }))
  // Compile and validate every entry before touching existing output; no clean command.
  for (const item of prepared) {
    await mkdir(dirname(join(root, item.target)), { recursive: true })
    await writeFile(join(root, item.target), item.bytes)
  }
  await writeFile(join(root, 'plugin.json'), JSON.stringify(manifest, null, 2) + '\n')
  console.log(`Built ${manifest.id}: ${prepared.length} declared files`)
} else {
  manifest = validateManifest(await readFile(join(root, 'plugin.json'), 'utf8'))
  if (
    JSON.stringify([...manifest.files.map((file) => file.path)].sort()) !==
    JSON.stringify([...targets].sort())
  )
    throw Error('Manifest inventory differs from build outputs')
  if (!output) throw Error('Pack requires a new output zip path')
  const target = resolve(output)
  if (existsSync(target)) throw Error('Refusing to replace existing package')
  const zip = new AdmZip()
  let size = 0
  for (const name of [
    'plugin.json',
    ...(existsSync(join(root, 'signature.json')) ? ['signature.json'] : []),
    ...targets
  ].sort()) {
    const full = await file(name)
    const bytes = (await stat(full)).size
    if (bytes > 64 * 1024 * 1024 || (size += bytes) > 256 * 1024 * 1024)
      throw Error('Package budget exceeded')
    const data = await readFile(full)
    const entry = manifest.files.find((file) => file.path === name)
    if (
      entry &&
      (entry.bytes !== data.length ||
        entry.sha256 !== createHash('sha256').update(data).digest('hex'))
    )
      throw Error('Payload changed after build: ' + name)
    zip.addFile(name, data, '', 0o644)
    zip.getEntry(name).header.time = new Date(2000, 0, 1)
  }
  await mkdir(dirname(target), { recursive: true })
  await writeFile(target, zip.toBuffer(), { flag: 'wx' })
  console.log(`Packed ${manifest.id}: ${target}`)
}

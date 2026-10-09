import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  cpSync,
  symlinkSync,
  realpathSync
} from 'node:fs'
import { join, dirname, resolve } from 'node:path'
import { tmpdir } from 'node:os'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..')
const require = createRequire(import.meta.url)
const AdmZip = require('adm-zip')
function fixture() {
  const base = mkdtempSync(join(tmpdir(), 'cb-next-build-'))
  const tool = join(base, 'tool'),
    plugin = join(base, 'plugin')
  mkdirSync(join(tool, 'node_modules/@cruciblebox'), { recursive: true })
  cpSync(join(repo, 'packages/cruciblebox-plugin-build/cli.mjs'), join(tool, 'cli.mjs'))
  cpSync(join(repo, 'packages/cruciblebox-plugin-build/package.json'), join(tool, 'package.json'))
  cpSync(
    join(repo, 'packages/cruciblebox-next-api'),
    join(tool, 'node_modules/@cruciblebox/next-api'),
    { recursive: true, filter: (p) => !p.includes('node_modules') }
  )
  for (const name of ['esbuild', 'adm-zip'])
    symlinkSync(
      dirname(require.resolve(name + '/package.json')),
      join(tool, 'node_modules', name),
      'junction'
    )
  mkdirSync(join(plugin, 'src'), { recursive: true })
  const manifest = {
    id: 'build-fixture',
    version: '1.0.0',
    displayName: 'build fixture',
    manifestVersion: 5,
    sdkApiVersion: 5,
    wireVersion: 3,
    dataSchemaVersion: 1,
    renderer: 'dist/renderer.js',
    permissions: []
  }
  writeFileSync(join(plugin, 'plugin.json'), JSON.stringify(manifest))
  writeFileSync(
    join(plugin, 'src/renderer.tsx'),
    '/** @jsx h */\nfunction h(tag:string,props:unknown,text:string){return {tag,text}};export const view=<h1>fixture</h1>'
  )
  const run = (cmd = 'build', output) =>
    spawnSync(process.execPath, [join(tool, 'cli.mjs'), cmd, plugin, ...(output ? [output] : [])], {
      encoding: 'utf8',
      windowsHide: true
    })
  return {
    base,
    plugin,
    manifest,
    run,
    config: (value) =>
      writeFileSync(
        join(plugin, 'plugin.build.json'),
        JSON.stringify({ schemaVersion: 1, ...value })
      )
  }
}
test('CLI --version matches package.json metadata', () => {
  const f = fixture()
  const metadata = JSON.parse(
    readFileSync(join(repo, 'packages/cruciblebox-plugin-build/package.json'), 'utf8')
  )
  const result = f.run('--version')
  assert.equal(result.status, 0, result.stderr)
  assert.equal(result.stdout.trim(), metadata.version)
})
test('standalone TSX, inline CSS, workers and assets pack reproducibly without undeclared files', () => {
  const f = fixture()
  mkdirSync(join(f.plugin, 'assets'))
  writeFileSync(join(f.plugin, 'assets/data.json'), '{"kept":true}')
  writeFileSync(join(f.plugin, 'src/style.css'), '.fixture { color: var(--ob-color-text); }')
  writeFileSync(
    join(f.plugin, 'src/renderer.tsx'),
    'import "./style.css"; export const version: number = 1'
  )
  writeFileSync(join(f.plugin, 'src/worker.ts'), 'self.onmessage=()=>postMessage({done:true})')
  f.config({
    workers: [{ source: 'src/worker.ts', target: 'dist/workers/run.js' }],
    assets: [{ source: 'assets/data.json', target: 'dist/assets/data.json' }]
  })
  let r = f.run()
  assert.equal(r.status, 0, r.stderr)
  const first = readFileSync(join(f.plugin, 'dist/renderer.js'))
  assert.match(first.toString(), /createElement\("style"\)/)
  r = f.run()
  assert.equal(r.status, 0, r.stderr)
  assert.deepEqual(readFileSync(join(f.plugin, 'dist/renderer.js')), first)
  writeFileSync(join(f.plugin, 'dist/stale.js'), 'old data')
  const a = join(f.base, 'a.zip'),
    b = join(f.base, 'b.zip')
  assert.equal(f.run('pack', a).status, 0)
  assert.equal(f.run('pack', b).status, 0)
  assert.deepEqual(readFileSync(a), readFileSync(b))
  assert.deepEqual(
    new AdmZip(a)
      .getEntries()
      .map((x) => x.entryName)
      .sort(),
    ['dist/assets/data.json', 'dist/renderer.js', 'dist/workers/run.js', 'plugin.json']
  )
  assert.notEqual(f.run('pack', a).status, 0)
  assert.deepEqual(readFileSync(a), readFileSync(b))
})
test('invalid config, traversal and case-folded duplicate targets fail before replacing output', () => {
  const f = fixture()
  mkdirSync(join(f.plugin, 'dist'))
  writeFileSync(join(f.plugin, 'dist/renderer.js'), 'existing')
  writeFileSync(join(f.plugin, 'src/worker.ts'), 'postMessage(1)')
  for (const value of [
    { schemaVersion: 2 },
    { extra: true },
    { renderer: 'src/../escape.ts' },
    {
      workers: [
        { source: 'src/worker.ts', target: 'dist/workers/run.js' },
        { source: 'src/worker.ts', target: 'dist/workers/RUN.js' }
      ]
    },
    { assets: [{ source: 'assets/x', target: 'dist/assets/CON.txt' }] }
  ]) {
    f.config(value)
    assert.notEqual(f.run().status, 0)
    assert.equal(readFileSync(join(f.plugin, 'dist/renderer.js'), 'utf8'), 'existing')
  }
})
test('failed backend compilation does not overwrite an already built renderer', () => {
  const f = fixture()
  f.manifest.backend = 'dist/main.js'
  writeFileSync(join(f.plugin, 'plugin.json'), JSON.stringify(f.manifest))
  writeFileSync(
    join(f.plugin, 'src/main.ts'),
    'import missing from "./missing";export default missing'
  )
  mkdirSync(join(f.plugin, 'dist'))
  writeFileSync(join(f.plugin, 'dist/renderer.js'), 'existing')
  assert.notEqual(f.run().status, 0)
  assert.equal(readFileSync(join(f.plugin, 'dist/renderer.js'), 'utf8'), 'existing')
})
test('ambiguous entry, escaping relative imports, CSS URLs and linked output directories fail closed', () => {
  const f = fixture()
  writeFileSync(join(f.plugin, 'src/renderer.mjs'), 'export {}')
  assert.notEqual(f.run().status, 0)
  f.config({ renderer: 'src/renderer.tsx' })
  writeFileSync(join(f.base, 'outside.ts'), 'export const secret=1')
  writeFileSync(join(f.plugin, 'src/renderer.tsx'), 'export {secret} from "../../outside.ts"')
  assert.notEqual(f.run().status, 0)
  writeFileSync(join(f.plugin, 'src/style.css'), 'div{background:url(./missing.png)}')
  writeFileSync(join(f.plugin, 'src/renderer.tsx'), 'import "./style.css"')
  assert.notEqual(f.run().status, 0)
  writeFileSync(join(f.plugin, 'src/renderer.tsx'), 'export const okay=1')
  mkdirSync(join(f.base, 'outside'))
  symlinkSync(realpathSync(join(f.base, 'outside')), join(f.plugin, 'dist'), 'junction')
  assert.notEqual(f.run().status, 0)
})

test('string CSS embeds bounded local fonts and rejects network or escaping URLs before replacing output', () => {
  const f = fixture()
  writeFileSync(join(f.plugin, 'src/font.woff2'), Buffer.from('font-data'))
  writeFileSync(join(f.plugin, 'src/style.css'), '@font-face{src:url("./font.woff2")}')
  writeFileSync(
    join(f.plugin, 'src/renderer.tsx'),
    'import css from "./style.css";export const value=css'
  )
  f.config({ css: 'string' })
  let result = f.run()
  assert.equal(result.status, 0, result.stderr)
  const original = readFileSync(join(f.plugin, 'dist/renderer.js'))
  assert.match(original.toString(), /data:font\/woff2;base64,Zm9udC1kYXRh/)
  for (const reference of ['https://example.com/font.woff2', '../../outside.woff2']) {
    writeFileSync(join(f.base, 'outside.woff2'), 'private')
    writeFileSync(join(f.plugin, 'src/style.css'), '@font-face{src:url("' + reference + '")}')
    assert.notEqual(f.run().status, 0)
    assert.deepEqual(readFileSync(join(f.plugin, 'dist/renderer.js')), original)
  }
})
test('explicit workspace mode accepts only declared plugin dependencies under its node_modules', () => {
  const f = fixture(),
    workspace = join(f.base, 'workspace'),
    plugin = join(workspace, 'plugins', 'build-fixture')
  mkdirSync(plugin, { recursive: true })
  cpSync(f.plugin, plugin, { recursive: true })
  const dependency = join(workspace, 'node_modules', 'fixture-dep')
  mkdirSync(dependency, { recursive: true })
  writeFileSync(
    join(workspace, 'package.json'),
    JSON.stringify({ private: true, workspaces: ['plugins/*'] })
  )
  writeFileSync(
    join(dependency, 'package.json'),
    JSON.stringify({ name: 'fixture-dep', main: 'index.js' })
  )
  writeFileSync(join(dependency, 'index.js'), 'export const value=3')
  symlinkSync(
    dirname(require.resolve('esbuild/package.json')),
    join(workspace, 'node_modules/esbuild'),
    'junction'
  )
  writeFileSync(join(plugin, 'src/renderer.tsx'), 'export {value} from "fixture-dep"')
  const run = (flag) =>
    spawnSync(
      process.execPath,
      [join(f.base, 'tool/cli.mjs'), 'build', plugin, ...(flag ? [flag] : [])],
      { encoding: 'utf8', windowsHide: true }
    )
  assert.notEqual(run().status, 0)
  let result = run('--workspace-root=' + workspace)
  assert.equal(result.status, 0, result.stderr)
  writeFileSync(join(workspace, 'outside.ts'), 'export const privateValue=1')
  writeFileSync(join(plugin, 'src/renderer.tsx'), 'export {privateValue} from "../../outside.ts"')
  assert.notEqual(run('--workspace-root=' + workspace).status, 0)
})

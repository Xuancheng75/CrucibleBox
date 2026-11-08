import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, writeFileSync, cpSync, rmSync } from 'node:fs'
import { join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { checkImports, checkSdkCopies } from './check-source-boundaries.mjs'

test('source boundaries reject private aliases, SDK imports and copied SDK drift', () => {
  const temporary = fileURLToPath(new URL('../.workbench/temp/', import.meta.url))
  mkdirSync(temporary, { recursive: true })
  const fixture = mkdtempSync(join(temporary, 'source-boundaries-'))
  const host = join(fixture, 'host'),
    plugin = join(fixture, 'plugin')
  const write = (file, text) => {
    mkdirSync(resolve(file, '..'), { recursive: true })
    writeFileSync(file, text)
  }
  try {
    write(join(host, 'src-tauri/private.ts'), 'export const secret = 1')
    write(
      join(plugin, 'tsconfig.json'),
      JSON.stringify({
        compilerOptions: { baseUrl: '.', paths: { '@private/*': ['../host/src-tauri/*'] } }
      })
    )
    write(join(plugin, 'entry.ts'), "import {secret} from '@private/private'\nexport {secret}")
    assert.throws(
      () => checkImports(plugin, { plugin: true, root: host }),
      /plugin reaches host private source/
    )
    write(join(plugin, 'entry.ts'), 'export const own = true')
    assert.equal(checkImports(plugin, { plugin: true, root: host }), 0)
    const sdk = join(host, 'packages/cruciblebox-next2-api')
    write(join(sdk, 'src/index.mjs'), "export {secret} from '../../../src-tauri/private.js'")
    assert.throws(() => checkImports(sdk, { root: host }), /public SDK reaches host private source/)
    write(join(sdk, 'src/index.mjs'), 'export const api = true')
    write(
      join(sdk, 'package.json'),
      JSON.stringify({ name: '@cruciblebox/next2-api', version: '7.0.0-beta.3' })
    )
    for (const name of ['README.md', 'LICENSE']) write(join(sdk, name), name)
    cpSync(sdk, join(plugin, 'vendor/next2-api'), { recursive: true })
    assert.equal(checkSdkCopies(plugin, host), 4)
    write(join(plugin, 'vendor/next2-api/src/index.mjs'), 'export const drift = true')
    assert.throws(() => checkSdkCopies(plugin, host), /public SDK copy drift/)
    write(
      join(plugin, 'native/vendor/task-runtime/Cargo.toml'),
      '[package]\nname = "cruciblebox-task-runtime"\n'
    )
    write(join(plugin, 'vendor/next2-api/src/index.mjs'), 'export const api = true')
    assert.throws(() => checkSdkCopies(plugin, host), /host task infrastructure copied into plugin/)
  } finally {
    assert(resolve(fixture).startsWith(resolve(temporary) + sep))
    rmSync(fixture, { recursive: true })
  }
})

import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, readdirSync } from 'node:fs'
import { isAbsolute, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const crates = resolve(root, 'src-tauri/crates')
const nativeSdk = resolve(root, 'packages/cruciblebox-next2-native/Cargo.toml')
const app = resolve(root, 'src-tauri/Cargo.toml')
const runner = resolve(root, 'src-tauri/cruciblebox-plugin-host/Cargo.toml')
const core = readdirSync(crates, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => resolve(crates, entry.name, 'Cargo.toml'))
  .filter(existsSync)

for (const manifest of [nativeSdk, app, runner, ...core]) {
  const result = spawnSync(
    'cargo',
    [
      'metadata',
      '--format-version',
      '1',
      '--no-deps',
      '--locked',
      '--offline',
      '--manifest-path',
      manifest
    ],
    { cwd: root, encoding: 'utf8', windowsHide: true }
  )
  if (result.error) throw result.error
  assert.equal(result.status, 0, result.stderr)
  const entry = JSON.parse(result.stdout).packages.find(
    (item) => resolve(item.manifest_path) === manifest
  )
  assert(entry, `Missing Cargo package metadata for ${manifest}`)
  for (const dependency of entry.dependencies) {
    const location = `${entry.name} → ${dependency.name}`
    if (manifest !== app) {
      assert(!/^tauri(?:$|-)/.test(dependency.name), `${location} reaches the desktop layer`)
    }
    if (manifest === nativeSdk) {
      assert(dependency.source?.startsWith('registry+'), `${location} leaves the standalone SDK`)
    }
    if (dependency.path) {
      const local = relative(crates, dependency.path)
      assert(
        local && !isAbsolute(local) && local !== '..' && !local.startsWith(`..${sep}`),
        `${location} leaves platform crates`
      )
    }
  }
}

console.log(`Platform dependencies checked for ${core.length + 3} Cargo packages`)

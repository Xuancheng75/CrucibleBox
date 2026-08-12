import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  MAX_PLUGIN_MANIFEST_BYTES,
  assertPluginManifestInstallable,
  normalizePluginEntry,
  parsePluginManifest,
  readPluginManifest,
  validatePluginEntrypoints
} from '../plugin-system/PluginManifestPolicy'

const validManifest = {
  name: 'example-plugin',
  version: '1.2.3',
  displayName: 'Example',
  description: 'Example plugin',
  author: 'OpenBox',
  main: 'dist/main.js',
  renderer: 'dist/renderer.js',
  manifestVersion: 2,
  backendApiVersion: 2,
  rendererApiVersion: 2,
  permissions: ['database:read'],
  config: {
    mode: {
      type: 'select',
      label: 'Mode',
      default: 'safe',
      options: [{ label: 'Safe', value: 'safe' }]
    }
  }
}

describe('plugin manifest policy', () => {
  let testDir: string

  beforeEach(() => {
    testDir = mkdtempSync(join(tmpdir(), 'openbox-manifest-policy-'))
  })

  afterEach(() => {
    const resolved = resolve(testDir)
    const tempRoot = resolve(tmpdir())
    if (resolved.startsWith(`${tempRoot}\\`) || resolved.startsWith(`${tempRoot}/`)) {
      rmSync(resolved, { force: true, recursive: true })
    }
  })

  it('accepts all six production plugin manifests', () => {
    const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
    for (const pluginName of [
      'diary',
      'dice-roller',
      'gif-editor',
      'theme-manager',
      'turntable',
      'unienv'
    ]) {
      const root = join(repositoryRoot, 'plugins', pluginName)
      const manifest = readPluginManifest(root)
      expect(manifest.name).toBe(pluginName)
      expect(() => validatePluginEntrypoints(root, manifest)).not.toThrow()
    }
  })

  it('normalizes Windows separators but rejects absolute and traversal entries', () => {
    expect(normalizePluginEntry('dist\\main.js', 'main')).toBe('dist/main.js')
    for (const entry of [
      '../main.js',
      'dist/../main.js',
      '/main.js',
      'C:\\main.js',
      'dist//main.js'
    ]) {
      expect(() => normalizePluginEntry(entry, 'main')).toThrow('normalized relative')
    }
    expect(() => normalizePluginEntry('dist/main.txt', 'main')).toThrow('JavaScript')
  })

  it('rejects unknown fields, permissions, duplicate permissions and invalid versions', () => {
    expect(() => parsePluginManifest({ ...validManifest, surprise: true })).toThrow(
      'not a supported field'
    )
    expect(() =>
      parsePluginManifest({ ...validManifest, permissions: ['host:everything'] })
    ).toThrow('unknown')
    expect(() =>
      parsePluginManifest({ ...validManifest, permissions: ['database:read', 'database:read'] })
    ).toThrow('duplicated')
    expect(() => parsePluginManifest({ ...validManifest, version: '1.2' })).toThrow(
      'Invalid semantic version'
    )
    expect(() => parsePluginManifest({ ...validManifest, rendererApiVersion: 3 })).toThrow(
      'must be 1 or 2'
    )
    expect(() => parsePluginManifest({ ...validManifest, backendApiVersion: 3 })).toThrow(
      'must be 1 or 2'
    )
    expect(() => parsePluginManifest({ ...validManifest, manifestVersion: 3 })).toThrow(
      'must be 1 or 2'
    )
    expect(() => parsePluginManifest({ ...validManifest, rendererApiVersion: undefined })).toThrow(
      'version 2 requires'
    )
  })

  it('defaults legacy manifests to API v1 without rewriting them', () => {
    const legacyManifest: Partial<typeof validManifest> = { ...validManifest }
    delete legacyManifest.rendererApiVersion
    delete legacyManifest.backendApiVersion
    delete legacyManifest.manifestVersion
    expect(parsePluginManifest(legacyManifest).manifestVersion).toBeUndefined()
    expect(parsePluginManifest(legacyManifest).rendererApiVersion).toBeUndefined()
    expect(parsePluginManifest(legacyManifest).backendApiVersion).toBeUndefined()
    expect(parsePluginManifest(validManifest).rendererApiVersion).toBe(2)
    expect(parsePluginManifest(validManifest).backendApiVersion).toBe(2)
  })

  it('keeps legacy manifests readable but rejects them at the install boundary', () => {
    const legacyManifest: Partial<typeof validManifest> = { ...validManifest }
    delete legacyManifest.rendererApiVersion
    delete legacyManifest.backendApiVersion
    delete legacyManifest.manifestVersion
    const parsed = parsePluginManifest(legacyManifest)

    expect(() => assertPluginManifestInstallable(parsed)).toThrow(
      'legacy v1 packages can no longer be installed'
    )
    expect(() => assertPluginManifestInstallable(parsed, true)).not.toThrow()
  })

  it('accepts a renderer-only v2 manifest without a backend API contract', () => {
    const manifest = parsePluginManifest({
      ...validManifest,
      backend: false,
      backendApiVersion: undefined
    })

    expect(manifest.backend).toBe(false)
    expect(manifest.backendApiVersion).toBeUndefined()
    expect(() => parsePluginManifest({ ...validManifest, backend: 'false' })).toThrow(
      'must be a boolean'
    )
  })

  it('validates config shapes and default types', () => {
    expect(() =>
      parsePluginManifest({
        ...validManifest,
        config: { count: { type: 'number', label: 'Count', default: 'many' } }
      })
    ).toThrow('does not match')
    expect(() =>
      parsePluginManifest({
        ...validManifest,
        config: {
          mode: {
            type: 'select',
            label: 'Mode',
            options: [
              { label: 'One', value: 'same' },
              { label: 'Two', value: 'same' }
            ]
          }
        }
      })
    ).toThrow('duplicate')
  })

  it('requires both entrypoints to exist as regular files inside the plugin root', () => {
    mkdirSync(join(testDir, 'dist'))
    writeFileSync(join(testDir, 'dist', 'main.js'), 'module.exports = {}')
    const manifest = parsePluginManifest(validManifest)
    expect(() => validatePluginEntrypoints(testDir, manifest)).toThrow(
      'does not exist: dist/renderer.js'
    )

    writeFileSync(join(testDir, 'dist', 'renderer.js'), 'module.exports = {}')
    expect(() => validatePluginEntrypoints(testDir, manifest)).not.toThrow()
  })

  it('distinguishes a missing entrypoint from an entrypoint that is not a regular file', () => {
    mkdirSync(join(testDir, 'dist'))
    const manifest = parsePluginManifest(validManifest)

    expect(() => validatePluginEntrypoints(testDir, manifest)).toThrow(
      'does not exist: dist/main.js'
    )

    // A directory named dist/main.js exists but is not a regular file.
    mkdirSync(join(testDir, 'dist', 'main.js'))
    expect(() => validatePluginEntrypoints(testDir, manifest)).toThrow(
      'is not a regular file: dist/main.js'
    )

    rmSync(join(testDir, 'dist', 'main.js'), { force: true, recursive: true })
    writeFileSync(join(testDir, 'dist', 'main.js'), 'module.exports = {}')
    expect(() => validatePluginEntrypoints(testDir, manifest)).toThrow(
      'does not exist: dist/renderer.js'
    )
  })

  it('rejects oversized and malformed plugin.json files before installation', () => {
    writeFileSync(join(testDir, 'plugin.json'), 'x'.repeat(MAX_PLUGIN_MANIFEST_BYTES + 1))
    expect(() => readPluginManifest(testDir)).toThrow('exceeds')
    writeFileSync(join(testDir, 'plugin.json'), '{not-json')
    expect(() => readPluginManifest(testDir)).toThrow('not valid JSON')
  })
})

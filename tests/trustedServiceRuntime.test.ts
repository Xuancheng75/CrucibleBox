import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  calculateTrustedBundleDigest,
  TrustedServiceRuntime,
  type TrustedBundlePolicy
} from '../plugin-system/TrustedServiceRuntime'
import { Permission } from '../shared/types/permissions'
import type { PluginManifest } from '../shared/types/plugin.types'

const files = ['dist/main.js', 'dist/renderer.js', 'plugin.json'] as const
const roots: string[] = []

function fixture(): { root: string; manifest: PluginManifest; policy: TrustedBundlePolicy } {
  const root = mkdtempSync(join(tmpdir(), 'openbox-trusted-service-'))
  roots.push(root)
  mkdirSync(join(root, 'dist'))
  const manifest: PluginManifest = {
    name: 'unienv',
    version: 'test-version',
    displayName: 'UniEnv',
    description: '',
    author: 'OpenBox',
    main: 'dist/main.js',
    renderer: 'dist/renderer.js',
    manifestVersion: 2,
    backendApiVersion: 2,
    rendererApiVersion: 2,
    permissions: [Permission.TrustedUniEnv],
    config: {}
  }
  writeFileSync(join(root, 'plugin.json'), JSON.stringify(manifest))
  writeFileSync(join(root, 'dist', 'main.js'), 'trusted-main')
  writeFileSync(join(root, 'dist', 'renderer.js'), 'trusted-renderer')
  return {
    root,
    manifest,
    policy: {
      name: 'unienv',
      version: 'test-version',
      files,
      digest: calculateTrustedBundleDigest(root, files)
    }
  }
}

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { force: true, recursive: true })
})

describe('trusted service runtime', () => {
  it('pins the complete runtime file set and rejects tampering', () => {
    const { root, policy } = fixture()
    expect(calculateTrustedBundleDigest(root, files)).toBe(policy.digest)
    writeFileSync(join(root, 'dist', 'main.js'), 'tampered')
    expect(calculateTrustedBundleDigest(root, files)).not.toBe(policy.digest)
    writeFileSync(join(root, 'unexpected.js'), 'extra')
    expect(() => calculateTrustedBundleDigest(root, files)).toThrow('file set')
  })

  it('runs UniEnv only for an exactly pinned bundle', async () => {
    const { root, manifest, policy } = fixture()
    const runtime = new TrustedServiceRuntime({
      pluginId: 'unienv-id',
      pluginDirectory: root,
      manifest,
      config: { installRoot: 'C:\\UniEnv', downloadMirror: 'direct', customCombos: '[]' },
      logger: {
        debug: vi.fn(),
        error: vi.fn(),
        info: vi.fn(),
        warn: vi.fn()
      },
      policy
    })

    await expect(runtime.invoke('unienv', 'message', { type: 'listTools' })).rejects.toThrow(
      'not active'
    )
    await runtime.invoke('unienv', 'activate', null)
    await expect(runtime.invoke('unienv', 'message', { type: 'listTools' })).resolves.toHaveLength(
      5
    )
    await runtime.dispose()
    await expect(runtime.invoke('unienv', 'message', { type: 'listTools' })).rejects.toThrow(
      'disposed'
    )
  })

  it('does not authorize a manifest permission without the pinned digest', () => {
    const { root, manifest, policy } = fixture()
    expect(
      () =>
        new TrustedServiceRuntime({
          pluginId: 'unienv-id',
          pluginDirectory: root,
          manifest,
          config: {},
          logger: { debug: vi.fn(), error: vi.fn(), info: vi.fn(), warn: vi.fn() },
          policy: { ...policy, digest: '0'.repeat(64) }
        })
    ).toThrow('digest mismatch')
  })
})

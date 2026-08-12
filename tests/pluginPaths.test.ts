import { describe, it, expect } from 'vitest'
import { resolve } from 'path'
import { tmpdir } from 'os'
import { mkdirSync, writeFileSync, rmSync } from 'fs'
import { join } from 'path'
import { resolvePluginAssetPath } from '../plugin-system/pluginPaths'

describe('resolvePluginAssetPath', () => {
  const pluginsDir = join(tmpdir(), `openbox-test-${Date.now()}`)
  const pluginDir = join(pluginsDir, 'demo-plugin')
  mkdirSync(pluginDir, { recursive: true })
  writeFileSync(join(pluginDir, 'app.js'), 'console.log(1)')
  writeFileSync(join(pluginDir, 'data.txt'), 'hello')

  afterAll(() => {
    rmSync(pluginsDir, { recursive: true, force: true })
  })

  it('resolves files inside the plugin directory', () => {
    const p = resolvePluginAssetPath(pluginsDir, 'demo-plugin', '/app.js')
    expect(p).toBe(resolve(pluginDir, 'app.js'))
  })

  it('resolves nested paths safely', () => {
    const p = resolvePluginAssetPath(pluginsDir, 'demo-plugin', '/assets.json')
    expect(p).toBe(resolve(pluginDir, 'assets.json'))
  })

  it('rejects parent-directory traversal', () => {
    expect(resolvePluginAssetPath(pluginsDir, 'demo-plugin', '/../outside.txt')).toBeNull()
    expect(resolvePluginAssetPath(pluginsDir, 'demo-plugin', '/../../etc/passwd')).toBeNull()
  })

  it('rejects plugin name escaping the plugins dir', () => {
    expect(resolvePluginAssetPath(pluginsDir, '../outside', '/app.js')).toBeNull()
    expect(resolvePluginAssetPath(pluginsDir, '..', '/app.js')).toBeNull()
  })

  it('keeps percent-encoded segments literal (no traversal)', () => {
    const p = resolvePluginAssetPath(pluginsDir, 'demo-plugin', '/%2e%2e/outside.txt')
    expect(p).toBe(resolve(pluginDir, '%2e%2e', 'outside.txt'))
  })
})

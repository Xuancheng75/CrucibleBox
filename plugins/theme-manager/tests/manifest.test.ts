import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const pluginRoot = resolve(__dirname, '..')
const manifest = JSON.parse(readFileSync(resolve(pluginRoot, 'plugin.json'), 'utf8')) as {
  name: string
  version: string
  displayName: string
  description: string
  author: string
  main: string
  renderer: string
  manifestVersion: number
  backendApiVersion: number
  rendererApiVersion: number
  permissions: string[]
  config: Record<string, unknown>
}

describe('theme-manager manifest contract (SDK v2)', () => {
  it('declares the pinned identity', () => {
    expect(manifest.name).toBe('theme-manager')
    expect(manifest.version).toMatch(/^\d+\.\d+\.\d+$/)
    expect(manifest.displayName).toBeTruthy()
    expect(manifest.author).toBeTruthy()
  })

  it('uses Manifest v2 with both APIs pinned to v2', () => {
    expect(manifest.manifestVersion).toBe(2)
    expect(manifest.backendApiVersion).toBe(2)
    expect(manifest.rendererApiVersion).toBe(2)
  })

  it('uses the standard entry points and declares only theme:write', () => {
    expect(manifest.main).toBe('dist/main.js')
    expect(manifest.renderer).toBe('dist/renderer.js')
    expect(manifest.permissions).toEqual(['theme:write'])
  })

  it('keeps the description aligned with the current preset set', () => {
    expect(manifest.description).toContain('科幻面板')
    expect(manifest.description).toContain('零号城区')
    expect(manifest.description).not.toContain('赛博朋克')
  })

  it('declares a JSON config schema with a string default', () => {
    const schema = manifest.config.customThemes as { type: string; default: string }
    expect(schema.type).toBe('string')
    expect(schema.default).toBe('[]')
  })
})

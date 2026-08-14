import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  MAX_PLUGIN_RENDERER_SESSION_TTL_MS,
  PluginRendererSessionRegistry,
  type CreatePluginRendererSessionInput
} from '../plugin-system/PluginRendererSessionRegistry'

function token(index: number): string {
  return index.toString(16).padStart(64, '0')
}

describe('PluginRendererSessionRegistry', () => {
  let root: string
  let pluginDirectory: string
  let runtimePath: string
  let nextToken: number
  let now: number

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'openbox-renderer-session-'))
    pluginDirectory = join(root, 'plugin')
    mkdirSync(join(pluginDirectory, 'dist'), { recursive: true })
    writeFileSync(join(pluginDirectory, 'dist', 'renderer.js'), 'renderer')
    runtimePath = join(root, 'runtime.js')
    writeFileSync(runtimePath, 'runtime')
    nextToken = 1
    now = 10_000
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  function createRegistry(): PluginRendererSessionRegistry {
    return new PluginRendererSessionRegistry({
      now: () => now,
      randomToken: () => token(nextToken++)
    })
  }

  function input(overrides: Partial<CreatePluginRendererSessionInput> = {}) {
    return {
      pluginId: 'plugin-id',
      pluginName: 'demo-plugin',
      pluginDirectory,
      rendererEntry: 'dist/renderer.js',
      runtimePath,
      rendererApiVersion: 2 as const,
      permissions: ['database:read'],
      ownerWebContentsId: 42,
      ttlMs: 1_000,
      ...overrides
    }
  }

  it('issues independent cryptographic route and handshake tokens with a unique origin', () => {
    const registry = createRegistry()
    const permissions = ['database:read']
    const first = registry.create(input({ permissions }))
    permissions.push('shell:exec')
    const second = registry.create(input({ pluginId: 'second' }))

    expect(first.token).toBe(token(1))
    expect(first.handshakeToken).toBe(token(2))
    expect(first.token).not.toBe(first.handshakeToken)
    expect(first.origin).toBe(`cruciblebox-plugin://${token(1)}.session`)
    expect(first.indexUrl).toBe(`${first.origin}/index.html`)
    expect(second.origin).not.toBe(first.origin)
    expect(first.permissions).toEqual(['database:read'])
    expect(() => (first.permissions as string[]).push('file:read')).toThrow()
    expect(first.pluginDirectory).toBe(resolve(pluginDirectory))
    expect(first.ownerWebContentsId).toBe(42)
  })

  it('activates exactly once and enforces the owner on every access', () => {
    const registry = createRegistry()
    const session = registry.create(input())

    expect(registry.getActive(session.token, 42)).toEqual({
      ok: false,
      reason: 'not-active'
    })
    expect(registry.consumeIndex(session.token, 7)).toEqual({
      ok: false,
      reason: 'owner-mismatch'
    })
    const activated = registry.consumeIndex(session.token, 42)
    expect(activated.ok && activated.session.state).toBe('active')
    expect(activated.ok && activated.session.activatedAt).toBe(now)
    expect(registry.consumeIndex(session.token, 42)).toEqual({
      ok: false,
      reason: 'already-consumed'
    })
    expect(registry.getActive(session.token, 7)).toEqual({
      ok: false,
      reason: 'owner-mismatch'
    })
    expect(registry.getActive(session.token, 42).ok).toBe(true)
  })

  it('expires at the deadline and removes expired sessions', () => {
    const registry = createRegistry()
    const first = registry.create(input({ ttlMs: 10 }))
    const second = registry.create(input({ pluginId: 'second', ttlMs: 20 }))

    now += 10
    expect(registry.consumeIndex(first.token, 42)).toEqual({
      ok: false,
      reason: 'expired'
    })
    expect(registry.get(first.token, 42)).toEqual({ ok: false, reason: 'not-found' })
    expect(registry.cleanupExpired()).toBe(0)
    now += 10
    expect(registry.cleanupExpired()).toBe(1)
    expect(registry.get(second.token, 42)).toEqual({ ok: false, reason: 'not-found' })
  })

  it('disposes individual and owner-bound sessions without affecting other owners', () => {
    const registry = createRegistry()
    const first = registry.create(input())
    const second = registry.create(input({ pluginId: 'second' }))
    const otherOwner = registry.create(input({ pluginId: 'third', ownerWebContentsId: 99 }))

    expect(registry.dispose(first.token)).toBe(true)
    expect(registry.dispose(first.token)).toBe(false)
    expect(registry.disposeOwner(42)).toBe(1)
    expect(registry.get(second.token, 42)).toEqual({ ok: false, reason: 'not-found' })
    expect(registry.get(otherOwner.token, 99).ok).toBe(true)
    expect(registry.disposeOwner(99)).toBe(1)
  })

  it('never reissues a retired token and rejects invalid token generators', () => {
    const values = [token(1), token(2), token(1), token(2), token(3), token(4)]
    const registry = new PluginRendererSessionRegistry({
      now: () => now,
      randomToken: () => values.shift() ?? token(5)
    })
    const first = registry.create(input())
    registry.dispose(first.token)
    const second = registry.create(input({ pluginId: 'second' }))
    expect(second.token).toBe(token(3))
    expect(second.handshakeToken).toBe(token(4))

    const invalid = new PluginRendererSessionRegistry({ randomToken: () => 'not-a-token' })
    expect(() => invalid.create(input())).toThrow(/64 lowercase hexadecimal/)
  })

  it('rejects invalid lifetimes, owners, renderer paths and non-files', () => {
    const registry = createRegistry()
    expect(() => registry.create(input({ ttlMs: 0 }))).toThrow(/ttlMs/)
    expect(() => registry.create(input({ ttlMs: MAX_PLUGIN_RENDERER_SESSION_TTL_MS + 1 }))).toThrow(
      /ttlMs/
    )
    expect(() => registry.create(input({ ownerWebContentsId: 0 }))).toThrow(/ownerWebContentsId/)
    expect(() => registry.create(input({ rendererEntry: '../runtime.js' }))).toThrow(
      /rendererEntry/
    )
    expect(() => registry.create(input({ rendererEntry: 'dist/missing.js' }))).toThrow()
    expect(() => registry.create(input({ runtimePath: pluginDirectory }))).toThrow(/runtimePath/)
  })
})

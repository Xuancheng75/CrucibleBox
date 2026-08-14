import { randomBytes } from 'node:crypto'
import { lstatSync, realpathSync } from 'node:fs'
import { isAbsolute, relative, resolve, sep } from 'node:path'

export const PLUGIN_RENDERER_SCHEME = 'cruciblebox-plugin'
export const DEFAULT_PLUGIN_RENDERER_SESSION_TTL_MS = 30 * 60 * 1_000
export const MAX_PLUGIN_RENDERER_SESSION_TTL_MS = 24 * 60 * 60 * 1_000

const SESSION_TOKEN_PATTERN = /^[a-f0-9]{64}$/
const MAX_TOKEN_GENERATION_ATTEMPTS = 16

export type PluginRendererApiVersion = 1 | 2
export type PluginRendererSessionState = 'issued' | 'active'

export interface CreatePluginRendererSessionInput {
  pluginId: string
  pluginName: string
  pluginDirectory: string
  rendererEntry: string
  runtimePath: string
  rendererApiVersion: PluginRendererApiVersion
  permissions: readonly string[]
  ownerWebContentsId: number
  ttlMs?: number
}

export interface PluginRendererSession {
  readonly token: string
  readonly handshakeToken: string
  readonly origin: string
  readonly indexUrl: string
  readonly pluginId: string
  readonly pluginName: string
  readonly pluginDirectory: string
  readonly rendererEntry: string
  readonly rendererPath: string
  readonly runtimePath: string
  readonly rendererApiVersion: PluginRendererApiVersion
  readonly permissions: readonly string[]
  readonly ownerWebContentsId: number
  readonly createdAt: number
  readonly expiresAt: number
  readonly state: PluginRendererSessionState
  readonly activatedAt?: number
}

export type PluginRendererSessionDenialReason =
  'invalid-token' | 'not-found' | 'expired' | 'owner-mismatch' | 'not-active' | 'already-consumed'

export type PluginRendererSessionAccess =
  | { ok: true; session: PluginRendererSession }
  | { ok: false; reason: PluginRendererSessionDenialReason }

export interface PluginRendererSessionRegistryOptions {
  now?: () => number
  randomToken?: () => string
  defaultTtlMs?: number
}

interface MutablePluginRendererSession {
  token: string
  handshakeToken: string
  origin: string
  indexUrl: string
  pluginId: string
  pluginName: string
  pluginDirectory: string
  rendererEntry: string
  rendererPath: string
  runtimePath: string
  rendererApiVersion: PluginRendererApiVersion
  permissions: readonly string[]
  ownerWebContentsId: number
  createdAt: number
  expiresAt: number
  state: PluginRendererSessionState
  activatedAt?: number
}

function defaultRandomToken(): string {
  return randomBytes(32).toString('hex')
}

function assertNonEmpty(value: string, field: string): void {
  if (value.length === 0 || value !== value.trim()) {
    throw new Error(`${field} must be a non-empty trimmed string`)
  }
}

function assertOwnerWebContentsId(value: number): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error('ownerWebContentsId must be a positive safe integer')
  }
}

function canonicalRegularFile(path: string, field: string): string {
  const stats = lstatSync(path)
  if (!stats.isFile() || stats.isSymbolicLink()) {
    throw new Error(`${field} must be a regular non-symbolic-link file`)
  }
  return realpathSync(path)
}

function canonicalPluginDirectory(path: string): string {
  const stats = lstatSync(path)
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error('pluginDirectory must be a regular non-symbolic-link directory')
  }
  return realpathSync(path)
}

function resolveContainedRenderer(pluginDirectory: string, rendererEntry: string): string {
  if (
    rendererEntry.length === 0 ||
    rendererEntry.includes('\\') ||
    rendererEntry.startsWith('/') ||
    isAbsolute(rendererEntry) ||
    rendererEntry
      .split('/')
      .some((segment) => segment === '' || segment === '.' || segment === '..') ||
    !rendererEntry.endsWith('.js')
  ) {
    throw new Error('rendererEntry must be a normalized relative JavaScript path')
  }

  const candidate = resolve(pluginDirectory, ...rendererEntry.split('/'))
  const canonicalCandidate = canonicalRegularFile(candidate, 'rendererEntry')
  const candidateRelative = relative(pluginDirectory, canonicalCandidate)
  if (
    candidateRelative === '' ||
    candidateRelative === '..' ||
    candidateRelative.startsWith(`..${sep}`) ||
    isAbsolute(candidateRelative)
  ) {
    throw new Error('rendererEntry resolves outside pluginDirectory')
  }
  return canonicalCandidate
}

function snapshotSession(session: MutablePluginRendererSession): PluginRendererSession {
  return Object.freeze({
    ...session,
    permissions: session.permissions
  })
}

export class PluginRendererSessionRegistry {
  private readonly sessions = new Map<string, MutablePluginRendererSession>()
  private readonly usedTokens = new Set<string>()
  private readonly now: () => number
  private readonly randomToken: () => string
  private readonly defaultTtlMs: number

  constructor(options: PluginRendererSessionRegistryOptions = {}) {
    this.now = options.now ?? Date.now
    this.randomToken = options.randomToken ?? defaultRandomToken
    this.defaultTtlMs = options.defaultTtlMs ?? DEFAULT_PLUGIN_RENDERER_SESSION_TTL_MS
    this.assertTtl(this.defaultTtlMs)
  }

  create(input: CreatePluginRendererSessionInput): PluginRendererSession {
    assertNonEmpty(input.pluginId, 'pluginId')
    assertNonEmpty(input.pluginName, 'pluginName')
    assertOwnerWebContentsId(input.ownerWebContentsId)
    if (input.rendererApiVersion !== 1 && input.rendererApiVersion !== 2) {
      throw new Error('rendererApiVersion must be 1 or 2')
    }

    const ttlMs = input.ttlMs ?? this.defaultTtlMs
    this.assertTtl(ttlMs)
    const pluginDirectory = canonicalPluginDirectory(input.pluginDirectory)
    const rendererPath = resolveContainedRenderer(pluginDirectory, input.rendererEntry)
    const runtimePath = canonicalRegularFile(input.runtimePath, 'runtimePath')
    const token = this.issueToken()
    const handshakeToken = this.issueToken()
    const origin = `${PLUGIN_RENDERER_SCHEME}://${token}.session`
    const createdAt = this.now()
    const session: MutablePluginRendererSession = {
      token,
      handshakeToken,
      origin,
      indexUrl: `${origin}/index.html`,
      pluginId: input.pluginId,
      pluginName: input.pluginName,
      pluginDirectory,
      rendererEntry: input.rendererEntry,
      rendererPath,
      runtimePath,
      rendererApiVersion: input.rendererApiVersion,
      permissions: Object.freeze([...input.permissions]),
      ownerWebContentsId: input.ownerWebContentsId,
      createdAt,
      expiresAt: createdAt + ttlMs,
      state: 'issued'
    }
    this.sessions.set(token, session)
    return snapshotSession(session)
  }

  consumeIndex(token: string, ownerWebContentsId: number): PluginRendererSessionAccess {
    const access = this.access(token, ownerWebContentsId)
    if (!access.ok) return access
    if (access.session.state === 'active') {
      return { ok: false, reason: 'already-consumed' }
    }

    const session = this.sessions.get(token)
    if (!session) return { ok: false, reason: 'not-found' }
    session.state = 'active'
    session.activatedAt = this.now()
    return { ok: true, session: snapshotSession(session) }
  }

  getActive(token: string, ownerWebContentsId: number): PluginRendererSessionAccess {
    const access = this.access(token, ownerWebContentsId)
    if (!access.ok) return access
    if (access.session.state !== 'active') return { ok: false, reason: 'not-active' }
    return access
  }

  get(token: string, ownerWebContentsId: number): PluginRendererSessionAccess {
    return this.access(token, ownerWebContentsId)
  }

  dispose(token: string): boolean {
    if (!SESSION_TOKEN_PATTERN.test(token)) return false
    return this.sessions.delete(token)
  }

  disposeOwner(ownerWebContentsId: number): number {
    let disposed = 0
    for (const [token, session] of this.sessions) {
      if (session.ownerWebContentsId === ownerWebContentsId) {
        this.sessions.delete(token)
        disposed += 1
      }
    }
    return disposed
  }

  cleanupExpired(): number {
    const now = this.now()
    let disposed = 0
    for (const [token, session] of this.sessions) {
      if (now >= session.expiresAt) {
        this.sessions.delete(token)
        disposed += 1
      }
    }
    return disposed
  }

  private access(token: string, ownerWebContentsId: number): PluginRendererSessionAccess {
    if (!SESSION_TOKEN_PATTERN.test(token)) return { ok: false, reason: 'invalid-token' }
    const session = this.sessions.get(token)
    if (!session) return { ok: false, reason: 'not-found' }
    if (this.now() >= session.expiresAt) {
      this.sessions.delete(token)
      return { ok: false, reason: 'expired' }
    }
    if (session.ownerWebContentsId !== ownerWebContentsId) {
      return { ok: false, reason: 'owner-mismatch' }
    }
    return { ok: true, session: snapshotSession(session) }
  }

  private issueToken(): string {
    for (let attempt = 0; attempt < MAX_TOKEN_GENERATION_ATTEMPTS; attempt += 1) {
      const token = this.randomToken()
      if (!SESSION_TOKEN_PATTERN.test(token)) {
        throw new Error('randomToken must return 64 lowercase hexadecimal characters')
      }
      if (!this.usedTokens.has(token)) {
        this.usedTokens.add(token)
        return token
      }
    }
    throw new Error('Unable to generate a unique plugin renderer session token')
  }

  private assertTtl(ttlMs: number): void {
    if (!Number.isSafeInteger(ttlMs) || ttlMs <= 0 || ttlMs > MAX_PLUGIN_RENDERER_SESSION_TTL_MS) {
      throw new Error(
        `ttlMs must be an integer between 1 and ${MAX_PLUGIN_RENDERER_SESSION_TTL_MS}`
      )
    }
  }
}

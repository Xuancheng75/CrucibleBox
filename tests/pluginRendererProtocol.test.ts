import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  describePluginRendererRequest,
  registerPluginRendererProtocol,
  type PluginRendererProtocolRegistrar
} from '../plugin-system/PluginRendererProtocol'
import {
  PLUGIN_RENDERER_SCHEME,
  PluginRendererSessionRegistry,
  type PluginRendererApiVersion,
  type PluginRendererSession
} from '../plugin-system/PluginRendererSessionRegistry'

function token(index: number): string {
  return index.toString(16).padStart(64, '0')
}

describe('PluginRendererProtocol', () => {
  let root: string
  let pluginDirectory: string
  let runtimePath: string
  let registry: PluginRendererSessionRegistry
  let nextToken: number

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'openbox-renderer-protocol-'))
    pluginDirectory = join(root, 'plugin')
    mkdirSync(join(pluginDirectory, 'dist'), { recursive: true })
    mkdirSync(join(pluginDirectory, 'assets'), { recursive: true })
    writeFileSync(join(pluginDirectory, 'dist', 'renderer.js'), 'renderer-bundle')
    writeFileSync(join(pluginDirectory, 'dist', 'main.js'), 'backend-bundle')
    writeFileSync(join(pluginDirectory, 'assets', 'theme.css'), 'body{}')
    writeFileSync(join(pluginDirectory, 'assets', 'pixel.png'), Buffer.from([1, 2, 3]))
    writeFileSync(join(pluginDirectory, 'other.html'), '<script>bad()</script>')
    writeFileSync(join(pluginDirectory, 'plugin.json'), '{}')
    runtimePath = join(root, 'runtime.js')
    writeFileSync(runtimePath, 'host-runtime')
    nextToken = 1
    registry = new PluginRendererSessionRegistry({
      now: () => 1_000,
      randomToken: () => token(nextToken++)
    })
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  function createSession(version: PluginRendererApiVersion = 2): PluginRendererSession {
    return registry.create({
      pluginId: 'plugin-id',
      pluginName: 'demo-plugin',
      pluginDirectory,
      rendererEntry: 'dist/renderer.js',
      runtimePath,
      rendererApiVersion: version,
      permissions: ['database:read'],
      ownerWebContentsId: 42
    })
  }

  function request(
    session: PluginRendererSession,
    pathname: string,
    options: { method?: string; origin?: string; owner?: number } = {}
  ) {
    return describePluginRendererRequest(registry, {
      url: `${session.origin}${pathname}`,
      method: options.method ?? 'GET',
      headers: options.origin === undefined ? {} : { Origin: options.origin },
      ownerWebContentsId: options.owner ?? 42
    })
  }

  it('generates a one-time v2 index with an independent handshake token', () => {
    const session = createSession(2)
    const result = request(session, '/index.html')
    const html = String(result.body)

    expect(result.status).toBe(200)
    expect(result.headers['Content-Type']).toBe('text/html; charset=utf-8')
    expect(html).toContain(`data-session-token="${session.handshakeToken}"`)
    expect(html).toContain('data-api-version="2"')
    expect(html).toContain('data-renderer-url="/renderer.js"')
    expect(html).toContain('<script src="/runtime.js"></script>')
    expect(html).toContain('<script src="/renderer.js"></script>')
    expect(result.headers['Content-Security-Policy']).not.toContain("'unsafe-eval'")
    expect(result.headers['Content-Security-Policy']).toContain("connect-src 'none'")
    expect(result.headers['Content-Security-Policy']).toContain("worker-src 'self' blob:")
    expect(result.headers['Content-Security-Policy']).toContain("font-src 'self' data:")
    expect(result.headers['Content-Security-Policy']).toContain("frame-src 'none'")
    expect(result.headers['Content-Security-Policy']).toContain("object-src 'none'")
    expect(result.headers['Content-Security-Policy']).toContain("base-uri 'none'")
    expect(result.headers['Content-Security-Policy']).toContain("form-action 'none'")
    expect(result.headers['X-Content-Type-Options']).toBe('nosniff')
    expect(result.headers['Referrer-Policy']).toBe('no-referrer')
    expect(request(session, '/index.html').status).toBe(410)
  })

  it('keeps unsafe-eval and renderer loading confined to v1 compatibility', () => {
    const session = createSession(1)
    const result = request(session, '/index.html')
    const html = String(result.body)

    expect(result.status).toBe(200)
    expect(result.headers['Content-Security-Policy']).toContain("script-src 'self' 'unsafe-eval'")
    expect(result.headers['Content-Security-Policy']).toContain("connect-src 'self'")
    expect(html).toContain('<script src="/runtime.js"></script>')
    expect(html).not.toContain('<script src="/renderer.js"></script>')
  })

  it('requires activation and then serves only the runtime and declared renderer aliases', () => {
    const session = createSession()
    expect(request(session, '/runtime.js').status).toBe(403)
    expect(request(session, '/index.html').status).toBe(200)

    const runtime = request(session, '/runtime.js')
    const renderer = request(session, '/renderer.js')
    expect(runtime.status).toBe(200)
    expect(Buffer.from(runtime.body as ArrayBuffer).toString()).toBe('host-runtime')
    expect(renderer.status).toBe(200)
    expect(Buffer.from(renderer.body as ArrayBuffer).toString()).toBe('renderer-bundle')
    expect(renderer.headers['Content-Type']).toBe('application/javascript; charset=utf-8')
    expect(request(session, '/dist/renderer.js').status).toBe(404)
    expect(request(session, '/dist/main.js').status).toBe(404)
    expect(request(session, '/plugin.json').status).toBe(404)
    expect(request(session, '/other.html').status).toBe(404)
    expect(request(session, '/').status).toBe(404)
  })

  it('serves allowlisted assets but rejects traversal and symlink escapes', () => {
    const session = createSession()
    expect(request(session, '/index.html').status).toBe(200)
    expect(request(session, '/assets/theme.css').status).toBe(200)
    expect(request(session, '/assets/pixel.png').status).toBe(200)
    expect(request(session, '/../runtime.js').status).toBe(404)
    expect(request(session, '/assets/%2e%2e/%2e%2e/runtime.js').status).toBe(404)

    const outside = join(root, 'outside')
    mkdirSync(outside)
    writeFileSync(join(outside, 'escaped.css'), 'secret')
    symlinkSync(outside, join(pluginDirectory, 'assets', 'outside'), 'junction')
    expect(request(session, '/assets/outside/escaped.css').status).toBe(404)
  })

  it('rejects null and foreign origins without consuming the index ticket', () => {
    const session = createSession()
    expect(request(session, '/index.html', { origin: 'null' }).status).toBe(403)
    expect(request(session, '/index.html', { origin: 'https://attacker.example' }).status).toBe(403)
    expect(request(session, '/index.html', { owner: 7 }).status).toBe(403)
    expect(request(session, '/index.html').status).toBe(200)
    expect(
      request(session, '/runtime.js', {
        origin: session.origin
      }).status
    ).toBe(200)
  })

  it('rejects non-GET methods, malformed routes and query-bearing URLs', () => {
    const session = createSession()
    expect(request(session, '/index.html', { method: 'POST' }).status).toBe(405)
    expect(request(session, '/index.html?x=1').status).toBe(404)
    expect(
      describePluginRendererRequest(registry, {
        url: 'not a URL',
        method: 'GET',
        headers: {},
        ownerWebContentsId: 42
      }).status
    ).toBe(400)
    expect(
      describePluginRendererRequest(registry, {
        url: `https://${session.token}.session/index.html`,
        method: 'GET',
        headers: {},
        ownerWebContentsId: 42
      }).status
    ).toBe(404)
  })

  it('registers an Electron-compatible handler without importing Electron', async () => {
    const session = createSession()
    let scheme = ''
    let handler: ((request: Request) => Response | Promise<Response>) | undefined
    const protocol: PluginRendererProtocolRegistrar = {
      handle(registeredScheme, registeredHandler) {
        scheme = registeredScheme
        handler = registeredHandler
      }
    }
    registerPluginRendererProtocol({
      protocol,
      registry,
      getOwnerWebContentsId: () => 42
    })

    expect(scheme).toBe(PLUGIN_RENDERER_SCHEME)
    expect(handler).toBeDefined()
    const result = await handler!(new Request(session.indexUrl))
    expect(result.status).toBe(200)
    expect(await result.text()).toContain('data-api-version="2"')
  })
})

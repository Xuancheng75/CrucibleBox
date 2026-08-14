// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { closeSync, fstatSync, lstatSync, openSync, readSync, realpathSync } from 'node:fs'
import { extname, isAbsolute, relative, resolve, sep } from 'node:path'
import {
  PLUGIN_RENDERER_SCHEME,
  PluginRendererSessionRegistry,
  type PluginRendererSession
} from './PluginRendererSessionRegistry'

const MAX_RENDERER_RESOURCE_BYTES = 32 * 1024 * 1024
const SESSION_HOST_PATTERN = /^([a-f0-9]{64})\.session$/

const SAFE_ASSET_MIME_TYPES: Readonly<Record<string, string>> = Object.freeze({
  '.apng': 'image/apng',
  '.avif': 'image/avif',
  '.css': 'text/css; charset=utf-8',
  '.gif': 'image/gif',
  '.ico': 'image/x-icon',
  '.jpeg': 'image/jpeg',
  '.jpg': 'image/jpeg',
  '.mp3': 'audio/mpeg',
  '.mp4': 'video/mp4',
  '.ogg': 'audio/ogg',
  '.otf': 'font/otf',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.wasm': 'application/wasm',
  '.wav': 'audio/wav',
  '.webm': 'video/webm',
  '.webp': 'image/webp',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2'
})

export interface PluginRendererProtocolRequest {
  url: string
  method: string
  headers: Headers | Readonly<Record<string, string | undefined>>
  ownerWebContentsId: number | undefined
}

export interface PluginRendererProtocolResponseDescription {
  status: number
  headers: Readonly<Record<string, string>>
  body: string | ArrayBuffer
}

export interface PluginRendererProtocolRegistrar {
  handle(scheme: string, handler: (request: Request) => Response | Promise<Response>): void
}

export interface RegisterPluginRendererProtocolOptions {
  protocol: PluginRendererProtocolRegistrar
  registry: PluginRendererSessionRegistry
  getOwnerWebContentsId: (request: Request) => number | undefined
}

function contentSecurityPolicy(session?: PluginRendererSession): string {
  const scriptSource = session?.rendererApiVersion === 1 ? "'self' 'unsafe-eval'" : "'self'"
  const connectSource = session?.rendererApiVersion === 1 ? "'self'" : "'none'"
  return [
    "default-src 'none'",
    `script-src ${scriptSource}`,
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self' data:",
    "media-src 'self' data: blob:",
    "worker-src 'self' blob:",
    `connect-src ${connectSource}`,
    "frame-src 'none'",
    "child-src 'none'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'"
  ].join('; ')
}

function securityHeaders(
  contentType: string,
  session?: PluginRendererSession
): Readonly<Record<string, string>> {
  return Object.freeze({
    'Cache-Control': 'no-store, max-age=0',
    'Content-Security-Policy': contentSecurityPolicy(session),
    'Content-Type': contentType,
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Resource-Policy': contentType.startsWith('text/html')
      ? 'cross-origin'
      : 'same-origin',
    'Permissions-Policy':
      'camera=(), microphone=(), geolocation=(), payment=(), usb=(), serial=(), bluetooth=(), display-capture=(), fullscreen=(), clipboard-read=(), clipboard-write=()',
    'Referrer-Policy': 'no-referrer',
    'X-Content-Type-Options': 'nosniff'
  })
}

function response(
  status: number,
  body: string | ArrayBuffer,
  contentType = 'text/plain; charset=utf-8',
  session?: PluginRendererSession,
  extraHeaders: Readonly<Record<string, string>> = {}
): PluginRendererProtocolResponseDescription {
  return {
    status,
    headers: Object.freeze({ ...securityHeaders(contentType, session), ...extraHeaders }),
    body
  }
}

function headerValue(
  headers: PluginRendererProtocolRequest['headers'],
  name: string
): string | undefined {
  if (headers instanceof Headers) return headers.get(name) ?? undefined
  const lowerName = name.toLowerCase()
  for (const [key, value] of Object.entries(headers)) {
    if (key.toLowerCase() === lowerName) return value
  }
  return undefined
}

function generatedIndex(session: PluginRendererSession): string {
  const rendererScript =
    session.rendererApiVersion === 2 ? '\n    <script src="/renderer.js"></script>' : ''
  return `<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title></title>
  </head>
  <body>
    <div id="root" data-session-token="${escapeHtmlAttribute(session.handshakeToken)}" data-api-version="${session.rendererApiVersion}" data-renderer-url="/renderer.js"></div>
    <script src="/runtime.js"></script>${rendererScript}
  </body>
</html>`
}

function hasRawTraversal(url: string): boolean {
  const authorityStart = url.indexOf('://')
  if (authorityStart < 0) return false
  const pathStart = url.indexOf('/', authorityStart + 3)
  if (pathStart < 0) return false
  const rawPath = url.slice(pathStart).split(/[?#]/, 1)[0]
  let decoded: string
  try {
    decoded = decodeURIComponent(rawPath)
  } catch {
    return true
  }
  return (
    decoded.includes('\\') ||
    decoded.includes('\0') ||
    decoded.split('/').some((segment) => segment === '.' || segment === '..')
  )
}

function escapeHtmlAttribute(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
}

function isContained(root: string, candidate: string): boolean {
  const candidateRelative = relative(root, candidate)
  return (
    candidateRelative !== '' &&
    candidateRelative !== '..' &&
    !candidateRelative.startsWith(`..${sep}`) &&
    !isAbsolute(candidateRelative)
  )
}

function resolveSafeAsset(session: PluginRendererSession, pathname: string): string | null {
  let decoded: string
  try {
    decoded = decodeURIComponent(pathname)
  } catch {
    return null
  }
  if (
    !decoded.startsWith('/') ||
    decoded.includes('\0') ||
    decoded.includes('\\') ||
    decoded.split('/').some((segment) => segment === '.' || segment === '..')
  ) {
    return null
  }
  const relativePath = decoded.slice(1)
  const segments = relativePath.split('/')
  if (
    relativePath.length === 0 ||
    segments.some(
      (segment) =>
        segment.length === 0 ||
        segment === '.' ||
        segment === '..' ||
        Array.from(segment).some((character) => character.charCodeAt(0) <= 0x1f)
    ) ||
    !SAFE_ASSET_MIME_TYPES[extname(relativePath).toLowerCase()]
  ) {
    return null
  }

  const candidate = resolve(session.pluginDirectory, ...relativePath.split('/'))
  if (!isContained(session.pluginDirectory, candidate)) return null
  try {
    const stats = lstatSync(candidate)
    if (!stats.isFile() || stats.isSymbolicLink()) return null
    const canonicalCandidate = realpathSync(candidate)
    return isContained(session.pluginDirectory, canonicalCandidate) ? canonicalCandidate : null
  } catch {
    return null
  }
}

function readVerifiedFile(path: string): ArrayBuffer | null {
  let before
  try {
    before = lstatSync(path)
  } catch {
    return null
  }
  if (
    !before.isFile() ||
    before.isSymbolicLink() ||
    before.size < 0 ||
    before.size > MAX_RENDERER_RESOURCE_BYTES
  ) {
    return null
  }

  let descriptor: number
  try {
    descriptor = openSync(path, 'r')
  } catch {
    return null
  }
  try {
    const opened = fstatSync(descriptor)
    if (
      !opened.isFile() ||
      opened.dev !== before.dev ||
      opened.ino !== before.ino ||
      opened.size !== before.size ||
      opened.size > MAX_RENDERER_RESOURCE_BYTES
    ) {
      return null
    }
    const bytes = Buffer.alloc(opened.size)
    let offset = 0
    while (offset < bytes.length) {
      const bytesRead = readSync(descriptor, bytes, offset, bytes.length - offset, null)
      if (bytesRead === 0) return null
      offset += bytesRead
    }
    const after = fstatSync(descriptor)
    if (
      after.size !== opened.size ||
      after.mtimeMs !== opened.mtimeMs ||
      after.ctimeMs !== opened.ctimeMs
    ) {
      return null
    }
    return Uint8Array.from(bytes).buffer
  } finally {
    closeSync(descriptor)
  }
}

function denialStatus(reason: string): number {
  switch (reason) {
    case 'expired':
    case 'already-consumed':
      return 410
    case 'owner-mismatch':
    case 'not-active':
      return 403
    default:
      return 404
  }
}

export function describePluginRendererRequest(
  registry: PluginRendererSessionRegistry,
  request: PluginRendererProtocolRequest
): PluginRendererProtocolResponseDescription {
  if (request.method.toUpperCase() !== 'GET') {
    return response(405, 'Method Not Allowed', undefined, undefined, { Allow: 'GET' })
  }
  if (request.ownerWebContentsId === undefined) {
    return response(403, 'Forbidden')
  }
  if (hasRawTraversal(request.url)) return response(404, 'Not Found')

  let url: URL
  try {
    url = new URL(request.url)
  } catch {
    return response(400, 'Bad Request')
  }
  const hostMatch = SESSION_HOST_PATTERN.exec(url.hostname)
  if (
    url.protocol !== `${PLUGIN_RENDERER_SCHEME}:` ||
    !hostMatch ||
    url.port !== '' ||
    url.username !== '' ||
    url.password !== '' ||
    url.search !== '' ||
    url.hash !== ''
  ) {
    return response(404, 'Not Found')
  }

  const token = hostMatch[1]
  const origin = headerValue(request.headers, 'Origin')
  if (origin === 'null') return response(403, 'Forbidden')
  if (origin !== undefined && origin !== `${PLUGIN_RENDERER_SCHEME}://${url.hostname}`) {
    return response(403, 'Forbidden')
  }

  const isIndex = url.pathname === '/index.html'
  const access = isIndex
    ? registry.consumeIndex(token, request.ownerWebContentsId)
    : registry.getActive(token, request.ownerWebContentsId)
  if (!access.ok) {
    return response(denialStatus(access.reason), 'Not Found')
  }
  const session = access.session

  if (isIndex) {
    return response(200, generatedIndex(session), 'text/html; charset=utf-8', session)
  }

  let filePath: string | null
  let contentType: string | undefined
  if (url.pathname === '/runtime.js') {
    filePath = session.runtimePath
    contentType = 'application/javascript; charset=utf-8'
  } else if (url.pathname === '/renderer.js') {
    filePath = session.rendererPath
    contentType = 'application/javascript; charset=utf-8'
  } else {
    filePath = resolveSafeAsset(session, url.pathname)
    contentType = filePath ? SAFE_ASSET_MIME_TYPES[extname(filePath).toLowerCase()] : undefined
  }
  if (!filePath || !contentType) return response(404, 'Not Found', undefined, session)

  const body = readVerifiedFile(filePath)
  if (!body) return response(404, 'Not Found', undefined, session)
  return response(200, body, contentType, session)
}

export function registerPluginRendererProtocol({
  protocol,
  registry,
  getOwnerWebContentsId
}: RegisterPluginRendererProtocolOptions): void {
  protocol.handle(PLUGIN_RENDERER_SCHEME, (request) => {
    const description = describePluginRendererRequest(registry, {
      url: request.url,
      method: request.method,
      headers: request.headers,
      ownerWebContentsId: getOwnerWebContentsId(request)
    })
    return new Response(description.body, {
      status: description.status,
      headers: description.headers
    })
  })
}

import { existsSync, lstatSync, readFileSync } from 'node:fs'
import { extname, isAbsolute, relative, resolve, sep } from 'node:path'

export const HOST_RENDERER_SCHEME = 'openbox-app'
export const HOST_RENDERER_URL = `${HOST_RENDERER_SCHEME}://app/index.html`

const MAX_ASSET_BYTES = 32 * 1024 * 1024
const MIME_TYPES: Readonly<Record<string, string>> = Object.freeze({
  '.css': 'text/css; charset=utf-8',
  '.gif': 'image/gif',
  '.html': 'text/html; charset=utf-8',
  '.ico': 'image/x-icon',
  '.jpeg': 'image/jpeg',
  '.jpg': 'image/jpeg',
  '.js': 'application/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.webp': 'image/webp',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2'
})

interface ProtocolRegistrar {
  handle(scheme: string, handler: (request: Request) => Response | Promise<Response>): void
}

function hasRawTraversal(value: string): boolean {
  const authorityStart = value.indexOf('://')
  const pathStart = authorityStart < 0 ? -1 : value.indexOf('/', authorityStart + 3)
  if (pathStart < 0) return false
  const rawPath = value.slice(pathStart).split(/[?#]/u, 1)[0]
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

function isContained(root: string, candidate: string): boolean {
  const candidateRelative = relative(root, candidate)
  return (
    candidateRelative !== '' &&
    candidateRelative !== '..' &&
    !candidateRelative.startsWith(`..${sep}`) &&
    !isAbsolute(candidateRelative)
  )
}

export function resolveHostRendererAsset(rendererRoot: string, requestUrl: string): string | null {
  if (hasRawTraversal(requestUrl)) return null
  let url: URL
  try {
    url = new URL(requestUrl)
  } catch {
    return null
  }
  if (
    url.protocol !== `${HOST_RENDERER_SCHEME}:` ||
    url.hostname !== 'app' ||
    url.username !== '' ||
    url.password !== '' ||
    url.port !== '' ||
    url.search !== '' ||
    url.hash !== ''
  ) {
    return null
  }

  let pathname: string
  try {
    pathname = decodeURIComponent(url.pathname)
  } catch {
    return null
  }
  if (
    !pathname.startsWith('/') ||
    pathname.includes('\\') ||
    pathname.includes('\0') ||
    pathname.split('/').some((segment) => segment === '.' || segment === '..')
  ) {
    return null
  }
  const relativePath = pathname === '/' ? 'index.html' : pathname.slice(1)
  if (!relativePath || !MIME_TYPES[extname(relativePath).toLowerCase()]) return null
  const candidate = resolve(rendererRoot, ...relativePath.split('/'))
  return isContained(resolve(rendererRoot), candidate) ? candidate : null
}

function response(status: number, body: string | ArrayBuffer, contentType = 'text/plain'): Response {
  return new Response(body, {
    status,
    headers: {
      'Cache-Control': 'no-store, max-age=0',
      'Content-Security-Policy':
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: plugin:; connect-src 'self'; frame-src openbox-plugin:; object-src 'none'; base-uri 'none'; font-src 'self' data:",
      'Content-Type': contentType,
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Resource-Policy': 'same-origin',
      'Permissions-Policy':
        'camera=(), microphone=(), geolocation=(), payment=(), usb=(), serial=(), bluetooth=(), display-capture=(), fullscreen=()',
      'Referrer-Policy': 'no-referrer',
      'X-Content-Type-Options': 'nosniff'
    }
  })
}

export function registerHostRendererProtocol(
  protocol: ProtocolRegistrar,
  rendererRoot: string
): void {
  protocol.handle(HOST_RENDERER_SCHEME, (request) => {
    if (request.method.toUpperCase() !== 'GET') return response(405, 'Method Not Allowed')
    const assetPath = resolveHostRendererAsset(rendererRoot, request.url)
    if (!assetPath || !existsSync(assetPath)) return response(404, 'Not Found')
    const stats = lstatSync(assetPath)
    if (!stats.isFile() || stats.size < 0 || stats.size > MAX_ASSET_BYTES) {
      return response(404, 'Not Found')
    }
    const contentType = MIME_TYPES[extname(assetPath).toLowerCase()]
    if (!contentType) return response(404, 'Not Found')
    const bytes = Uint8Array.from(readFileSync(assetPath)).buffer
    return response(200, bytes, contentType)
  })
}

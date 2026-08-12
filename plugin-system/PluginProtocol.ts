import { protocol } from 'electron'
import { extname } from 'path'
import { existsSync, readFileSync } from 'fs'
import { resolvePluginAssetPath } from './pluginPaths'

const MIME_TYPES: Record<string, string> = {
  '.js': 'application/javascript',
  '.ts': 'application/x-typescript',
  '.jsx': 'application/javascript',
  '.tsx': 'application/x-typescript',
  '.css': 'text/css',
  '.html': 'text/html',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.map': 'application/json'
}

export class PluginProtocol {
  private static initialized = false
  private static pluginsDir = ''

  static register(pluginsDir?: string): void {
    if (this.initialized) return
    if (pluginsDir) {
      this.pluginsDir = pluginsDir
    }

    protocol.handle('plugin', (request) => {
      const url = new URL(request.url)
      const fullPath = resolvePluginAssetPath(PluginProtocol.pluginsDir, url.hostname, url.pathname)

      if (!fullPath || !existsSync(fullPath)) {
        return new Response('Not Found', { status: 404 })
      }

      const ext = extname(fullPath).toLowerCase()
      const mimeType = MIME_TYPES[ext] || 'application/octet-stream'
      const content = readFileSync(fullPath)

      const origin = request.headers.get('Origin') ?? ''
      // Allow the app's own window (dev: loopback HTTP, prod: openbox-app) and
      // null origin. Reject other cross-origin readers.
      const allowed =
        origin === '' ||
        origin === 'null' ||
        /^http:\/\/(localhost|127\.0\.0\.1)(:\d+)?$/u.test(origin) ||
        origin === 'openbox-app://app'

      if (!allowed) {
        return new Response('Forbidden', { status: 403 })
      }

      return new Response(content, {
        status: 200,
        headers: {
          'Content-Type': mimeType,
          'Access-Control-Allow-Origin': origin === '' ? 'openbox-app://app' : origin,
          'Cache-Control': 'no-cache'
        }
      })
    })

    this.initialized = true
  }

  static getPluginUrl(pluginName: string, filePath: string): string {
    return `plugin://${pluginName}/${filePath.replace(/\\/g, '/')}`
  }
}

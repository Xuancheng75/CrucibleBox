import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { HOST_RENDERER_URL, resolveHostRendererAsset } from '../electron/HostRendererProtocol'

describe('host renderer custom protocol', () => {
  const root = resolve('out/renderer')

  it('maps only the fixed app origin into the renderer root', () => {
    expect(resolveHostRendererAsset(root, HOST_RENDERER_URL)).toBe(resolve(root, 'index.html'))
    expect(resolveHostRendererAsset(root, 'openbox-app://app/assets/main.js')).toBe(
      resolve(root, 'assets/main.js')
    )
    expect(resolveHostRendererAsset(root, 'openbox-app://other/index.html')).toBeNull()
    expect(resolveHostRendererAsset(root, 'https://app/index.html')).toBeNull()
  })

  it('rejects traversal, credentials, ports, queries, and unsupported assets', () => {
    for (const candidate of [
      'openbox-app://app/../main/index.js',
      'openbox-app://app/%2e%2e/main/index.js',
      'openbox-app://app/%5c..%5cmain/index.js',
      'openbox-app://user:secret@app/index.html',
      'openbox-app://app:42/index.html',
      'openbox-app://app/index.html?debug=1',
      'openbox-app://app/secrets.txt'
    ]) {
      expect(resolveHostRendererAsset(root, candidate), candidate).toBeNull()
    }
  })

  it('rejects malformed encodings and origin lookalikes', () => {
    expect(resolveHostRendererAsset(root, 'openbox-app://app/%ZZ.js')).toBeNull()
    expect(resolveHostRendererAsset(root, 'openbox-app://app.evil/index.html')).toBeNull()
    expect(resolveHostRendererAsset(root, 'not a URL')).toBeNull()
  })
})

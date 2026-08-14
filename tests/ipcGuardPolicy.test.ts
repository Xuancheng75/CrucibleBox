import { describe, expect, it } from 'vitest'
import { isAllowedHostRendererUrl } from '../electron/ipc/ipcGuardPolicy'

describe('IPC sender URL policy', () => {
  it('accepts only the packaged host renderer entry when no dev server is configured', () => {
    const packagedUrl = 'openbox-app://app/index.html'
    expect(isAllowedHostRendererUrl(packagedUrl, '', packagedUrl)).toBe(true)
    expect(isAllowedHostRendererUrl(`${packagedUrl}#settings`, '', packagedUrl)).toBe(true)
    expect(isAllowedHostRendererUrl('openbox-app://other/index.html', '', packagedUrl)).toBe(false)
    expect(
      isAllowedHostRendererUrl('cruciblebox-plugin://session/index.html', '', packagedUrl)
    ).toBe(false)
  })

  it('accepts the configured development origin and rejects lookalikes', () => {
    const developmentUrl = 'http://127.0.0.1:5173/'
    expect(isAllowedHostRendererUrl('http://127.0.0.1:5173/src/main.tsx', developmentUrl)).toBe(
      true
    )
    expect(isAllowedHostRendererUrl('http://127.0.0.1:5174/', developmentUrl)).toBe(false)
    expect(isAllowedHostRendererUrl('http://127.0.0.1.evil.test:5173/', developmentUrl)).toBe(false)
  })

  it('rejects malformed candidates and malformed configured URLs', () => {
    expect(isAllowedHostRendererUrl('not a URL', '', 'openbox-app://app/index.html')).toBe(false)
    expect(isAllowedHostRendererUrl('http://127.0.0.1:5173/', 'not a URL')).toBe(false)
    expect(isAllowedHostRendererUrl('openbox-app://app/index.html', '', 'not a URL')).toBe(false)
    expect(isAllowedHostRendererUrl('openbox-app://app/index.html', '')).toBe(false)
  })
})

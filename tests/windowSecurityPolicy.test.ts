import { describe, expect, it } from 'vitest'
import {
  isAllowedExternalUrl,
  isAllowedHostNavigation,
  isChromiumPermissionAllowed
} from '../electron/windowSecurityPolicy'

describe('window security policy', () => {
  it('allows only credential-free HTTP(S) URLs to open externally', () => {
    expect(isAllowedExternalUrl('https://example.com/docs?q=1')).toBe(true)
    expect(isAllowedExternalUrl('http://127.0.0.1:8080/')).toBe(true)
    expect(isAllowedExternalUrl('https://user:secret@example.com/')).toBe(false)
    expect(isAllowedExternalUrl('file:///C:/Windows/System32/calc.exe')).toBe(false)
    expect(isAllowedExternalUrl('javascript:alert(1)')).toBe(false)
    expect(isAllowedExternalUrl('not a URL')).toBe(false)
  })

  it('keeps host navigation on the configured renderer origin', () => {
    const developmentUrl = 'http://127.0.0.1:5173/'
    expect(isAllowedHostNavigation('http://127.0.0.1:5173/plugins', developmentUrl)).toBe(true)
    expect(isAllowedHostNavigation('http://127.0.0.1:5174/', developmentUrl)).toBe(false)
    expect(isAllowedHostNavigation('data:text/html,host', developmentUrl)).toBe(false)
  })

  it('keeps packaged navigation on the host renderer entry', () => {
    const packagedUrl = 'openbox-app://app/index.html'
    expect(isAllowedHostNavigation(packagedUrl, '', packagedUrl)).toBe(true)
    expect(isAllowedHostNavigation('openbox-app://other/index.html', '', packagedUrl)).toBe(false)
  })

  it('denies Chromium permissions by default', () => {
    expect(isChromiumPermissionAllowed()).toBe(false)
  })
})

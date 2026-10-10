import { describe, expect, it } from 'vitest'
import { compareVersions } from '../tauri-frontend/src/features/marketplace/selectors/compareVersions'

describe('compareVersions', () => {
  it('preserves the Marketplace release and prerelease ordering behavior', () => {
    expect(compareVersions('1.2.3', '1.2.3')).toBe(0)
    expect(compareVersions('1.2.4', '1.2.3')).toBeGreaterThan(0)
    expect(compareVersions('2.1.0-beta.3', '2.1.0-beta.2')).toBeGreaterThan(0)
    expect(compareVersions('2.1.0-beta.3', '2.1.0')).toBeLessThan(0)
    expect(compareVersions('2.1.0-beta.10', '2.1.0-beta.2')).toBeGreaterThan(0)
    expect(compareVersions('v1.2.3', '1.2.3')).toBe(0)
  })
})

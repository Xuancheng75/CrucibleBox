import { describe, expect, it } from 'vitest'
import { compareVersions, parseSemVer } from '../plugin-system/semver'

describe('semantic version policy', () => {
  it('compares major, minor and patch versions numerically', () => {
    expect(compareVersions('2.0.0', '1.9.9')).toBeGreaterThan(0)
    expect(compareVersions('1.2.0', '1.1.9')).toBeGreaterThan(0)
    expect(compareVersions('1.1.2', '1.1.3')).toBeLessThan(0)
    expect(compareVersions('1.2.3', '1.2.3')).toBe(0)
  })

  it('implements SemVer prerelease precedence and ignores build metadata', () => {
    const ordered = [
      '1.0.0-alpha',
      '1.0.0-alpha.1',
      '1.0.0-alpha.beta',
      '1.0.0-beta',
      '1.0.0-beta.2',
      '1.0.0-beta.11',
      '1.0.0-rc.1',
      '1.0.0'
    ]
    expect([...ordered].sort(compareVersions)).toEqual(ordered)
    expect(compareVersions('1.0.0+build.1', '1.0.0+build.2')).toBe(0)
  })

  it.each(['1.2', '01.2.3', '1.0.0-', 'v1.2.3', '1.2.3evil', ''])(
    'rejects invalid version %j',
    (version) => {
      expect(() => parseSemVer(version)).toThrow('Invalid semantic version')
    }
  )

  it('rejects leading zeroes in numeric prerelease identifiers', () => {
    expect(() => parseSemVer('1.0.0-01')).toThrow('leading zeroes')
  })

  it('rejects numeric components outside the safe integer range', () => {
    expect(() => parseSemVer('9007199254740992.0.0')).toThrow('safe integer')
  })

  it('compares arbitrarily large numeric prerelease identifiers without Number rounding', () => {
    const lower = '1.0.0-900719925474099200000'
    const higher = '1.0.0-900719925474099200001'
    expect(compareVersions(lower, higher)).toBeLessThan(0)
    expect(compareVersions(higher, lower)).toBeGreaterThan(0)
  })
})

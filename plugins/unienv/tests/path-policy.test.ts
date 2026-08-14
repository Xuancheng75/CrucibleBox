import { describe, expect, it } from 'vitest'
import {
  MAX_INSTALL_PATH_LENGTH,
  PathPolicyError,
  canonicalizeInstallRoot,
  safeJoinVersionDirectory
} from '../../../plugin-system/trusted-services/unienv/path-policy'
import type { ToolId, ToolVersion } from '../../../plugin-system/trusted-services/unienv/protocol'

describe('canonicalizeInstallRoot', () => {
  it('canonicalizes drive case, separators, dot segments, and trailing separators', () => {
    expect(canonicalizeInstallRoot('c:/Dev Tools//UniEnv/./runtimes/')).toBe(
      'C:\\Dev Tools\\UniEnv\\runtimes'
    )
    expect(canonicalizeInstallRoot('d:\\开发环境\\UniEnv')).toBe('D:\\开发环境\\UniEnv')
  })

  it.each(['C:\\', 'd:/'])('rejects a drive root: %s', (input) => {
    expect(() => canonicalizeInstallRoot(input)).toThrowError(
      expect.objectContaining({ code: 'drive-root' })
    )
  })

  it.each([
    '\\\\server\\share\\UniEnv',
    '//server/share/UniEnv',
    '\\\\?\\C:\\UniEnv',
    '\\\\.\\C:\\UniEnv',
    '\\??\\C:\\UniEnv',
    '\\Device\\HarddiskVolume1\\UniEnv'
  ])('rejects UNC and device namespace path: %s', (input) => {
    expect(() => canonicalizeInstallRoot(input)).toThrowError(
      expect.objectContaining({ code: 'namespace-path' })
    )
  })

  it.each(['UniEnv', '.\\UniEnv', '\\UniEnv', 'C:UniEnv'])('rejects relative path: %s', (input) => {
    expect(() => canonicalizeInstallRoot(input)).toThrowError(
      expect.objectContaining({ code: 'relative-path' })
    )
  })

  it.each(['C:\\safe\\..\\UniEnv', 'C:/safe/../UniEnv'])('rejects traversal: %s', (input) => {
    expect(() => canonicalizeInstallRoot(input)).toThrowError(
      expect.objectContaining({ code: 'path-traversal' })
    )
  })

  it('rejects control characters and surrounding whitespace', () => {
    expect(() => canonicalizeInstallRoot('C:\\Uni\u0000Env')).toThrowError(
      expect.objectContaining({ code: 'invalid-path' })
    )
    expect(() => canonicalizeInstallRoot(' C:\\UniEnv')).toThrowError(
      expect.objectContaining({ code: 'invalid-path' })
    )
    expect(() => canonicalizeInstallRoot('C:\\UniEnv\\folder ')).toThrowError(
      expect.objectContaining({ code: 'invalid-segment' })
    )
  })

  it.each([
    'C:\\UniEnv\\NUL',
    'C:\\UniEnv\\con.txt',
    'C:\\UniEnv\\folder.',
    'C:\\UniEnv\\bad&calc',
    "C:\\UniEnv\\bad'quote"
  ])('rejects ambiguous, reserved, or shell-active segment: %s', (input) => {
    expect(() => canonicalizeInstallRoot(input)).toThrowError(
      expect.objectContaining({ code: 'invalid-segment' })
    )
  })

  it('rejects an overlong installation root', () => {
    expect(() =>
      canonicalizeInstallRoot(`C:\\${'x'.repeat(MAX_INSTALL_PATH_LENGTH)}`)
    ).toThrowError(expect.objectContaining({ code: 'path-limit' }))
  })
})

describe('safeJoinVersionDirectory', () => {
  it('joins a supported tool and version below the canonical installation root', () => {
    expect(safeJoinVersionDirectory('c:/UniEnv/', 'python', '3.11.9')).toBe(
      'C:\\UniEnv\\python\\3.11.9'
    )
    expect(safeJoinVersionDirectory('D:\\Dev Tools\\UniEnv', 'java', '21.0.5')).toBe(
      'D:\\Dev Tools\\UniEnv\\java\\21.0.5'
    )
  })

  it('rejects an unknown tool even when passed through an unsafe cast', () => {
    expect(() =>
      safeJoinVersionDirectory('C:\\UniEnv', 'ruby' as ToolId, '3.3.0' as ToolVersion)
    ).toThrowError(expect.objectContaining({ code: 'unknown-tool' }))
  })

  it.each(['99.0.0', '..', '3.11.9\\..\\escape'])(
    'rejects unsafe or unknown version: %s',
    (version) => {
      expect(() =>
        safeJoinVersionDirectory('C:\\UniEnv', 'python', version as ToolVersion)
      ).toThrowError(expect.objectContaining({ code: 'unknown-version' }))
    }
  )

  it('validates the root before joining', () => {
    expect(() => safeJoinVersionDirectory('C:\\', 'python', '3.11.9')).toThrowError(PathPolicyError)
    expect(() =>
      safeJoinVersionDirectory('\\\\server\\share\\UniEnv', 'python', '3.11.9')
    ).toThrowError(expect.objectContaining({ code: 'namespace-path' }))
  })
})

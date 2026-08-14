import { describe, expect, it } from 'vitest'
import { DEFAULT_CUSTOM, FALLBACK_TOKENS, buildCustomTheme, isValidTheme } from '../src/renderer'

describe('theme-manager renderer logic', () => {
  it('exports a complete set of fallback semantic tokens', () => {
    for (const key of [
      'colorPrimary',
      'colorPrimaryHover',
      'colorPrimaryBg',
      'colorBg',
      'colorBgLayout',
      'colorBgContainer',
      'colorText',
      'colorBorder',
      'colorSuccess',
      'colorWarning',
      'colorError',
      'colorLink'
    ]) {
      expect(FALLBACK_TOKENS[key]).toMatch(/^#[0-9a-fA-F]{6}$/)
    }
  })

  it('builds a valid custom theme in light mode with overridden colors', () => {
    const theme = buildCustomTheme(undefined, 'light', {
      primary: '#123456',
      bg: '#abcdef',
      container: '#ffffff',
      text: '#111111',
      border: '#dddddd'
    })

    expect(isValidTheme(theme)).toBe(true)
    expect(theme.id).toBe('custom')
    expect(theme.mode).toBe('light')
    expect(theme.tokens.colorPrimary).toBe('#123456')
    expect(theme.tokens.colorBgLayout).toBe('#abcdef')
    expect(theme.tokens.colorBgContainer).toBe('#ffffff')
    // 未覆盖的语义 token 从 fallback 继承
    expect(theme.tokens.colorError).toBe(FALLBACK_TOKENS.colorError)
  })

  it('keeps dark-mode background when mode is dark', () => {
    const theme = buildCustomTheme(undefined, 'dark', DEFAULT_CUSTOM)
    expect(theme.mode).toBe('dark')
    expect(theme.tokens.colorBg).toBe('#141414')
  })

  it('validates theme shapes strictly', () => {
    expect(isValidTheme(null)).toBe(false)
    expect(isValidTheme({ id: 'x' })).toBe(false)
    expect(isValidTheme({ id: 'x', name: 'y', mode: 'dark', tokens: { a: 1 } })).toBe(true)
    expect(isValidTheme({ id: 'x', name: 'y', mode: 'neon', tokens: {} })).toBe(false)
  })
})

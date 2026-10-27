import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import {
  PRESET_THEMES,
  DEFAULT_THEME,
  isPresetTheme,
  getPresetTheme,
  createLightTokens,
  createDarkTokens
} from '../shared/themes/presets'
import { themeToCssVars, getThemeCssVarKeys } from '../shared/themes/css-vars'
import { normalizeTheme } from '../shared/themes/normalize'
import cyberThemeJson from '../themes/cyber.json'

const globalCss = ['base', 'cyber', 'neon']
  .map((part) =>
    readFileSync(resolve(__dirname, `../tauri-frontend/src/styles/${part}.css`), 'utf-8')
  )
  .join('\n')
function extractRule(css: string, selector: string): string | null {
  const idx = css.indexOf(selector)
  if (idx === -1) return null
  const start = css.indexOf('{', idx)
  if (start === -1) return null
  let depth = 0
  for (let i = start; i < css.length; i++) {
    if (css[i] === '{') depth++
    else if (css[i] === '}') {
      depth--
      if (depth === 0) return css.slice(idx, i + 1)
    }
  }
  return null
}

describe('Preset themes', () => {
  it('provides at least one theme and a default', () => {
    expect(PRESET_THEMES.length).toBeGreaterThan(0)
    expect(DEFAULT_THEME).toBe(PRESET_THEMES[0])
  })

  it('every preset has a unique id and a valid mode', () => {
    const ids = new Set(PRESET_THEMES.map((t) => t.id))
    expect(ids.size).toBe(PRESET_THEMES.length)
    for (const t of PRESET_THEMES) {
      expect(['light', 'dark']).toContain(t.mode)
    }
  })

  it('dark preset uses dark backgrounds', () => {
    const dark = PRESET_THEMES.find((t) => t.mode === 'dark')
    expect(dark).toBeDefined()
    expect(dark!.tokens.colorBgContainer).toBe('#171a21')
  })

  it('isPresetTheme distinguishes preset ids', () => {
    expect(isPresetTheme('light')).toBe(true)
    expect(isPresetTheme('dark')).toBe(true)
    expect(isPresetTheme('cyber')).toBe(false)
    expect(isPresetTheme('custom-id-123')).toBe(false)
  })

  it('resolves canonical preset data by id', () => {
    expect(getPresetTheme('dark')).toBe(PRESET_THEMES.find((t) => t.id === 'dark'))
    expect(getPresetTheme('custom-id-123')).toBeUndefined()
  })

  it('token factories produce complete token sets', () => {
    const light = createLightTokens('#555', '#666', '#eee')
    const dark = createDarkTokens('#4096ff', '#69b1ff', '#123')
    expect(light.colorPrimary).toBe('#555')
    expect(dark.colorPrimary).toBe('#4096ff')
    expect(light.borderRadius).toBe(10)
    expect(typeof light.fontFamily).toBe('string')
  })

  it('migrates partial legacy custom themes onto a complete semantic token set', () => {
    const normalized = normalizeTheme({
      id: 'legacy-custom',
      name: 'Legacy',
      mode: 'dark',
      tokens: { colorPrimary: '#123456', borderRadius: 99, unknown: 'ignored' }
    })
    expect(normalized?.tokens.colorPrimary).toBe('#123456')
    expect(normalized?.tokens.colorBgContainer).toBe('#171a21')
    expect(normalized?.tokens.borderRadius).toBe(32)
    expect(normalized?.tokens).not.toHaveProperty('unknown')
  })

  it('drops unsafe custom token values while preserving safe defaults', () => {
    const normalized = normalizeTheme({
      id: 'safe-custom',
      name: 'Safe',
      mode: 'light',
      tokens: { colorBg: 'url(javascript:alert(1))', fontFamily: 'sans-serif; color:red' }
    })
    expect(normalized?.tokens.colorBg).toBe('#fafafa')
    expect(normalized?.tokens.fontFamily).not.toContain(';')
  })
})

describe('Theme CSS contract', () => {
  it('distributable cyber JSON name matches preset', () => {
    expect(cyberThemeJson.id).toBe('cyber')
    expect(cyberThemeJson.name).toBe('科幻面板')
  })

  it('cyber HUD strip uses symmetric corner cuts like the search bar', () => {
    const rule = extractRule(globalCss, "[data-ob-theme='cyber'] .ob-hud-strip {")
    expect(rule).toBeTruthy()
    expect(rule).toContain('clip-path: polygon(')
    expect(rule).not.toContain('border-left:')
    expect(rule).not.toContain('border-right:')
    expect(rule).not.toContain('rgba(255, 0, 60')
  })

  it('cyber HUD strip keeps a restrained dark translucent background', () => {
    const rule = extractRule(globalCss, "[data-ob-theme='cyber'] .ob-hud-strip {")
    expect(rule).toContain('color-mix(in srgb, var(--ob-color-bg-container)')
    expect(rule).not.toMatch(/linear-gradient\s*\(/)
  })

  it('cyber HUD strip text triggers ob-glitch on hover without moving the bar', () => {
    const rule = extractRule(globalCss, "[data-ob-theme='cyber'] .ob-hud-strip:hover > span {")
    expect(rule).toBeTruthy()
    expect(rule).toContain('animation: ob-glitch')
    expect(rule).not.toContain('transform:')
  })

  it('cyber HUD strip hover glitch applies to both text spans simultaneously', () => {
    const rule = extractRule(globalCss, "[data-ob-theme='cyber'] .ob-hud-strip:hover > span {")
    expect(rule).toBeTruthy()
    expect(rule).not.toContain(':first-child')
    expect(rule).not.toContain(':last-child')
    expect(rule).toContain('animation: ob-glitch')
  })

  it('cyber theme removes the bottom-right red square decoration', () => {
    expect(globalCss).not.toContain("[data-ob-theme='cyber'] .ob-main-content::after")
  })

  it('cyber theme removes the rose bottom-right ambient glow', () => {
    const rule = extractRule(globalCss, "[data-ob-theme='cyber'] body,")
    expect(rule).toBeTruthy()
    expect(rule).not.toContain('at 100% 100%')
    expect(rule).not.toContain('rgba(255, 0, 60, 0.11)')
  })

  it('neon-district selected rail button cuts the top-right corner instead of bottom-right', () => {
    const rule = extractRule(
      globalCss,
      "[data-ob-theme='neon-district'] .ob-rail-btn[data-active='true'] {"
    )
    expect(rule).toBeTruthy()
    expect(rule).toContain('clip-path: polygon(')
    // Old bottom-right cut is gone.
    expect(rule).not.toContain('100% calc(100% - 10px)')
    expect(rule).not.toContain('calc(100% - 10px) 100%')
    // New top-right cut is present.
    expect(rule).toContain('calc(100% - 10px) 0')
    expect(rule).toContain('100% 10px')
    // Left side and remaining corners stay unchanged.
    expect(rule).toContain('0 calc(100% - 10px)')
    expect(rule).toContain('10px 100%')
    expect(rule).toContain('100% 100%')
  })
})

describe('themeToCssVars', () => {
  it('emits mode and theme id variables', () => {
    const vars = themeToCssVars(normalizeTheme(PRESET_THEMES[0])!)
    expect(vars['--ob-mode']).toBe(PRESET_THEMES[0].mode)
    expect(vars['--ob-theme-id']).toBe(PRESET_THEMES[0].id)
  })

  it('renders numeric tokens as pixels', () => {
    const vars = themeToCssVars(normalizeTheme(PRESET_THEMES[0])!)
    expect(vars['--ob-radius']).toBe('10px')
  })

  it('renders color tokens verbatim', () => {
    const vars = themeToCssVars(normalizeTheme(PRESET_THEMES[0])!)
    expect(vars['--ob-color-primary']).toBe(PRESET_THEMES[0].tokens.colorPrimary)
    expect(vars['--ob-colorPrimary']).toBe(PRESET_THEMES[0].tokens.colorPrimary)
  })

  it('getThemeCssVarKeys returns the full expected set', () => {
    const keys = getThemeCssVarKeys()
    expect(keys).toContain('--ob-mode')
    expect(keys).toContain('--ob-color-bg-container')
    expect(keys).toContain('--ob-colorBgContainer')
    expect(keys).toContain('--ob-font-family')
  })
})

it('primary button text follows beta3 theme contrast rather than fixed white', () => {
  const cyber = themeToCssVars(normalizeTheme(cyberThemeJson)!)
  const light = themeToCssVars(
    normalizeTheme({
      ...DEFAULT_THEME,
      tokens: { ...DEFAULT_THEME.tokens, colorPrimary: '#123456' }
    })!
  )
  expect(cyber['--ob-color-primary-contrast']).toBe('#000000')
  expect(light['--ob-color-primary-contrast']).toBe('#ffffff')
  expect(getThemeCssVarKeys()).toContain('--ob-color-primary-contrast')
})

test('all beta3 preset font stacks survive render-only normalization', () => {
  for (const theme of PRESET_THEMES)
    expect(normalizeTheme(theme)?.tokens.fontFamily).toBe(theme.tokens.fontFamily)
})

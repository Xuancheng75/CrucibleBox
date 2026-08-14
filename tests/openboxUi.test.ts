import { describe, expect, it } from 'vitest'
import {
  themeColorVar,
  themeFontFamilyVar,
  themeRadiusVar
} from '../plugins/theme-manager/src/theme-vars'

describe('theme-manager theme primitives (inlined @openbox/ui)', () => {
  it('references the canonical semantic CSS variables', () => {
    expect(themeColorVar('bg-container', '#fff')).toBe('var(--ob-color-bg-container, #fff)')
    expect(themeRadiusVar()).toBe('var(--ob-radius, 8px)')
    expect(themeFontFamilyVar('system-ui')).toBe('var(--ob-font-family, system-ui)')
  })
})

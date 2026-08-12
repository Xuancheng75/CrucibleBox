import { describe, expect, it } from 'vitest'
import { themeColorVar, themeFontFamilyVar, themeRadiusVar } from '../packages/openbox-ui/src/index'

describe('@openbox/ui theme primitives', () => {
  it('references the canonical semantic CSS variables', () => {
    expect(themeColorVar('bg-container', '#fff')).toBe('var(--ob-color-bg-container, #fff)')
    expect(themeRadiusVar()).toBe('var(--ob-radius, 8px)')
    expect(themeFontFamilyVar('system-ui')).toBe('var(--ob-font-family, system-ui)')
  })
})

import { describe, expect, it } from 'vitest'
import { wheelLabelColorFor } from '../src/wheel-label-color'

describe('wheelLabelColorFor', () => {
  it('uses dark text over bright sectors including the cyber cyan accent', () => {
    expect(wheelLabelColorFor('#00e5ff', '#fff')).toBe('#000')
    expect(wheelLabelColorFor('#ffffff', '#fff')).toBe('#000')
    expect(wheelLabelColorFor('#2f78c2', '#fff')).toBe('#000')
  })

  it('uses white text over mid-gray sectors', () => {
    expect(wheelLabelColorFor('#6b6b6b', '#000')).toBe('#fff')
  })

  it('uses white text over dark sectors', () => {
    expect(wheelLabelColorFor('#123456', '#000')).toBe('#fff')
  })

  it('supports shorthand hex colors', () => {
    expect(wheelLabelColorFor('#abc', '#fff')).toBe('#000')
    expect(wheelLabelColorFor('#123', '#000')).toBe('#fff')
  })

  it('uses the supplied theme contrast fallback for non-hex persisted colors', () => {
    expect(wheelLabelColorFor('var(--plugin-sector-color)', '#001018')).toBe('#001018')
  })
})

import { describe, expect, it } from 'vitest'
import { DEFAULT_THEME } from '../shared/themes/presets'
import { normalizeTheme } from '../shared/themes/normalize'
import { compileThemeStyle, effectiveMotion } from '../shared/themes/style'
import type { ThemeAsset, ThemeDesign, ToolboxTheme } from '../shared/types/theme.types'

const makeTheme = (design: ThemeDesign): ToolboxTheme => ({
  ...DEFAULT_THEME,
  id: 'neutral-design',
  design
})
const assetId = '1'.repeat(64)
const image: ThemeAsset = {
  id: assetId,
  name: 'sample.png',
  kind: 'image',
  mime: 'image/png',
  bytes: 128,
  url: `http://cruciblebox-theme.localhost/${'a'.repeat(64)}/${assetId}`,
  license: 'User-created'
}

describe('versioned generic theme design', () => {
  it('preserves legacy data and validates new design fields without guessing unknown values', () => {
    expect(normalizeTheme(DEFAULT_THEME)).toEqual(DEFAULT_THEME)
    const theme = makeTheme({
      version: 2,
      layout: { navigation: 'menu' },
      motion: { level: 'off' }
    })
    expect(normalizeTheme(theme)).toEqual(theme)
    expect(normalizeTheme({ ...theme, design: { version: 3 } })).toBeNull()
    expect(
      normalizeTheme({ ...theme, design: { version: 2, layout: { navigation: 'unknown' } } })
    ).toBeNull()
    expect(normalizeTheme({ ...theme, design: { version: 2, css: 'x'.repeat(8193) } })).toBeNull()
  })

  it('scopes selectors and namespaces keyframe references while preserving easing keywords', () => {
    const result = compileThemeStyle(
      makeTheme({
        version: 2,
        css: ':root{--sample:1} body,.card{color:red;animation:rise 1s ease}@keyframes rise{to{opacity:1}}'
      })
    )
    expect(result.css).toContain('[data-ob-theme-version="2"]{--sample:1}')
    expect(result.css).toContain(
      '[data-ob-theme-version="2"] body,[data-ob-theme-version="2"] .card'
    )
    expect(result.css).toContain('@keyframes cbx-user-rise')
    expect(result.css).toContain('animation:cbx-user-rise 1s ease')
  })

  it.each([
    ':root::view-transition-old(cbx-page other){opacity:0}',
    ':root::view-transition-old(var(--name)){opacity:0}',
    ':root::unknown-pseudo(cbx-page){opacity:0}',
    '@import "https://example.com/a.css";',
    '@\\69mport "https://example.com/a.css";',
    '.x{background:url(https://example.com/a.png)}',
    '.x{background:image-set("https://example.com/a.png" 1x)}',
    '.x{width:expression(alert(1))}',
    '@font-face{font-family:x;src:url(asset:abc)}',
    '.x{animation:missing 1s}',
    '.x{animation-name:var(--unscoped)}'
  ])('rejects uncontained or unresolved CSS: %s', (source) => {
    expect(() => compileThemeStyle(makeTheme({ version: 2, css: source }))).toThrow()
  })

  it('scopes a bounded view-transition name and namespaces its animation', () => {
    const result = compileThemeStyle(
      makeTheme({
        version: 2,
        css: ':root::view-transition-old(cbx-page){animation:leave 1s}@keyframes leave{to{transform:translateY(-100%)}}'
      })
    )
    expect(result.css).toContain('[data-ob-theme-version="2"]::view-transition-old(cbx-page)')
    expect(result.css).toContain('animation:cbx-user-leave')
  })

  it('resolves only declared managed asset URLs and refuses mismatched image/font roles', () => {
    const theme = makeTheme({
      version: 2,
      assets: [assetId],
      css: `.x{background:url("asset:${assetId}")}`
    })
    expect(compileThemeStyle(theme, [image]).css).toContain(image.url)
    expect(() => compileThemeStyle(theme)).toThrow('THEME_ASSET_MISSING')
    expect(() =>
      compileThemeStyle(makeTheme({ ...theme.design!, fonts: [{ assetId, family: 'Sample' }] }), [
        image
      ])
    ).toThrow('THEME_FONT_ASSET_REQUIRED')
    expect(
      normalizeTheme(
        makeTheme({
          version: 2,
          layers: [
            {
              id: 'image',
              type: 'image',
              scope: 'shell',
              x: 0,
              y: 0,
              width: 100,
              height: 100,
              rotation: 0,
              opacity: 1,
              order: 0,
              assetId
            }
          ]
        })
      )
    ).toBeNull()
  })

  it('respects reduced motion and completely clears v2 rules when returning to a legacy theme', () => {
    const theme = makeTheme({
      version: 2,
      motion: { level: 'full', transition: 'slide' },
      css: '.x{animation:rise 1s;transition:transform 1s}@keyframes rise{to{opacity:1}}'
    })
    const reduced = compileThemeStyle(theme, [], true)
    expect(effectiveMotion('off', true)).toBe('off')
    expect(reduced.motion).toBe('reduced')
    expect(compileThemeStyle(theme).css).toContain('--ob-page-transition-display:block')
    expect(reduced.css).toContain('--ob-page-transition-display:none')
    expect(
      compileThemeStyle(makeTheme({ version: 2, motion: { level: 'full', transition: 'none' } }))
        .css
    ).toContain('--ob-page-transition-display:none')
    expect(reduced.css).not.toContain('animation:cbx-user-rise')
    expect(reduced.css).not.toContain('transition:transform')
    expect(compileThemeStyle(DEFAULT_THEME)).toEqual({
      version: 2,
      css: '',
      motion: 'off',
      layers: []
    })
  })
})

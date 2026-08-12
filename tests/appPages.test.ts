import { describe, expect, it } from 'vitest'
import { APP_PAGE_IDS, APP_PAGE_LOADERS, isAppPage } from '../src/app-pages'

describe('application page registry', () => {
  it('keeps every page behind one lazy module loader', () => {
    expect(Object.keys(APP_PAGE_LOADERS)).toEqual(APP_PAGE_IDS)
    expect(new Set(Object.values(APP_PAGE_LOADERS)).size).toBe(APP_PAGE_IDS.length)
  })

  it('narrows only declared page identifiers', () => {
    for (const page of APP_PAGE_IDS) expect(isAppPage(page)).toBe(true)
    expect(isAppPage('unknown')).toBe(false)
    expect(isAppPage('')).toBe(false)
  })
})

import { expect, test } from 'vitest'
import {
  OFFICIAL_MARKETPLACE_CATALOG,
  isNextOfficialPlugin
} from '../tauri-frontend/src/marketplace-catalog'
import nextOfficialPolicy from '../contracts/next/official-plugins.json'

test('official directory shows only the seven requested plugins in policy order', () => {
  expect(OFFICIAL_MARKETPLACE_CATALOG.map(({ id }) => id)).toEqual(
    nextOfficialPolicy.officialPlugins.map(({ id }) => id)
  )
  expect(OFFICIAL_MARKETPLACE_CATALOG).toHaveLength(7)
  expect(OFFICIAL_MARKETPLACE_CATALOG.find(({ id }) => id === 'archive-extractor')?.name).toBe(
    '压缩与解压缩'
  )
})
test('old official feeds cannot reintroduce retired plugins into the Next directory', () => {
  for (const id of nextOfficialPolicy.excludedFromNextOfficial)
    expect(isNextOfficialPlugin(id)).toBe(false)
  expect(isNextOfficialPlugin('unknown')).toBe(false)
  for (const { id } of OFFICIAL_MARKETPLACE_CATALOG) expect(isNextOfficialPlugin(id)).toBe(true)
})

import { expect, test } from 'vitest'
import { marketplaceEntry } from '../tauri-frontend/src/marketplace-catalog'
import { isOfficialPlugin } from '../tauri-frontend/src/plugin-identity'
test('marketplace displays arbitrary IDs from catalog metadata without a compiled identity list', () => {
  const plugin = marketplaceEntry({
    id: 'independent-csv',
    version: '1.0.0',
    displayName: 'CSV comparison',
    publisher: 'CrucibleBox',
    artifact: 'independent-csv-1.0.0.zip',
    size: 123,
    url: 'https://example.test/package.zip'
  })
  expect(plugin.name).toBe('CSV comparison')
  expect(isOfficialPlugin({})).toBe(false)
  expect(isOfficialPlugin({ publisherVerified: true, officialPublisher: true })).toBe(true)
  expect(isOfficialPlugin({ publisherVerified: false, officialPublisher: true })).toBe(false)
})

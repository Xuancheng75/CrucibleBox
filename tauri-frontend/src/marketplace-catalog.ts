import type { MarketplaceCatalogPluginDto } from './api/tauriApi'
export interface MarketplacePlugin {
  id: string
  name: string
  version: string
  publisher: string
  category: string
  description: string
  highlights: string[]
  tags: string[]
  keywords?: string[]
  minHostVersion?: string
  icon?: string
  artifact?: string
  size?: number
  url?: string
}
export function marketplaceEntry(entry: MarketplaceCatalogPluginDto): MarketplacePlugin {
  return { ...entry, name: entry.displayName || entry.id, publisher: entry.publisher || '未验证发布者', category: entry.category || '其他', description: entry.description || '', highlights: entry.highlights || [], tags: entry.tags || [] }
}

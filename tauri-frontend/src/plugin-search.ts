export interface PluginSearchFields {
  name: string
  id: string
  keywords?: readonly string[]
  description?: string
}

function score(query: string, fields: PluginSearchFields): number {
  if (!query) return 0
  const name = fields.name.toLocaleLowerCase()
  const id = fields.id.toLocaleLowerCase()
  if (name === query || id === query) return 0
  if (name.startsWith(query) || id.startsWith(query)) return 1
  if (fields.keywords?.some((keyword) => keyword.toLocaleLowerCase().includes(query))) return 2
  if (fields.description?.toLocaleLowerCase().includes(query)) return 3
  return -1
}

export function searchPlugins<T>(
  items: readonly T[],
  query: string,
  fields: (item: T) => PluginSearchFields
): T[] {
  const normalized = query.trim().toLocaleLowerCase()
  return items
    .map((item, index) => ({ item, index, rank: score(normalized, fields(item)) }))
    .filter((match) => match.rank >= 0)
    .sort((left, right) => left.rank - right.rank || left.index - right.index)
    .map((match) => match.item)
}

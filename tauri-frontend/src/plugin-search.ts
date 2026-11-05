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
  if (name === query) return 0
  if (name.startsWith(query)) return 1
  if (name.includes(query)) return 2
  if (/^[a-zü\s]+$/u.test(query) && /\p{Script=Han}/u.test(name)) {
    const syllables = pinyin(name, { toneType: 'none', type: 'array', nonZh: 'consecutive' }).map(
      (syllable) => syllable.replaceAll('ü', 'v')
    )
    const letters = query.replaceAll('ü', 'v').replaceAll(/\s+/g, '')
    const initials = syllables.map((syllable) => syllable[0]).join('')
    if (syllables.join('').startsWith(letters) || initials.startsWith(letters)) return 3
    if (
      initials.includes(letters) ||
      syllables.some((_, index) => syllables.slice(index).join('').startsWith(letters))
    )
      return 4
  }
  if (id === query) return 5
  if (id.startsWith(query)) return 6
  if (id.includes(query)) return 7
  if (fields.keywords?.some((keyword) => keyword.toLocaleLowerCase().includes(query))) return 8
  if (fields.description?.toLocaleLowerCase().includes(query)) return 9
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
import { pinyin } from 'pinyin-pro'

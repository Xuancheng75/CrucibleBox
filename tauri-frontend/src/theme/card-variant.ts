function order(id: string) {
  let hash = 2166136261
  for (const char of id) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619) >>> 0
  return hash
}

// Balanced for the same complete set of IDs, independent of display order.
export function cardVariants(ids: readonly string[]): Map<string, 1 | 2 | 3 | 4> {
  const ranked = ids
    .map((id) => ({ id, order: order(id) }))
    .sort((a, b) => a.order - b.order || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  return new Map(ranked.map(({ id }, index) => [id, ((index % 4) + 1) as 1 | 2 | 3 | 4]))
}

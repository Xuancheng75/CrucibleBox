export interface PluginIdentity {
  category: string
  accent: string
  accentAlt: string
  publisher: string
}
export function pluginIdentity(pluginId: string, author?: string): PluginIdentity {
  let hash = 0
  for (const character of pluginId) hash = (hash * 31 + character.charCodeAt(0)) >>> 0
  const palette = ['#2563eb', '#0f766e', '#7c3aed', '#c2410c', '#db2777', '#0891b2']
  return { category: '其他', accent: palette[hash % palette.length], accentAlt: palette[(hash + 1) % palette.length], publisher: author?.trim() || '未验证发布者' }
}
export function isOfficialPlugin(plugin: { publisherVerified?: boolean; officialPublisher?: boolean }): boolean {
  return plugin.publisherVerified === true && plugin.officialPublisher === true
}

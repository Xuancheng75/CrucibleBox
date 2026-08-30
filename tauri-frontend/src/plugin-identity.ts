export interface PluginIdentity {
  category: string
  accent: string
  accentAlt: string
  publisher: string
}

const OFFICIAL_IDENTITIES: Record<string, Omit<PluginIdentity, 'publisher'>> = {
  'document-engine': { category: '文档与 AI', accent: '#2563eb', accentAlt: '#06b6d4' },
  unienv: { category: '开发环境', accent: '#7c3aed', accentAlt: '#ec4899' },
  diary: { category: '记录与写作', accent: '#c2410c', accentAlt: '#f59e0b' },
  'gif-editor': { category: '图像与媒体', accent: '#db2777', accentAlt: '#8b5cf6' },
  'clipboard-manager': { category: '效率工具', accent: '#059669', accentAlt: '#22c55e' },
  'json-toolkit': { category: '开发工具', accent: '#0891b2', accentAlt: '#2563eb' },
  turntable: { category: '随机与决策', accent: '#ea580c', accentAlt: '#eab308' },
  'dice-roller': { category: '随机与决策', accent: '#9333ea', accentAlt: '#4f46e5' },
  'exchange-rates': { category: '数据与查询', accent: '#0284c7', accentAlt: '#14b8a6' },
  'system-info': { category: '系统工具', accent: '#475569', accentAlt: '#0f766e' },
  'theme-manager': { category: '个性化', accent: '#e11d48', accentAlt: '#8b5cf6' }
}

export function pluginIdentity(pluginId: string, author?: string): PluginIdentity {
  const official = OFFICIAL_IDENTITIES[pluginId]
  if (official) return { ...official, publisher: 'CrucibleBox' }

  return {
    category: '第三方插件',
    accent: '#4f46e5',
    accentAlt: '#0ea5e9',
    publisher: author?.trim() || '第三方发布者'
  }
}

export function isOfficialPlugin(pluginId: string): boolean {
  return Object.hasOwn(OFFICIAL_IDENTITIES, pluginId)
}

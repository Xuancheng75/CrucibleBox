import { describe, expect, it } from 'vitest'
import { searchPlugins } from '../tauri-frontend/src/plugin-search'

const plugins = [
  { id: 'description', name: '记录', keywords: [], description: '图片处理' },
  { id: 'keyword', name: '文件', keywords: ['图片处理'], description: '' },
  { id: 'prefix', name: '图片工具', keywords: [], description: '' },
  { id: 'exact', name: '图片', keywords: [], description: '' },
  { id: 'missing', name: '视频', keywords: [], description: '' }
]

describe('shared plugin search', () => {
  it('ranks exact name, name prefix, keyword, then description', () => {
    expect(searchPlugins(plugins, '图片', (plugin) => plugin).map((plugin) => plugin.id)).toEqual([
      'exact',
      'prefix',
      'keyword',
      'description'
    ])
  })

  it('preserves catalog order with an empty query', () => {
    expect(searchPlugins(plugins, ' ', (plugin) => plugin)).toEqual(plugins)
  })
})

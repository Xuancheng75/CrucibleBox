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

  it('ranks Chinese pinyin before an English manifest identifier', () => {
    const items = [
      { id: 'wen-file', name: '图片助手' },
      { id: 'text-tool', name: '文本编辑' }
    ]
    expect(searchPlugins(items, 'wen', (plugin) => plugin).map((plugin) => plugin.id)).toEqual([
      'text-tool',
      'wen-file'
    ])
  })

  it('matches full pinyin, initials, and a syllable starting within the Chinese name', () => {
    const items = [{ id: 'text-tool', name: '文本编辑' }]
    for (const query of ['wenbenbianji', 'wbbj', 'bianji', 'wen ben']) {
      expect(searchPlugins(items, query, (plugin) => plugin)).toEqual(items)
    }
  })

  it('finds Chinese substrings and preserves order among equally ranked matches', () => {
    const items = [
      { id: 'a', name: '图片编辑' },
      { id: 'b', name: '文本编辑' }
    ]
    expect(searchPlugins(items, '编辑', (plugin) => plugin)).toEqual(items)
  })
})

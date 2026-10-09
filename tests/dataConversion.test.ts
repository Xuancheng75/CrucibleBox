import { describe, expect, it } from 'vitest'
import {
  parseStructuredData,
  stringifyStructuredData
} from '../plugins/json-toolkit/src/DataConversionTab'

describe('数据格式转换', () => {
  it('正确读取含逗号与引号的 CSV', () => {
    expect(parseStructuredData('name,note\n"Ada, A","said ""hi"""', 'csv')).toEqual([
      { name: 'Ada, A', note: 'said "hi"' }
    ])
  })

  it('读取嵌套 YAML 与 TOML，而非仅解析键值行', () => {
    expect(parseStructuredData('team:\n  members:\n    - Ada\n    - Lin', 'yaml')).toEqual({
      team: { members: ['Ada', 'Lin'] }
    })
    expect(parseStructuredData('[team]\nmembers = ["Ada", "Lin"]', 'toml')).toEqual({
      team: { members: ['Ada', 'Lin'] }
    })
  })

  it('完成 JSON5 和 TSV 转换', () => {
    expect(parseStructuredData("{name:'Ada', enabled:true,}", 'json5')).toEqual({
      name: 'Ada',
      enabled: true
    })
    expect(stringifyStructuredData([{ name: 'Ada', age: '36' }], 'tsv')).toContain('name\tage')
  })
})

import { describe, expect, it } from 'vitest'
import { csvToObjects, keyValueDocument, parseJson5 } from '../src/renderer'

describe('数据与接口转换', () => {
  it('转换 CSV、宽松 JSON 和键值文档', () => {
    expect(csvToObjects('name,age\nAda,36')).toEqual([{ name: 'Ada', age: '36' }])
    expect(parseJson5("{name:'Ada', enabled:true,}")).toEqual({ name: 'Ada', enabled: true })
    expect(keyValueDocument('port=8080\nenabled: true')).toEqual({ port: 8080, enabled: true })
  })
})

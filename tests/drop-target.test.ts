import { describe, expect, it } from 'vitest'
import { resolveDropPaths } from '../tauri-frontend/src/utils/drop-target'

describe('resolveDropPaths', () => {
  it('单个 zip 返回批量目标且无忽略', () => {
    const r = resolveDropPaths(['C:/a/plugin.zip'])
    expect(r).toEqual({ kind: 'zip', targets: ['C:/a/plugin.zip'], ignoredZips: 0 })
  })

  it('多个 zip 全部进入批量目标（1.9.12 批量导入）', () => {
    const r = resolveDropPaths(['C:/a.zip', 'C:/b.ZIP', 'C:/c.zip'])
    expect(r?.kind).toBe('zip')
    expect(r?.targets).toEqual(['C:/a.zip', 'C:/b.ZIP', 'C:/c.zip'])
    expect(r?.ignoredZips).toBe(0)
  })

  it('zip 与无关文件混合：取全部 zip，忽略计数正确', () => {
    const r = resolveDropPaths(['C:/readme.txt', 'C:/x.zip', 'C:/y.exe', 'C:/z.zip'])
    expect(r?.kind).toBe('zip')
    expect(r?.targets).toEqual(['C:/x.zip', 'C:/z.zip'])
    expect(r?.ignoredZips).toBe(2)
  })

  it('无 zip 时回退为目录导入（取第一个路径）', () => {
    const r = resolveDropPaths(['D:\\dev\\my-plugin'])
    expect(r).toEqual({ kind: 'directory', targets: ['D:\\dev\\my-plugin'], ignoredZips: 0 })
  })

  it('全空输入返回 null', () => {
    expect(resolveDropPaths([])).toBeNull()
    expect(resolveDropPaths(['', '   '])).toBeNull()
  })

  it('大写扩展名按 zip 处理', () => {
    expect(resolveDropPaths(['C:/A.ZiP'])?.kind).toBe('zip')
  })
})

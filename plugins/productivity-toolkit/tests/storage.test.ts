import { describe, expect, it, vi } from 'vitest'
import plugin from '../src/main'

describe('笔记与效率存储', () => {
  it('读写并删除插件数据', async () => {
    const storage = { get: vi.fn().mockResolvedValue(['note']), set: vi.fn(), delete: vi.fn() }
    await plugin.activate({ storage } as never)
    await expect(plugin.onMessage({ type: 'get', key: 'notes' })).resolves.toEqual(['note'])
    await expect(plugin.onMessage({ type: 'set', key: 'notes', value: [] })).resolves.toEqual({
      success: true
    })
    await expect(plugin.onMessage({ type: 'remove', key: 'notes' })).resolves.toEqual({
      success: true
    })
    expect(storage.set).toHaveBeenCalledWith('notes', [])
    expect(storage.delete).toHaveBeenCalledWith('notes')
    await plugin.deactivate()
  })
})

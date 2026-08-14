import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PluginContext } from 'cruciblebox-plugin-api'
import diaryPlugin from '../plugins/diary/src/main'
import turntablePlugin from '../plugins/turntable/src/main'
import type { PluginStorageEntry } from '../shared/types/plugin.types'

class MemoryPluginStorage {
  private readonly values = new Map<string, unknown>()

  async get<T = unknown>(key: string): Promise<T | null> {
    const value = this.values.get(key)
    return value === undefined ? null : (structuredClone(value) as T)
  }

  async set(key: string, value: unknown): Promise<void> {
    this.values.set(key, structuredClone(value))
  }

  async delete(key: string): Promise<void> {
    this.values.delete(key)
  }

  async list<T = unknown>(prefix = ''): Promise<PluginStorageEntry<T>[]> {
    return Array.from(this.values)
      .filter(([key]) => key.startsWith(prefix))
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, value]) => ({ key, value: structuredClone(value) as T }))
  }

  async batch(
    mutations: Array<{ type: 'set'; key: string; value: unknown } | { type: 'delete'; key: string }>
  ): Promise<void> {
    const before = new Map(this.values)
    try {
      for (const mutation of mutations) {
        if (mutation.type === 'set') this.values.set(mutation.key, structuredClone(mutation.value))
        else this.values.delete(mutation.key)
      }
    } catch (error) {
      this.values.clear()
      for (const [key, value] of before) this.values.set(key, value)
      throw error
    }
  }
}

function createContext(storage: MemoryPluginStorage): PluginContext {
  return {
    id: 'plugin-id',
    config: {},
    logger: {
      debug: () => undefined,
      error: () => undefined,
      info: () => undefined,
      warn: () => undefined
    },
    database: {
      execute: async () => {
        throw new Error('raw database API must not be used')
      },
      query: async () => {
        throw new Error('raw database API must not be used')
      }
    },
    storage,
    api: {
      emitEvent: () => undefined,
      fetch: async () => new Response(),
      invokeTrustedService: async () => null,
      notify: () => undefined,
      onEvent: () => () => undefined,
      openDialog: async () => null,
      readFile: async () => Buffer.alloc(0),
      registerShortcut: () => () => undefined,
      writeFile: async () => undefined
    }
  }
}

afterEach(() => {
  diaryPlugin.deactivate()
  turntablePlugin.deactivate()
  vi.restoreAllMocks()
})

describe('production plugin storage consumers', () => {
  it('preserves diary save, monthly listing, export, and delete behavior', async () => {
    const storage = new MemoryPluginStorage()
    await diaryPlugin.activate(createContext(storage))

    await diaryPlugin.onMessage?.({
      type: 'saveEntry',
      date: '2026-08-10',
      title: '标题',
      content: '正文'
    })
    await diaryPlugin.onMessage?.({
      type: 'saveEntry',
      date: '2026-08-01',
      title: '月初',
      content: '第一篇'
    })

    await expect(
      diaryPlugin.onMessage?.({ type: 'getMonthEntries', year: 2026, month: 8 })
    ).resolves.toEqual({
      entries: [
        { entry_date: '2026-08-01', title: '月初' },
        { entry_date: '2026-08-10', title: '标题' }
      ]
    })
    await expect(
      diaryPlugin.onMessage?.({ type: 'exportSingle', date: '2026-08-10' })
    ).resolves.toMatchObject({ content: expect.stringContaining('正文') })

    await diaryPlugin.onMessage?.({ type: 'deleteEntry', date: '2026-08-10' })
    await expect(
      diaryPlugin.onMessage?.({ type: 'getEntry', date: '2026-08-10' })
    ).resolves.toEqual({ entry: null, draft: null })
  })

  it('preserves turntable CRUD, ordering, and weighted winner behavior', async () => {
    const storage = new MemoryPluginStorage()
    await turntablePlugin.activate(createContext(storage))

    const first = (await turntablePlugin.onMessage?.({
      type: 'addItem',
      payload: { label: 'A', weight: 1, color: '#111111' }
    })) as { id: number }
    const second = (await turntablePlugin.onMessage?.({
      type: 'addItem',
      payload: { label: 'B', weight: 3, color: '#222222' }
    })) as { id: number }
    await turntablePlugin.onMessage?.({
      type: 'updateItem',
      payload: { id: first.id, label: 'A+' }
    })
    await turntablePlugin.onMessage?.({
      type: 'reorderItems',
      payload: { ids: [second.id, first.id] }
    })

    await expect(turntablePlugin.onMessage?.({ type: 'getItems' })).resolves.toMatchObject([
      { id: second.id, label: 'B', sort_order: 0 },
      { id: first.id, label: 'A+', sort_order: 1 }
    ])
    vi.spyOn(globalThis.crypto, 'getRandomValues').mockImplementation((array) => {
      ;(array as Uint32Array)[0] = 0
      return array
    })
    await expect(turntablePlugin.onMessage?.({ type: 'spin' })).resolves.toMatchObject({
      winner: { id: second.id }
    })

    await turntablePlugin.onMessage?.({ type: 'deleteItem', payload: { id: second.id } })
    await expect(turntablePlugin.onMessage?.({ type: 'getItems' })).resolves.toMatchObject([
      { id: first.id, sort_order: 0 }
    ])
  })
})

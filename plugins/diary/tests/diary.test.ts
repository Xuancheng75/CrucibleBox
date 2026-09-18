import { beforeEach, describe, expect, it } from 'vitest'
import type { PluginContext, PluginStorageEntry, PluginStorageMutation } from 'cruciblebox-plugin-api'
import {
  parseDiaryDate,
  shouldLeaveAfterSave,
  type DiaryMutationResult
} from '../src/diary-domain'
import diaryPlugin from '../src/main'

class MemoryStorage {
  readonly values = new Map<string, unknown>()
  failBatch = false

  async get<T = unknown>(key: string): Promise<T | null> {
    return (this.values.get(key) as T | undefined) ?? null
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
      .map(([key, value]) => ({ key, value: structuredClone(value) as T }))
  }

  async batch(mutations: PluginStorageMutation[]): Promise<void> {
    if (this.failBatch) throw new Error('injected storage failure')
    const next = new Map(this.values)
    for (const mutation of mutations) {
      if (mutation.type === 'set') next.set(mutation.key, structuredClone(mutation.value))
      else next.delete(mutation.key)
    }
    this.values.clear()
    for (const [key, value] of next) this.values.set(key, value)
  }
}

function context(storage: MemoryStorage): PluginContext {
  return {
    id: 'diary-id',
    config: {},
    storage,
    pluginData: storage,
    capabilities: {
      events: { emitEvent() {}, onEvent: () => () => undefined },
      system: {
        clipboard: { read: async () => ({ text: '' }), write: async () => ({ ok: true }) },
        getSystemInfo: async () => ({
          os: { name: '', version: '', hostname: '' },
          cpu: { brand: '', cores: 0, physicalCores: 0, usage: 0 },
          memory: { total: 0, available: 0, usage: 0 },
          disks: [],
          network: []
        }),
        registerShortcut: () => () => undefined
      }
    },
    database: { query: async () => [], execute: async () => undefined },
    logger: { debug() {}, error() {}, info() {}, warn() {} },
    api: {
      emitEvent() {},
      fetch: async () => new Response(),
      notify() {},
      onEvent: () => () => undefined,
      openDialog: async () => null,
      readFile: async () => new Uint8Array(),
      registerShortcut: () => () => undefined,
      writeFile: async () => undefined,
      clipboard: {
        read: async () => ({ text: '' }),
        write: async () => ({ ok: true })
      },
      getSystemInfo: async () => ({
        os: { name: '', version: '', hostname: '' },
        cpu: { brand: '', cores: 0, physicalCores: 0, usage: 0 },
        memory: { total: 0, available: 0, usage: 0 },
        disks: [],
        network: []
      })
    }
  }
}

beforeEach(async () => {
  await diaryPlugin.deactivate()
})

describe('diary domain', () => {
  it('parses calendar dates without UTC-to-local rollover', () => {
    expect(parseDiaryDate('2024-02-29')).toEqual({
      value: '2024-02-29',
      year: 2024,
      month: 2,
      day: 29,
      weekday: 4
    })
    expect(parseDiaryDate('2025-02-29')).toBeNull()
    expect(parseDiaryDate('2026-08-11')?.day).toBe(11)
  })

  it('allows navigation only after the exact editor revision was saved', () => {
    const success: DiaryMutationResult = {
      ok: true,
      savedAt: '2026-08-11T00:00:00.000Z',
      deleted: false
    }
    expect(shouldLeaveAfterSave(success, 4, 4)).toBe(true)
    expect(shouldLeaveAfterSave(success, 4, 5)).toBe(false)
    expect(
      shouldLeaveAfterSave(
        { ok: false, error: { code: 'STORAGE_ERROR', message: 'disk full' } },
        4,
        4
      )
    ).toBe(false)
  })
})

describe('diary storage workflow', () => {
  it('reads migrated entries and drafts during the compatibility period', async () => {
    const storage = new MemoryStorage()
    await storage.set('entry:2026-08-11', {
      entry_date: '2026-08-11',
      title: 'saved',
      content: 'durable'
    })
    await storage.set('draft:2026-08-11', {
      date: '2026-08-11',
      title: 'draft',
      content: 'recover me',
      updatedAt: '2026-08-11T00:00:00.000Z'
    })
    await diaryPlugin.activate(context(storage))
    await expect(
      diaryPlugin.onMessage?.({ type: 'getEntry', date: '2026-08-11' })
    ).resolves.toMatchObject({
      entry: { title: 'saved', content: 'durable' },
      draft: { title: 'draft', content: 'recover me' }
    })
  })

  it('rejects writes without changing legacy storage', async () => {
    const storage = new MemoryStorage()
    await storage.set('draft:2026-08-11', { content: 'still here' })
    await diaryPlugin.activate(context(storage))
    await expect(
      diaryPlugin.onMessage?.({
        type: 'saveEntry',
        date: '2026-08-11',
        title: 'saved',
        content: 'should fail'
      })
    ).resolves.toEqual({
      ok: false,
      error: {
        code: 'READ_ONLY',
        message: '旧版日记处于只读兼容期，请在“笔记与效率”中继续编辑。'
      }
    })
    expect(storage.values.get('draft:2026-08-11')).toMatchObject({ content: 'still here' })
    expect(storage.values.has('entry:2026-08-11')).toBe(false)
  })
})

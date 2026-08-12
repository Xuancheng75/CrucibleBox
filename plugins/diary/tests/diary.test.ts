import { beforeEach, describe, expect, it } from 'vitest'
import type { PluginContext, PluginStorageEntry, PluginStorageMutation } from 'openbox-plugin-api'
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
      writeFile: async () => undefined
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
  it('recovers a draft and clears it atomically after an explicit save', async () => {
    const storage = new MemoryStorage()
    await diaryPlugin.activate(context(storage))
    await diaryPlugin.onMessage?.({
      type: 'saveDraft',
      date: '2026-08-11',
      title: 'draft',
      content: 'recover me'
    })
    await expect(
      diaryPlugin.onMessage?.({ type: 'getEntry', date: '2026-08-11' })
    ).resolves.toMatchObject({ draft: { title: 'draft', content: 'recover me' } })

    await expect(
      diaryPlugin.onMessage?.({
        type: 'saveEntry',
        date: '2026-08-11',
        title: 'saved',
        content: 'durable'
      })
    ).resolves.toMatchObject({ ok: true })
    await expect(
      diaryPlugin.onMessage?.({ type: 'getEntry', date: '2026-08-11' })
    ).resolves.toEqual({
      entry: {
        entry_date: '2026-08-11',
        title: 'saved',
        content: 'durable'
      },
      draft: null
    })
  })

  it('returns an explicit failure and preserves the recoverable draft', async () => {
    const storage = new MemoryStorage()
    await diaryPlugin.activate(context(storage))
    await diaryPlugin.onMessage?.({
      type: 'saveDraft',
      date: '2026-08-11',
      title: 'draft',
      content: 'still here'
    })
    storage.failBatch = true
    await expect(
      diaryPlugin.onMessage?.({
        type: 'saveEntry',
        date: '2026-08-11',
        title: 'saved',
        content: 'should fail'
      })
    ).resolves.toEqual({
      ok: false,
      error: { code: 'STORAGE_ERROR', message: 'injected storage failure' }
    })
    expect(storage.values.get('draft:2026-08-11')).toMatchObject({ content: 'still here' })
    expect(storage.values.has('entry:2026-08-11')).toBe(false)
  })
})

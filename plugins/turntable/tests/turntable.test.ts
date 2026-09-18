import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { PluginContext, PluginStorageEntry, PluginStorageMutation } from 'cruciblebox-plugin-api'
import turntablePlugin from '../src/main'
import {
  normalizeAngle,
  POINTER_ANGLE,
  selectWeightedItem,
  targetRotationForWinner,
  winnerCenterAngle
} from '../src/turntable-domain'
import type { TurntableItem } from '../src/types'

class MemoryStorage {
  readonly values = new Map<string, unknown>()

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
      .map(([key, value]) => ({ key, value: structuredClone(value) as T }))
  }

  async batch(mutations: PluginStorageMutation[]): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, 1))
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
    id: 'turntable-id',
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

function item(id: number, weight: number): TurntableItem {
  return {
    id,
    label: `item-${id}`,
    weight,
    color: '#1677ff',
    sort_order: id - 1,
    created_at: '2026-08-11T00:00:00.000Z'
  }
}

beforeEach(async () => {
  await turntablePlugin.deactivate()
  vi.restoreAllMocks()
})

describe('weighted winner selection', () => {
  it('uses half-open weighted intervals at exact boundaries', () => {
    const items = [item(1, 1), item(2, 3)]
    expect(selectWeightedItem(items, 0).id).toBe(1)
    expect(selectWeightedItem(items, 0.249999).id).toBe(1)
    expect(selectWeightedItem(items, 0.25).id).toBe(2)
    expect(selectWeightedItem(items, 0.999999).id).toBe(2)
  })

  it('rejects invalid samples and weights', () => {
    expect(() => selectWeightedItem([item(1, 1)], 1)).toThrow()
    expect(() => selectWeightedItem([item(1, 0)], 0)).toThrow()
    expect(() => selectWeightedItem([], 0)).toThrow()
  })
})

describe('winner geometry', () => {
  it('places every weighted sector center under the top pointer', () => {
    const items = [item(1, 1), item(2, 2), item(3, 7)]
    for (const current of [-9.2, 0, 1.7, 41.5]) {
      for (const winner of items) {
        const target = targetRotationForWinner(items, winner.id, current, 5)
        const finalCenter = normalizeAngle(winnerCenterAngle(items, winner.id) + target)
        expect(finalCenter).toBeCloseTo(normalizeAngle(POINTER_ANGLE), 10)
        expect(target).toBeGreaterThanOrEqual(current + 5 * Math.PI * 2)
      }
    }
  })
})

describe('turntable persistence', () => {
  it('preserves legacy option order across restart without accepting edits', async () => {
    const storage = new MemoryStorage()
    storage.values.set('items', [item(3, 1), item(1, 1), item(2, 1)])
    await turntablePlugin.activate(context(storage))
    await expect(
      turntablePlugin.onMessage?.({ type: 'reorderItems', payload: { ids: [3, 1, 2] } })
    ).resolves.toEqual({ error: '旧版转盘处于兼容期，请在“笔记与效率”中继续维护选项。' })

    await turntablePlugin.deactivate()
    await turntablePlugin.activate(context(storage))
    await expect(turntablePlugin.onMessage?.({ type: 'getItems' })).resolves.toMatchObject([
      { id: 1, sort_order: 0 },
      { id: 2, sort_order: 1 },
      { id: 3, sort_order: 2 }
    ])
  })

  it('rejects partial reorder data and uses Web Crypto for the winner', async () => {
    const storage = new MemoryStorage()
    storage.values.set('items', [item(1, 1), item(2, 3)])
    await turntablePlugin.activate(context(storage))
    await expect(
      turntablePlugin.onMessage?.({ type: 'reorderItems', payload: { ids: [2] } })
    ).resolves.toEqual({ error: '旧版转盘处于兼容期，请在“笔记与效率”中继续维护选项。' })

    vi.spyOn(globalThis.crypto, 'getRandomValues').mockImplementation((array) => {
      ;(array as Uint32Array)[0] = 0
      return array
    })
    await expect(turntablePlugin.onMessage?.({ type: 'spin' })).resolves.toMatchObject({
      winner: { id: 1 }
    })
  })
})

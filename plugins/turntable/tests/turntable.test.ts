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
  it('serializes concurrent edits and preserves atomic order across restart', async () => {
    const storage = new MemoryStorage()
    await turntablePlugin.activate(context(storage))
    const added = await Promise.all(
      ['A', 'B', 'C'].map((label) =>
        turntablePlugin.onMessage?.({
          type: 'addItem',
          payload: { label, weight: 1, color: '' }
        })
      )
    )
    expect(added.map((value) => (value as TurntableItem).id)).toEqual([1, 2, 3])
    await expect(
      turntablePlugin.onMessage?.({ type: 'reorderItems', payload: { ids: [3, 1, 2] } })
    ).resolves.toMatchObject([
      { id: 3, sort_order: 0 },
      { id: 1, sort_order: 1 },
      { id: 2, sort_order: 2 }
    ])

    await turntablePlugin.deactivate()
    await turntablePlugin.activate(context(storage))
    await expect(turntablePlugin.onMessage?.({ type: 'getItems' })).resolves.toMatchObject([
      { id: 3, sort_order: 0 },
      { id: 1, sort_order: 1 },
      { id: 2, sort_order: 2 }
    ])
  })

  it('rejects partial reorder data and uses Web Crypto for the winner', async () => {
    const storage = new MemoryStorage()
    storage.values.set('items', [item(1, 1), item(2, 3)])
    await turntablePlugin.activate(context(storage))
    await expect(
      turntablePlugin.onMessage?.({ type: 'reorderItems', payload: { ids: [2] } })
    ).resolves.toEqual({ error: '排序必须包含且仅包含全部现有选项' })

    vi.spyOn(globalThis.crypto, 'getRandomValues').mockImplementation((array) => {
      ;(array as Uint32Array)[0] = 0
      return array
    })
    await expect(turntablePlugin.onMessage?.({ type: 'spin' })).resolves.toMatchObject({
      winner: { id: 1 }
    })
  })
})

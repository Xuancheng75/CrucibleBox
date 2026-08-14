import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'
import { secureRandomUnit, selectWeightedItem } from './turntable-domain'
import type {
  AddItemPayload,
  DeleteItemPayload,
  ReorderPayload,
  SpinResult,
  TurntableItem,
  UpdateItemPayload
} from './types'

const STORAGE_KEY = 'items'
const MAX_ITEMS = 500
const MAX_LABEL_LENGTH = 128
const MAX_WEIGHT = 1_000_000_000

const DEFAULT_COLORS = [
  '#FF6B6B',
  '#4ECDC4',
  '#45B7D1',
  '#96CEB4',
  '#FFEAA7',
  '#DDA0DD',
  '#98D8C8',
  '#F7DC6F',
  '#BB8FCE',
  '#85C1E9',
  '#F0B27A',
  '#82E0AA'
]

let storage: PluginContext['storage'] | null = null
let logger: PluginContext['logger'] | null = null
let mutationQueue: Promise<void> = Promise.resolve()

function currentStorage(): PluginContext['storage'] {
  if (!storage) throw new Error('插件存储未初始化')
  return storage
}

function normalizeStoredItems(value: unknown): TurntableItem[] {
  if (!Array.isArray(value)) return []
  const ids = new Set<number>()
  const items: TurntableItem[] = []
  for (const candidate of value.slice(0, MAX_ITEMS)) {
    if (!candidate || typeof candidate !== 'object') continue
    const raw = candidate as Partial<TurntableItem>
    const id = Number(raw.id)
    const weight = Number(raw.weight)
    const label = typeof raw.label === 'string' ? raw.label.trim() : ''
    if (
      !Number.isSafeInteger(id) ||
      id <= 0 ||
      ids.has(id) ||
      !label ||
      label.length > MAX_LABEL_LENGTH ||
      !Number.isFinite(weight) ||
      weight <= 0 ||
      weight > MAX_WEIGHT
    ) {
      continue
    }
    ids.add(id)
    items.push({
      id,
      label,
      weight,
      color: typeof raw.color === 'string' && raw.color ? raw.color.slice(0, 64) : '#1677ff',
      sort_order: Number.isSafeInteger(Number(raw.sort_order)) ? Number(raw.sort_order) : items.length,
      created_at: typeof raw.created_at === 'string' ? raw.created_at : ''
    })
  }
  return items.sort((left, right) => left.sort_order - right.sort_order || left.id - right.id)
}

async function loadItems(store = currentStorage()): Promise<TurntableItem[]> {
  return normalizeStoredItems(await store.get<unknown>(STORAGE_KEY))
}

async function saveItems(
  store: PluginContext['storage'],
  items: TurntableItem[]
): Promise<TurntableItem[]> {
  const normalized = items.map((item, sortOrder) => ({ ...item, sort_order: sortOrder }))
  await store.batch([{ type: 'set', key: STORAGE_KEY, value: normalized }])
  return normalized
}

async function runMutation<Result>(
  operation: (store: PluginContext['storage']) => Promise<Result>
): Promise<Result> {
  const store = currentStorage()
  const result = mutationQueue.then(
    () => operation(store),
    () => operation(store)
  )
  mutationQueue = result.then(
    () => undefined,
    () => undefined
  )
  return await result
}

function pickColor(index: number, existing: TurntableItem[]): string {
  const used = new Set(existing.map((item) => item.color))
  for (const color of DEFAULT_COLORS) {
    if (!used.has(color)) return color
  }
  return `hsl(${(index * 137.508) % 360}, 70%, 60%)`
}

function validWeight(value: unknown): number | null {
  const weight = Number(value)
  return Number.isFinite(weight) && weight > 0 && weight <= MAX_WEIGHT ? weight : null
}

async function handleMessage(message: unknown): Promise<unknown> {
  if (!message || typeof message !== 'object') return { error: '消息格式无效' }
  const msg = message as { type?: unknown; payload?: unknown }

  switch (msg.type) {
    case 'getItems': {
      const store = currentStorage()
      await mutationQueue
      return await loadItems(store)
    }

    case 'addItem':
      return await runMutation(async (store) => {
        const payload = msg.payload as AddItemPayload
        const label = typeof payload?.label === 'string' ? payload.label.trim() : ''
        const weight = validWeight(payload?.weight)
        if (!label || label.length > MAX_LABEL_LENGTH) return { error: '选项名称无效' }
        if (weight === null) return { error: '权重必须是有效的正数' }
        const items = await loadItems(store)
        if (items.length >= MAX_ITEMS) return { error: `选项数量不能超过 ${MAX_ITEMS}` }
        const sortOrder = items.length
        const item: TurntableItem = {
          id: Math.max(0, ...items.map((entry) => entry.id)) + 1,
          label,
          weight,
          color:
            typeof payload.color === 'string' && payload.color.trim()
              ? payload.color.trim().slice(0, 64)
              : pickColor(sortOrder, items),
          sort_order: sortOrder,
          created_at: new Date().toISOString()
        }
        await saveItems(store, [...items, item])
        return item
      })

    case 'updateItem':
      return await runMutation(async (store) => {
        const payload = msg.payload as UpdateItemPayload
        const id = Number(payload?.id)
        if (!Number.isSafeInteger(id) || id <= 0) return { error: '选项 ID 无效' }
        const items = await loadItems(store)
        const itemIndex = items.findIndex((item) => item.id === id)
        if (itemIndex < 0) return { error: '更新选项失败：未找到该选项' }
        if (payload.label === undefined && payload.weight === undefined && payload.color === undefined) {
          return { error: '没有要更新的字段' }
        }
        const updated = { ...items[itemIndex] }
        if (payload.label !== undefined) {
          const label = typeof payload.label === 'string' ? payload.label.trim() : ''
          if (!label || label.length > MAX_LABEL_LENGTH) return { error: '选项名称无效' }
          updated.label = label
        }
        if (payload.weight !== undefined) {
          const weight = validWeight(payload.weight)
          if (weight === null) return { error: '权重必须是有效的正数' }
          updated.weight = weight
        }
        if (payload.color !== undefined) {
          if (typeof payload.color !== 'string' || !payload.color.trim()) {
            return { error: '颜色无效' }
          }
          updated.color = payload.color.trim().slice(0, 64)
        }
        items[itemIndex] = updated
        await saveItems(store, items)
        return updated
      })

    case 'deleteItem':
      return await runMutation(async (store) => {
        const payload = msg.payload as DeleteItemPayload
        const id = Number(payload?.id)
        if (!Number.isSafeInteger(id) || id <= 0) return { error: '选项 ID 无效' }
        const items = await loadItems(store)
        if (!items.some((item) => item.id === id)) return { error: '未找到该选项' }
        await saveItems(
          store,
          items.filter((item) => item.id !== id)
        )
        return { success: true }
      })

    case 'reorderItems':
      return await runMutation(async (store) => {
        const payload = msg.payload as ReorderPayload
        if (
          !payload ||
          !Array.isArray(payload.ids) ||
          payload.ids.some((id) => !Number.isSafeInteger(id) || id <= 0) ||
          new Set(payload.ids).size !== payload.ids.length
        ) {
          return { error: '排序数据无效' }
        }
        const items = await loadItems(store)
        const existingIds = new Set(items.map((item) => item.id))
        if (payload.ids.length !== items.length || payload.ids.some((id) => !existingIds.has(id))) {
          return { error: '排序必须包含且仅包含全部现有选项' }
        }
        const byId = new Map(items.map((item) => [item.id, item]))
        return await saveItems(
          store,
          payload.ids.map((id) => byId.get(id) as TurntableItem)
        )
      })

    case 'spin': {
      const store = currentStorage()
      await mutationQueue
      const items = await loadItems(store)
      if (items.length === 0) return { error: '没有可抽奖的选项' }
      return { winner: selectWeightedItem(items, secureRandomUnit()) } satisfies SpinResult
    }

    default:
      return { error: `未知消息类型: ${String(msg.type)}` }
  }
}

const plugin: PluginMain = {
  activate(ctx: PluginContext) {
    storage = ctx.storage
    logger = ctx.logger
    mutationQueue = Promise.resolve()
    logger.info('转盘抽奖插件已激活')
  },

  deactivate() {
    storage = null
    logger = null
  },

  onMessage(message: unknown): Promise<unknown> {
    return handleMessage(message)
  }
}

export default plugin

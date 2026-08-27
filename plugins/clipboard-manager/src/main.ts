import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'

interface ClipItem {
  id: string
  text: string
  timestamp: number
  pinned: boolean
}

let ctx: PluginContext | null = null
let pollTimer: ReturnType<typeof setInterval> | null = null
let lastText = ''

const plugin: PluginMain = {
  async activate(pluginCtx: PluginContext) {
    ctx = pluginCtx
    const stored = await ctx.storage.get<string[]>('history')
    if (stored) {
      lastText = stored.length > 0 ? JSON.parse(stored[0] as string).text : ''
    }
    startPolling()
  },

  deactivate() {
    stopPolling()
    ctx = null
  },

  async onMessage(message: unknown) {
    if (!ctx) return { error: 'not activated' }
    const msg = message as { type: string; id?: string; text?: string }

    switch (msg.type) {
      case 'getHistory': {
        const items = await loadHistory()
        return { items }
      }
      case 'deleteItem': {
        if (!msg.id) return { error: 'missing id' }
        await deleteItem(msg.id)
        return { ok: true }
      }
      case 'togglePin': {
        if (!msg.id) return { error: 'missing id' }
        await togglePin(msg.id)
        return { ok: true }
      }
      case 'clearAll': {
        await ctx.storage.set('history', JSON.stringify([]))
        return { ok: true }
      }
      case 'copyToClipboard': {
        if (msg.text === undefined) return { error: 'missing text' }
        await ctx.api.clipboard.write(msg.text)
        return { ok: true }
      }
      default:
        return { error: `unknown message: ${msg.type}` }
    }
  }
}

function startPolling() {
  stopPolling()
  const interval = ctx?.config.pollInterval as number || 1000
  pollTimer = setInterval(pollClipboard, interval)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

async function pollClipboard() {
  if (!ctx) return
  try {
    const result = await ctx.api.clipboard.read()
    const text = result.text
    if (text && text !== lastText) {
      lastText = text
      await addToHistory(text)
    }
  } catch {
    // clipboard may be unavailable
  }
}

async function loadHistory(): Promise<ClipItem[]> {
  if (!ctx) return []
  const raw = await ctx.storage.get<string>('history')
  if (!raw) return []
  try {
    return JSON.parse(raw) as ClipItem[]
  } catch {
    return []
  }
}

async function addToHistory(text: string) {
  if (!ctx) return
  const items = await loadHistory()
  const maxItems = (ctx.config.maxItems as number) || 200
  const newItem: ClipItem = {
    id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
    text,
    timestamp: Date.now(),
    pinned: false
  }
  const filtered = items.filter((item) => item.text !== text)
  filtered.unshift(newItem)
  const trimmed = filtered.slice(0, maxItems)
  await ctx.storage.set('history', JSON.stringify(trimmed))
}

async function deleteItem(id: string) {
  if (!ctx) return
  const items = await loadHistory()
  await ctx.storage.set('history', JSON.stringify(items.filter((item) => item.id !== id)))
}

async function togglePin(id: string) {
  if (!ctx) return
  const items = await loadHistory()
  const updated = items.map((item) =>
    item.id === id ? { ...item, pinned: !item.pinned } : item
  )
  await ctx.storage.set('history', JSON.stringify(updated))
}

export default plugin

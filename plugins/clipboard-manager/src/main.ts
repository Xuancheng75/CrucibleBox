import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'

interface ClipItem {
  id: string
  text: string
  timestamp: number
  pinned: boolean
}

let ctx: PluginContext | null = null
let lastText = ''

const plugin: PluginMain = {
  async activate(pluginCtx: PluginContext) {
    ctx = pluginCtx
    // 读取当前剪贴板作为初始 lastText（避免首次事件重复记录）
    try {
      const result = await ctx.api.clipboard.read()
      lastText = result.text || ''
    } catch {
      // clipboard may be unavailable
    }
    // 加载已有历史，更新 lastText
    const stored = await ctx.storage.get<ClipItem[]>('history')
    if (stored && stored.length > 0) {
      lastText = stored[0].text || lastText
    }
  },

  deactivate() {
    ctx = null
  },

  async onMessage(message: unknown) {
    if (!ctx) return { error: 'not activated' }
    const msg = message as { type: string; id?: string; text?: string }

    switch (msg.type) {
      case 'clipboard:changed': {
        // 宿主侧 clipboard_monitor 广播的事件
        const text = msg.text || ''
        if (text && text !== lastText) {
          lastText = text
          await addToHistory(text)
        }
        return { ok: true }
      }
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
        await ctx.storage.set('history', [])
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

async function loadHistory(): Promise<ClipItem[]> {
  if (!ctx) return []
  const raw = await ctx.storage.get<ClipItem[]>('history')
  if (!raw) return []
  return raw
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
  await ctx.storage.set('history', trimmed)
}

async function deleteItem(id: string) {
  if (!ctx) return
  const items = await loadHistory()
  await ctx.storage.set('history', items.filter((item) => item.id !== id))
}

async function togglePin(id: string) {
  if (!ctx) return
  const items = await loadHistory()
  const updated = items.map((item) =>
    item.id === id ? { ...item, pinned: !item.pinned } : item
  )
  await ctx.storage.set('history', updated)
}

export default plugin

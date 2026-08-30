import { useState, useEffect, useCallback } from 'react'
import type { CSSProperties } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'

interface ClipItem {
  id: string
  text: string
  timestamp: number
  pinned: boolean
}

const s: Record<string, CSSProperties> = {
  page: {
    minHeight: '100%',
    padding: 20,
    background: 'var(--ob-color-bg-layout, #f7f9fb)',
    color: 'var(--ob-color-text, #1f2933)',
    fontFamily: 'var(--ob-font-family, system-ui, sans-serif)'
  },
  toolbar: {
    display: 'flex',
    gap: 8,
    marginBottom: 12,
    alignItems: 'center'
  },
  search: {
    flex: 1,
    padding: '8px 12px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    fontSize: 13,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)'
  },
  btn: {
    padding: '6px 14px',
    border: 'none',
    borderRadius: 6,
    background: 'var(--ob-color-primary, #2563eb)',
    color: '#fff',
    fontSize: 12,
    fontWeight: 600,
    cursor: 'pointer'
  },
  btnDanger: {
    padding: '6px 14px',
    border: 'none',
    borderRadius: 6,
    background: '#ff4d4f',
    color: '#fff',
    fontSize: 12,
    cursor: 'pointer'
  },
  item: {
    display: 'flex',
    alignItems: 'flex-start',
    gap: 10,
    padding: '10px 12px',
    marginBottom: 6,
    background: 'var(--ob-color-bg-container, #fff)',
    borderRadius: 8,
    border: '1px solid var(--ob-color-border-secondary, #e6edf3)',
    cursor: 'pointer'
  },
  itemPinned: {
    display: 'flex',
    alignItems: 'flex-start',
    gap: 10,
    padding: '10px 12px',
    marginBottom: 6,
    background: 'var(--ob-color-primary-bg, #eef4ff)',
    borderRadius: 8,
    border: '1px solid var(--ob-color-primary, #2563eb)'
  },
  text: {
    flex: 1,
    fontSize: 13,
    lineHeight: 1.5,
    wordBreak: 'break-all' as const,
    maxHeight: 60,
    overflow: 'hidden' as const,
    textOverflow: 'ellipsis' as const
  },
  meta: {
    fontSize: 11,
    color: 'var(--ob-color-text-secondary, #999)',
    whiteSpace: 'nowrap' as const
  },
  actions: {
    display: 'flex',
    gap: 4,
    flexShrink: 0
  },
  iconBtn: {
    padding: '4px 8px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 4,
    background: 'transparent',
    fontSize: 11,
    cursor: 'pointer',
    color: 'var(--ob-color-text-secondary, #666)'
  },
  empty: {
    textAlign: 'center' as const,
    padding: 40,
    color: 'var(--ob-color-text-secondary, #999)',
    fontSize: 14
  }
}

export default function ClipboardManagerPlugin({ api }: PluginRenderProps) {
  const [items, setItems] = useState<ClipItem[]>([])
  const [search, setSearch] = useState('')
  const [pinnedOnly, setPinnedOnly] = useState(false)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    const res = await api.sendToBackend({ type: 'getHistory' }) as { items: ClipItem[] }
    setItems(res.items || [])
    setLoading(false)
  }, [api])

  useEffect(() => {
    void refresh()
    return api.onBackendMessage((message) => {
      const event = message as { type?: unknown }
      if (event.type === 'clipboard:changed') void refresh()
    })
  }, [api, refresh])

  const filtered = items.filter((item) =>
    item.text.toLowerCase().includes(search.toLowerCase()) && (!pinnedOnly || item.pinned)
  )

  const pinned = filtered.filter((i) => i.pinned)
  const unpinned = filtered.filter((i) => !i.pinned)

  const handleCopy = async (text: string) => {
    await api.sendToBackend({ type: 'copyToClipboard', text })
    // The host clipboard event is debounced and may arrive after this view
    // renders.  Refresh immediately so an explicit copy is visible even when
    // the native event is coalesced or the clipboard owner is this plugin.
    await refresh()
    api.notify('已复制', text.slice(0, 50))
  }

  const handleDelete = async (id: string) => {
    await api.sendToBackend({ type: 'deleteItem', id })
    refresh()
  }

  const handleTogglePin = async (id: string) => {
    await api.sendToBackend({ type: 'togglePin', id })
    refresh()
  }

  const handleClear = async () => {
    const ok = await api.confirm({ title: '清空历史', message: '确定要清空所有剪贴板历史吗？' })
    if (!ok) return
    await api.sendToBackend({ type: 'clearAll' })
    refresh()
  }

  const formatTime = (ts: number) => {
    const d = new Date(ts)
    return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
  }

  if (loading) {
    return <div style={s.page}><div style={s.empty}>加载中...</div></div>
  }

  return (
    <div style={s.page}>
      <h2 style={{ margin: '0 0 16px', fontSize: 20 }}>剪贴板管理器</h2>
      <div style={s.toolbar}>
        <input
          style={s.search}
          placeholder="搜索..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <button style={s.btnDanger} onClick={handleClear}>清空</button>
        <button style={s.iconBtn} onClick={() => setPinnedOnly((value) => !value)}>
          {pinnedOnly ? '显示全部' : '仅置顶'}
        </button>
      </div>

      {filtered.length === 0 && (
        <div style={s.empty}>{search ? '没有匹配的记录' : '暂无剪贴板记录'}</div>
      )}

      {pinned.length > 0 && (
        <>
          <div style={{ fontSize: 12, color: '#999', marginBottom: 6, marginTop: 8 }}>已置顶</div>
          {pinned.map((item) => (
            <div key={item.id} style={s.itemPinned} onClick={() => handleCopy(item.text)}>
              <div style={s.text}>{item.text}</div>
              <div style={s.meta}>{formatTime(item.timestamp)}</div>
              <div style={s.actions}>
                <button style={s.iconBtn} onClick={(e) => { e.stopPropagation(); handleTogglePin(item.id) }}>取消置顶</button>
                <button style={s.iconBtn} onClick={(e) => { e.stopPropagation(); handleDelete(item.id) }}>删除</button>
              </div>
            </div>
          ))}
        </>
      )}

      {unpinned.length > 0 && (
        <>
          {pinned.length > 0 && <div style={{ fontSize: 12, color: '#999', marginBottom: 6, marginTop: 12 }}>最近</div>}
          {unpinned.map((item) => (
            <div key={item.id} style={s.item} onClick={() => handleCopy(item.text)}>
              <div style={s.text}>{item.text}</div>
              <div style={s.meta}>{formatTime(item.timestamp)}</div>
              <div style={s.actions}>
                <button style={s.iconBtn} onClick={(e) => { e.stopPropagation(); handleTogglePin(item.id) }}>置顶</button>
                <button style={s.iconBtn} onClick={(e) => { e.stopPropagation(); handleDelete(item.id) }}>删除</button>
              </div>
            </div>
          ))}
        </>
      )}
    </div>
  )
}

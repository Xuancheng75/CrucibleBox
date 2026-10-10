import React, { useEffect, useMemo, useState } from 'react'
import type { DiaryRenderProps } from '../next-service'

interface Note {
  id: string
  title: string
  content: string
  tags: string[]
  date: string
  updatedAt: number
  deleted?: boolean
  versions: Array<{ at: number; content: string }>
}

const field: React.CSSProperties = {
  width: '100%',
  boxSizing: 'border-box',
  padding: 9,
  border: '1px solid var(--ob-color-border, #cbd5e1)',
  borderRadius: 6,
  background: 'var(--ob-color-bg-container, #fff)',
  color: 'var(--ob-color-text, #222)'
}

export default function NotesPage({ api }: { api: DiaryRenderProps['api'] }) {
  const [notes, setNotes] = useState<Note[]>([])
  const [loaded, setLoaded] = useState(false)
  const [selected, setSelected] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [showTrash, setShowTrash] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    let active = true
    void api
      .execute({ type: 'getNotes' })
      .then((value) => {
        if (active) {
          setNotes(Array.isArray(value) ? (value as Note[]) : [])
          setLoaded(true)
        }
      })
      .catch((reason) => {
        if (active) setError(String(reason))
      })
    return () => {
      active = false
    }
  }, [api])

  useEffect(() => {
    if (!loaded) return
    const timer = window.setTimeout(() => {
      void api.execute({ type: 'setNotes', notes }).catch((reason) => setError(String(reason)))
    }, 400)
    return () => window.clearTimeout(timer)
  }, [api, loaded, notes])

  const active = notes.find((note) => note.id === selected)
  const visible = useMemo(
    () =>
      notes.filter(
        (note) =>
          Boolean(note.deleted) === showTrash &&
          `${note.title} ${note.content} ${note.tags.join(' ')}`
            .toLowerCase()
            .includes(query.toLowerCase())
      ),
    [notes, query, showTrash]
  )
  const backlinks = useMemo(
    () =>
      active
        ? notes.filter(
            (note) =>
              !note.deleted && note.id !== active.id && note.content.includes(`[[${active.title}]]`)
          )
        : [],
    [active, notes]
  )

  const change = (id: string, patch: Partial<Note>) => {
    setNotes((current) =>
      current.map((note) => (note.id === id ? { ...note, ...patch, updatedAt: Date.now() } : note))
    )
  }
  const create = () => {
    const now = Date.now()
    const note: Note = {
      id: crypto.randomUUID(),
      title: '未命名笔记',
      content: '',
      tags: [],
      date: new Date(now).toISOString().slice(0, 10),
      updatedAt: now,
      versions: []
    }
    setNotes((current) => [note, ...current])
    setSelected(note.id)
    setShowTrash(false)
  }
  const saveVersion = () => {
    if (!active) return
    change(active.id, {
      versions: [...(active.versions ?? []).slice(-49), { at: Date.now(), content: active.content }]
    })
  }
  const exportNote = () => {
    if (!active) return
    const blob = new Blob([`# ${active.title}\n\n${active.content}`], {
      type: 'text/markdown;charset=utf-8'
    })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = `${active.title.replace(/[<>:"/\\|?*]/g, '_') || '笔记'}.md`
    anchor.click()
    window.setTimeout(() => URL.revokeObjectURL(url), 1000)
  }

  return (
    <div style={{ padding: 18, color: 'var(--ob-color-text, #222)' }}>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 12 }}>
        <h2 style={{ flex: 1, margin: 0 }}>笔记</h2>
        <button onClick={create}>新建笔记</button>
        <button
          onClick={() => {
            setShowTrash((value) => !value)
            setSelected(null)
          }}
        >
          {showTrash ? '返回笔记' : '回收站'}
        </button>
      </div>
      {error && (
        <p role="alert" style={{ color: 'var(--ob-color-error, #c00)' }}>
          {error}
        </p>
      )}
      <input
        style={{ ...field, marginBottom: 12 }}
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="搜索标题、正文和标签"
      />
      <div style={{ display: 'grid', gridTemplateColumns: 'minmax(180px, 30%) 1fr', gap: 16 }}>
        <div style={{ display: 'grid', alignContent: 'start', gap: 6 }}>
          {visible.map((note) => (
            <button
              key={note.id}
              onClick={() => setSelected(note.id)}
              style={{
                textAlign: 'left',
                padding: 10,
                border: '1px solid var(--ob-color-border, #ddd)',
                borderRadius: 6,
                background:
                  note.id === selected ? 'var(--ob-color-primary-bg, #e6f4ff)' : 'transparent',
                color: 'inherit'
              }}
            >
              <strong>{note.title}</strong>
              <small style={{ display: 'block', opacity: 0.7 }}>
                {note.date} · {note.tags.join('、')}
              </small>
            </button>
          ))}
          {loaded && visible.length === 0 && <span>没有匹配的笔记</span>}
        </div>
        <div>
          {active ? (
            <div style={{ display: 'grid', gap: 10 }}>
              <input
                style={field}
                aria-label="笔记标题"
                value={active.title}
                onChange={(event) => change(active.id, { title: event.target.value })}
              />
              <input
                style={field}
                aria-label="笔记标签"
                value={active.tags.join(', ')}
                onChange={(event) =>
                  change(active.id, {
                    tags: event.target.value
                      .split(/[,，]/)
                      .map((tag) => tag.trim())
                      .filter(Boolean)
                  })
                }
                placeholder="标签以逗号分隔"
              />
              <textarea
                style={{ ...field, minHeight: 280, resize: 'vertical' }}
                aria-label="笔记正文"
                value={active.content}
                onChange={(event) => change(active.id, { content: event.target.value })}
                placeholder="支持 Markdown；使用 [[笔记标题]] 建立双向链接"
              />
              <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                <button onClick={saveVersion}>保存版本</button>
                <button onClick={exportNote}>导出 Markdown</button>
                <button onClick={() => change(active.id, { deleted: !active.deleted })}>
                  {active.deleted ? '恢复' : '移入回收站'}
                </button>
              </div>
              <div>
                引用此笔记：
                {backlinks.length
                  ? backlinks.map((note) => (
                      <button key={note.id} onClick={() => setSelected(note.id)}>
                        {note.title}
                      </button>
                    ))
                  : '暂无'}
              </div>
              <div>
                历史版本：
                {(active.versions ?? []).length === 0
                  ? '暂无'
                  : (active.versions ?? []).map((version, index) => (
                      <button
                        key={`${version.at}-${index}`}
                        onClick={() => change(active.id, { content: version.content })}
                      >
                        {new Date(version.at).toLocaleString()}
                      </button>
                    ))}
              </div>
            </div>
          ) : (
            <p>选择左侧笔记或新建笔记。</p>
          )}
        </div>
      </div>
    </div>
  )
}

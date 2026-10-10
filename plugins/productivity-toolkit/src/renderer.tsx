import React, { useEffect, useMemo, useState } from 'react'

type Api = {
  sendToBackend(message: unknown): Promise<unknown>
  notify(title: string, body?: string): void
}
type Note = {
  id: string
  title: string
  content: string
  tags: string[]
  date: string
  updatedAt: number
  deleted?: boolean
  versions: { at: number; content: string }[]
}
type Clip = { id: string; text: string; createdAt: number; pinned: boolean; tags: string[] }
const tabs = ['笔记与日记', '剪贴板', '随机决策', '换算工具'] as const
const field: React.CSSProperties = {
  padding: 8,
  border: '1px solid #cbd5e1',
  borderRadius: 7,
  boxSizing: 'border-box'
}

export default function ProductivityToolkit({ api }: { api: Api }) {
  const [tab, setTab] = useState<(typeof tabs)[number]>('笔记与日记')
  const [notes, setNotes] = useState<Note[]>([])
  const [clips, setClips] = useState<Clip[]>([])
  const [selected, setSelected] = useState('')
  const [query, setQuery] = useState('')
  const [showTrash, setShowTrash] = useState(false)
  const [clipboardPaused, setClipboardPaused] = useState(false)
  const [decision, setDecision] = useState('')
  const [choices, setChoices] = useState('选项 A\n选项 B\n选项 C')
  const [amount, setAmount] = useState(1)
  const [mode, setMode] = useState('公里 → 英里')
  useEffect(() => {
    void Promise.all([
      api.sendToBackend({ type: 'get', key: 'notes' }),
      api.sendToBackend({ type: 'get', key: 'clips' })
    ]).then(([n, c]) => {
      if (Array.isArray(n)) setNotes(n as Note[])
      if (Array.isArray(c)) setClips(c as Clip[])
    })
  }, [api])
  const persistNotes = (value: Note[]) => {
    setNotes(value)
    void api.sendToBackend({ type: 'set', key: 'notes', value })
  }
  const persistClips = (value: Clip[]) => {
    setClips(value)
    void api.sendToBackend({ type: 'set', key: 'clips', value })
  }
  const active = notes.find((note) => note.id === selected)
  const visible = useMemo(
    () =>
      notes.filter(
        (note) =>
          Boolean(note.deleted) === showTrash &&
          (note.title + note.content + note.tags.join(' '))
            .toLowerCase()
            .includes(query.toLowerCase())
      ),
    [notes, query, showTrash]
  )
  const createNote = () => {
    const now = Date.now()
    const note: Note = {
      id: crypto.randomUUID(),
      title: '未命名笔记',
      content: '',
      tags: [],
      date: new Date().toISOString().slice(0, 10),
      updatedAt: now,
      versions: []
    }
    persistNotes([note, ...notes])
    setSelected(note.id)
  }
  const updateNote = (patch: Partial<Note>) => {
    if (!active) return
    persistNotes(
      notes.map((note) =>
        note.id === active.id
          ? {
              ...note,
              ...patch,
              updatedAt: Date.now(),
              versions: [...note.versions.slice(-19), { at: Date.now(), content: note.content }]
            }
          : note
      )
    )
  }
  const collectClipboard = async () => {
    if (clipboardPaused) return
    try {
      const text = await navigator.clipboard.readText()
      if (!text.trim()) return
      const duplicate = clips.find((clip) => clip.text === text)
      if (duplicate) {
        persistClips([
          { ...duplicate, createdAt: Date.now() },
          ...clips.filter((clip) => clip.id !== duplicate.id)
        ])
        return
      }
      persistClips(
        [
          { id: crypto.randomUUID(), text, createdAt: Date.now(), pinned: false, tags: [] },
          ...clips
        ].slice(0, 500)
      )
    } catch {
      api.notify('无法读取剪贴板', '请在系统授权后重试')
    }
  }
  const randomize = () => {
    const parsed = choices
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const match = line.match(/^(.*?)(?:\s*\|\s*(\d+(?:\.\d+)?))?$/)!
        return { label: match[1], weight: Number(match[2] ?? 1) }
      })
    const total = parsed.reduce((sum, item) => sum + item.weight, 0)
    let point = Math.random() * total
    const winner = parsed.find((item) => (point -= item.weight) <= 0)
    setDecision(winner?.label ?? '没有可用选项')
  }
  const exportBackup = () => {
    const blob = new Blob([JSON.stringify({ version: 1, notes, clips }, null, 2)], {
      type: 'application/json'
    })
    const link = document.createElement('a')
    link.download = '笔记与效率备份.json'
    link.href = URL.createObjectURL(blob)
    link.click()
  }
  const converted = useMemo(() => {
    if (mode === '公里 → 英里') return amount * 0.621371
    if (mode === '摄氏 → 华氏') return (amount * 9) / 5 + 32
    if (mode === '千克 → 磅') return amount * 2.20462
    if (mode === '百分比') return amount / 100
    return amount
  }, [amount, mode])
  return (
    <div style={{ padding: 18, fontFamily: 'system-ui', color: '#172033' }}>
      <h2 style={{ marginTop: 0 }}>笔记与效率</h2>
      <div style={{ display: 'flex', gap: 8, marginBottom: 14 }}>
        {tabs.map((item) => (
          <button
            key={item}
            onClick={() => setTab(item)}
            style={{
              ...field,
              background: tab === item ? '#059669' : '#fff',
              color: tab === item ? '#fff' : '#172033'
            }}
          >
            {item}
          </button>
        ))}
      </div>
      {tab === '笔记与日记' && (
        <div style={{ display: 'grid', gridTemplateColumns: '260px 1fr', gap: 12 }}>
          <aside>
            <div style={{ display: 'flex', gap: 5 }}>
              <button onClick={createNote}>新建笔记</button>
              <button onClick={() => setShowTrash((value) => !value)}>
                {showTrash ? '返回笔记' : '回收站'}
              </button>
              <button onClick={exportBackup}>备份</button>
            </div>
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索标题、正文、标签"
              style={{ ...field, width: '100%', margin: '8px 0' }}
            />
            {visible.map((note) => (
              <div
                key={note.id}
                onClick={() => setSelected(note.id)}
                style={{
                  padding: 9,
                  borderBottom: '1px solid #e2e8f0',
                  background: selected === note.id ? '#d1fae5' : 'transparent',
                  cursor: 'pointer'
                }}
              >
                <strong>{note.title}</strong>
                <div>
                  {note.date} · {note.tags.join(' ')}
                </div>
              </div>
            ))}
          </aside>
          <main>
            {active ? (
              <>
                <input
                  value={active.title}
                  onChange={(event) => updateNote({ title: event.target.value })}
                  style={{ ...field, width: '100%', fontSize: 20 }}
                />
                <div style={{ display: 'flex', gap: 8, margin: '8px 0' }}>
                  <input
                    type="date"
                    value={active.date}
                    onChange={(event) => updateNote({ date: event.target.value })}
                  />
                  <input
                    value={active.tags.join(',')}
                    onChange={(event) =>
                      updateNote({
                        tags: event.target.value
                          .split(',')
                          .map((x) => x.trim())
                          .filter(Boolean)
                      })
                    }
                    placeholder="标签，逗号分隔"
                    style={{ ...field, flex: 1 }}
                  />
                  <button
                    onClick={() =>
                      persistNotes(
                        notes.map((note) =>
                          note.id === active.id ? { ...note, deleted: !note.deleted } : note
                        )
                      )
                    }
                  >
                    {active.deleted ? '恢复' : '移到回收站'}
                  </button>
                </div>
                <textarea
                  value={active.content}
                  onChange={(event) => updateNote({ content: event.target.value })}
                  placeholder="支持 Markdown、公式文本和 [[双向链接]]"
                  style={{ ...field, width: '100%', minHeight: 360, fontFamily: 'monospace' }}
                />
                <p>
                  版本记录 {active.versions.length} 条 · 反向链接{' '}
                  {notes.filter((note) => note.content.includes(`[[${active.title}]]`)).length} 条
                </p>
              </>
            ) : (
              <p>选择或新建一篇笔记。日记使用日期字段，普通笔记可留在任意日期。</p>
            )}
          </main>
        </div>
      )}
      {tab === '剪贴板' && (
        <div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button disabled={clipboardPaused} onClick={() => void collectClipboard()}>
              收集当前剪贴板
            </button>
            <button onClick={() => setClipboardPaused((value) => !value)}>
              {clipboardPaused ? '恢复收集' : '隐私暂停'}
            </button>
            <button onClick={() => persistClips([])}>清空历史</button>
          </div>
          {clips
            .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.createdAt - a.createdAt)
            .map((clip) => (
              <div
                key={clip.id}
                style={{ padding: 10, borderBottom: '1px solid #e2e8f0', display: 'flex', gap: 8 }}
              >
                <button
                  onClick={() =>
                    persistClips(
                      clips.map((item) =>
                        item.id === clip.id ? { ...item, pinned: !item.pinned } : item
                      )
                    )
                  }
                >
                  {clip.pinned ? '取消置顶' : '置顶'}
                </button>
                <pre style={{ whiteSpace: 'pre-wrap', flex: 1, margin: 0 }}>{clip.text}</pre>
                <button onClick={() => void navigator.clipboard.writeText(clip.text)}>复制</button>
                <button
                  onClick={() => {
                    const note: Note = {
                      id: crypto.randomUUID(),
                      title: '剪贴板收集',
                      content: clip.text,
                      tags: ['剪贴板'],
                      date: new Date().toISOString().slice(0, 10),
                      updatedAt: Date.now(),
                      versions: []
                    }
                    persistNotes([note, ...notes])
                    setSelected(note.id)
                    setTab('笔记与日记')
                  }}
                >
                  转为笔记
                </button>
              </div>
            ))}
        </div>
      )}
      {tab === '随机决策' && (
        <div>
          <p>每行一个选项；可用“选项 | 权重”。仅用于娱乐和日常决策，不用于赌博、法律抽签或投资。</p>
          <textarea
            value={choices}
            onChange={(event) => setChoices(event.target.value)}
            style={{ ...field, width: '100%', minHeight: 220 }}
          />
          <button onClick={randomize} style={{ marginTop: 8 }}>
            权重抽取
          </button>
          <button
            onClick={() => {
              const items = choices.split(/\r?\n/).filter(Boolean)
              setDecision(items.sort(() => Math.random() - 0.5).join('、'))
            }}
          >
            随机排序/分组
          </button>
          <h3>结果：{decision}</h3>
        </div>
      )}
      {tab === '换算工具' && (
        <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
          <input
            type="number"
            value={amount}
            onChange={(event) => setAmount(Number(event.target.value))}
          />
          <select value={mode} onChange={(event) => setMode(event.target.value)}>
            {['公里 → 英里', '摄氏 → 华氏', '千克 → 磅', '百分比'].map((item) => (
              <option key={item}>{item}</option>
            ))}
          </select>
          <strong>{converted.toFixed(4)}</strong>
          <p>汇率需联网数据源，旧“实时汇率”在兼容期仍可只读查询。</p>
        </div>
      )}
    </div>
  )
}

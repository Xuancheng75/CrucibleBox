import { useState, useCallback } from 'react'
import type { CSSProperties } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'

type TabId = 'json' | 'base64' | 'url' | 'timestamp' | 'uuid' | 'regex' | 'hash'

const TABS: { id: TabId; label: string }[] = [
  { id: 'json', label: 'JSON' },
  { id: 'base64', label: 'Base64' },
  { id: 'url', label: 'URL' },
  { id: 'timestamp', label: '时间戳' },
  { id: 'uuid', label: 'UUID' },
  { id: 'regex', label: '正则' },
  { id: 'hash', label: '哈希' }
]

const s: Record<string, CSSProperties> = {
  page: {
    minHeight: '100%',
    padding: 20,
    background: 'var(--ob-color-bg-layout, #f7f9fb)',
    color: 'var(--ob-color-text, #1f2933)',
    fontFamily: 'var(--ob-font-family, system-ui, sans-serif)'
  },
  tabs: {
    display: 'flex',
    gap: 4,
    marginBottom: 16,
    flexWrap: 'wrap'
  },
  tab: {
    padding: '6px 14px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text-secondary, #666)',
    cursor: 'pointer',
    fontSize: 13,
    fontWeight: 500
  },
  tabActive: {
    padding: '6px 14px',
    border: '1px solid var(--ob-color-primary, #2563eb)',
    borderRadius: 6,
    background: 'var(--ob-color-primary, #2563eb)',
    color: '#fff',
    cursor: 'pointer',
    fontSize: 13,
    fontWeight: 600
  },
  textarea: {
    width: '100%',
    minHeight: 120,
    padding: 10,
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    fontSize: 13,
    fontFamily: 'Consolas, Monaco, monospace',
    boxSizing: 'border-box' as const,
    resize: 'vertical' as const,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)'
  },
  btn: {
    padding: '8px 16px',
    border: 'none',
    borderRadius: 6,
    background: 'var(--ob-color-primary, #2563eb)',
    color: '#fff',
    fontSize: 13,
    fontWeight: 600,
    cursor: 'pointer',
    marginRight: 8,
    marginBottom: 8
  },
  btnSecondary: {
    padding: '8px 16px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)',
    fontSize: 13,
    cursor: 'pointer',
    marginRight: 8,
    marginBottom: 8
  },
  output: {
    marginTop: 12,
    padding: 10,
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    background: 'var(--ob-color-bg-container, #fff)',
    fontSize: 13,
    fontFamily: 'Consolas, Monaco, monospace',
    whiteSpace: 'pre-wrap' as const,
    wordBreak: 'break-all' as const,
    maxHeight: 300,
    overflow: 'auto' as const
  },
  error: {
    marginTop: 8,
    padding: 8,
    borderRadius: 6,
    background: '#fff2f0',
    border: '1px solid #ffccc7',
    color: '#cf1322',
    fontSize: 12
  },
  label: {
    display: 'block',
    fontSize: 12,
    color: 'var(--ob-color-text-secondary, #666)',
    marginBottom: 4,
    marginTop: 10
  },
  input: {
    width: '100%',
    padding: '8px 10px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    fontSize: 13,
    boxSizing: 'border-box' as const,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)'
  },
  row: {
    display: 'flex',
    gap: 12,
    alignItems: 'flex-end',
    marginBottom: 8
  }
}

function JsonTab() {
  const [input, setInput] = useState('')
  const [output, setOutput] = useState('')
  const [error, setError] = useState('')
  const [query, setQuery] = useState('')

  const runQuery = () => {
    try {
      const parsed: unknown = JSON.parse(input)
      const value = query
        .trim()
        .replace(/^\$\.?/, '')
        .split('.')
        .filter(Boolean)
        .reduce<unknown>((current, key) => {
          if (current && typeof current === 'object' && key in current) {
            return (current as Record<string, unknown>)[key]
          }
          throw new Error(`未找到路径: ${key}`)
        }, parsed)
      setOutput(JSON.stringify(value, null, 2))
      setError('')
    } catch (e) {
      setError((e as Error).message)
      setOutput('')
    }
  }

  const format = () => {
    try {
      setOutput(JSON.stringify(JSON.parse(input), null, 2))
      setError('')
    } catch (e) {
      setError((e as Error).message)
      setOutput('')
    }
  }

  const minify = () => {
    try {
      setOutput(JSON.stringify(JSON.parse(input)))
      setError('')
    } catch (e) {
      setError((e as Error).message)
      setOutput('')
    }
  }

  const validate = () => {
    try {
      JSON.parse(input)
      setError('')
      setOutput('Valid JSON')
    } catch (e) {
      setError((e as Error).message)
      setOutput('')
    }
  }

  return (
    <div>
      <textarea
        style={s.textarea}
        placeholder='粘贴 JSON...'
        value={input}
        onChange={(e) => setInput(e.target.value)}
      />
      <div style={{ marginTop: 10 }}>
        <button style={s.btn} onClick={format}>格式化</button>
        <button style={s.btn} onClick={minify}>压缩</button>
        <button style={s.btnSecondary} onClick={validate}>校验</button>
      </div>
      <label style={s.label}>JSONPath 查询（支持 $.user.name 这类点路径）</label>
      <div style={{ ...s.row, alignItems: 'center' }}>
        <input style={s.input} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="$.data.items" />
        <button style={s.btnSecondary} onClick={runQuery}>查询</button>
      </div>
      {error && <div style={s.error}>{error}</div>}
      {output && <div style={s.output}>{output}</div>}
    </div>
  )
}

function Base64Tab() {
  const [input, setInput] = useState('')
  const [output, setOutput] = useState('')
  const [error, setError] = useState('')

  const encode = () => {
    try {
      setOutput(btoa(unescape(encodeURIComponent(input))))
      setError('')
    } catch (e) {
      setError((e as Error).message)
    }
  }

  const decode = () => {
    try {
      setOutput(decodeURIComponent(escape(atob(input))))
      setError('')
    } catch (e) {
      setError((e as Error).message)
    }
  }

  return (
    <div>
      <textarea style={s.textarea} placeholder="输入文本..." value={input} onChange={(e) => setInput(e.target.value)} />
      <div style={{ marginTop: 10 }}>
        <button style={s.btn} onClick={encode}>编码</button>
        <button style={s.btn} onClick={decode}>解码</button>
      </div>
      {error && <div style={s.error}>{error}</div>}
      {output && <div style={s.output}>{output}</div>}
    </div>
  )
}

function UrlTab() {
  const [input, setInput] = useState('')
  const [output, setOutput] = useState('')

  const encode = () => setOutput(encodeURIComponent(input))
  const decode = () => {
    try { setOutput(decodeURIComponent(input)) } catch (e) { setOutput((e as Error).message) }
  }

  return (
    <div>
      <textarea style={s.textarea} placeholder="输入文本或 URL..." value={input} onChange={(e) => setInput(e.target.value)} />
      <div style={{ marginTop: 10 }}>
        <button style={s.btn} onClick={encode}>编码</button>
        <button style={s.btn} onClick={decode}>解码</button>
      </div>
      {output && <div style={s.output}>{output}</div>}
    </div>
  )
}

function TimestampTab() {
  const [ts, setTs] = useState('')
  const [result, setResult] = useState('')

  const toHuman = () => {
    const n = Number(ts)
    const d = new Date(n < 1e12 ? n * 1000 : n)
    if (isNaN(d.getTime())) { setResult('无效时间戳'); return }
    setResult(`${d.toLocaleString('zh-CN')}\nISO: ${d.toISOString()}\nUTC: ${d.toUTCString()}`)
  }

  const toTimestamp = () => {
    const d = new Date(ts)
    if (isNaN(d.getTime())) { setResult('无效日期'); return }
    setResult(`秒: ${Math.floor(d.getTime() / 1000)}\n毫秒: ${d.getTime()}`)
  }

  const now = () => {
    setTs(String(Math.floor(Date.now() / 1000)))
    toHumanFromValue(Math.floor(Date.now() / 1000))
  }

  const toHumanFromValue = (v: number) => {
    const d = new Date(v < 1e12 ? v * 1000 : v)
    setResult(`${d.toLocaleString('zh-CN')}\nISO: ${d.toISOString()}\nUTC: ${d.toUTCString()}`)
  }

  return (
    <div>
      <div style={s.row}>
        <div style={{ flex: 1 }}>
          <label style={s.label}>输入时间戳或日期字符串</label>
          <input style={s.input} value={ts} onChange={(e) => setTs(e.target.value)} placeholder="1700000000 或 2024-01-01" />
        </div>
      </div>
      <div style={{ marginTop: 10 }}>
        <button style={s.btn} onClick={toHuman}>时间戳 → 日期</button>
        <button style={s.btn} onClick={toTimestamp}>日期 → 时间戳</button>
        <button style={s.btnSecondary} onClick={now}>当前时间戳</button>
      </div>
      {result && <div style={s.output}>{result}</div>}
    </div>
  )
}

function UuidTab() {
  const [uuids, setUuids] = useState<string[]>([])

  const generate = () => {
    const arr = new Uint8Array(16)
    crypto.getRandomValues(arr)
    arr[6] = (arr[6] & 0x0f) | 0x40
    arr[8] = (arr[8] & 0x3f) | 0x80
    const hex = Array.from(arr, (b) => b.toString(16).padStart(2, '0')).join('')
    const uuid = `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
    setUuids((prev) => [uuid, ...prev].slice(0, 20))
  }

  const generateBatch = () => {
    const batch: string[] = []
    for (let i = 0; i < 10; i++) {
      const arr = new Uint8Array(16)
      crypto.getRandomValues(arr)
      arr[6] = (arr[6] & 0x0f) | 0x40
      arr[8] = (arr[8] & 0x3f) | 0x80
      const hex = Array.from(arr, (b) => b.toString(16).padStart(2, '0')).join('')
      batch.push(`${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`)
    }
    setUuids((prev) => [...batch, ...prev].slice(0, 50))
  }

  const copy = (text: string) => {
    navigator.clipboard?.writeText(text).catch(() => {})
  }

  return (
    <div>
      <div>
        <button style={s.btn} onClick={generate}>生成一个</button>
        <button style={s.btn} onClick={generateBatch}>批量生成 10 个</button>
      </div>
      {uuids.length > 0 && (
        <div style={{ ...s.output, marginTop: 12 }}>
          {uuids.map((u, i) => (
            <div key={i} style={{ cursor: 'pointer', padding: '2px 0' }} onClick={() => copy(u)} title="点击复制">
              {u}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function RegexTab() {
  const [pattern, setPattern] = useState('')
  const [flags, setFlags] = useState('g')
  const [text, setText] = useState('')
  const [matches, setMatches] = useState<{ match: string; index: number; groups: string[] }[]>([])
  const [error, setError] = useState('')

  const test = () => {
    try {
      const re = new RegExp(pattern, flags)
      const results: { match: string; index: number; groups: string[] }[] = []
      let m: RegExpExecArray | null
      if (flags.includes('g')) {
        while ((m = re.exec(text)) !== null) {
          results.push({ match: m[0], index: m.index, groups: m.slice(1) })
          if (!m[0]) re.lastIndex++
        }
      } else {
        m = re.exec(text)
        if (m) results.push({ match: m[0], index: m.index, groups: m.slice(1) })
      }
      setMatches(results)
      setError('')
    } catch (e) {
      setError((e as Error).message)
      setMatches([])
    }
  }

  return (
    <div>
      <div style={s.row}>
        <div style={{ flex: 2 }}>
          <label style={s.label}>正则表达式</label>
          <input style={s.input} value={pattern} onChange={(e) => setPattern(e.target.value)} placeholder="\\d+" />
        </div>
        <div style={{ flex: 1 }}>
          <label style={s.label}>标志</label>
          <input style={s.input} value={flags} onChange={(e) => setFlags(e.target.value)} placeholder="g" />
        </div>
      </div>
      <label style={s.label}>测试文本</label>
      <textarea style={s.textarea} value={text} onChange={(e) => setText(e.target.value)} placeholder="输入测试文本..." />
      <div style={{ marginTop: 10 }}>
        <button style={s.btn} onClick={test}>匹配</button>
      </div>
      {error && <div style={s.error}>{error}</div>}
      {matches.length > 0 && (
        <div style={s.output}>
          <div style={{ marginBottom: 6, fontWeight: 600 }}>找到 {matches.length} 个匹配</div>
          {matches.map((m, i) => (
            <div key={i} style={{ padding: '2px 0', borderBottom: '1px solid var(--ob-color-border-secondary, #eee)' }}>
              <span style={{ color: 'var(--ob-color-primary, #2563eb)' }}>"{m.match}"</span>
              <span style={{ color: '#999', marginLeft: 8 }}>@{m.index}</span>
              {m.groups.length > 0 && <span style={{ color: '#666', marginLeft: 8 }}>groups: [{m.groups.join(', ')}]</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function HashTab() {
  const [input, setInput] = useState('')
  const [hashes, setHashes] = useState<Record<string, string>>({})

  const compute = useCallback(async () => {
    const encoder = new TextEncoder()
    const data = encoder.encode(input)
    const algos = ['SHA-1', 'SHA-256', 'SHA-384', 'SHA-512']
    const results: Record<string, string> = {}
    for (const algo of algos) {
      const buf = await crypto.subtle.digest(algo, data)
      results[algo] = Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0')).join('')
    }
    let md5: string
    try {
      const buf = await crypto.subtle.digest('MD5' as AlgorithmIdentifier, data)
      md5 = Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0')).join('')
    } catch {
      md5 = '(当前环境不支持 MD5)'
    }
    results['MD5'] = md5
    setHashes(results)
  }, [input])

  const copy = (text: string) => {
    navigator.clipboard?.writeText(text).catch(() => {})
  }

  return (
    <div>
      <textarea style={s.textarea} value={input} onChange={(e) => setInput(e.target.value)} placeholder="输入文本..." />
      <div style={{ marginTop: 10 }}>
        <button style={s.btn} onClick={compute}>计算哈希</button>
      </div>
      {Object.keys(hashes).length > 0 && (
        <div style={s.output}>
          {Object.entries(hashes).map(([algo, hash]) => (
            <div key={algo} style={{ padding: '4px 0', borderBottom: '1px solid var(--ob-color-border-secondary, #eee)', cursor: 'pointer' }} onClick={() => copy(hash)} title="点击复制">
              <strong>{algo}</strong>: {hash}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

export default function JsonToolkitPlugin(_props: PluginRenderProps) {
  const [activeTab, setActiveTab] = useState<TabId>('json')

  return (
    <div style={s.page}>
      <h2 style={{ margin: '0 0 16px', fontSize: 20 }}>JSON/文本工具箱</h2>
      <div style={s.tabs}>
        {TABS.map((tab) => (
          <button
            key={tab.id}
            style={activeTab === tab.id ? s.tabActive : s.tab}
            onClick={() => setActiveTab(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {activeTab === 'json' && <JsonTab />}
      {activeTab === 'base64' && <Base64Tab />}
      {activeTab === 'url' && <UrlTab />}
      {activeTab === 'timestamp' && <TimestampTab />}
      {activeTab === 'uuid' && <UuidTab />}
      {activeTab === 'regex' && <RegexTab />}
      {activeTab === 'hash' && <HashTab />}
    </div>
  )
}

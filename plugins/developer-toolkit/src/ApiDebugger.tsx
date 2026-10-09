import { useEffect, useState } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import {
  Button,
  ErrorPanel,
  Field,
  PluginPage,
  SplitPane,
  TextInput,
  Toolbar
} from '@cruciblebox/plugin-ui'

type SavedRequest = {
  id: string
  name: string
  method: string
  url: string
  headers: string
  body: string
}
type HistoryEntry = SavedRequest & { status: number; durationMs: number; at: number }
type SavedState = {
  collections: SavedRequest[]
  history: HistoryEntry[]
  environment: Record<string, string>
}
const emptyState: SavedState = { collections: [], history: [], environment: {} }
export default function ApiDebugger({ api }: PluginRenderProps) {
  const [method, setMethod] = useState('GET')
  const [url, setUrl] = useState('https://example.com')
  const [headers, setHeaders] = useState('{}')
  const [body, setBody] = useState('')
  const [name, setName] = useState('未命名请求')
  const [selectedRequestId, setSelectedRequestId] = useState<string | null>(null)
  const [auth, setAuth] = useState('')
  const [state, setState] = useState<SavedState>(emptyState)
  const [response, setResponse] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    void api.sendToBackend({ type: 'getState' }).then((value) => {
      const saved = value as Partial<SavedState>
      setState({
        collections: (saved.collections ?? []).map((item) => ({
          ...item,
          id: item.id || crypto.randomUUID()
        })),
        history: (saved.history ?? []).map((item) => ({
          ...item,
          id: item.id || crypto.randomUUID()
        })),
        environment: saved.environment ?? {}
      })
    })
  }, [api])
  const persist = (next: SavedState) => {
    setState(next)
    void api.sendToBackend({ type: 'saveState', state: next }).catch((cause: unknown) => {
      setError(cause instanceof Error ? cause.message : String(cause))
    })
  }
  const substitute = (value: string, field: string) =>
    value.replace(/\{\{([\w.-]+)\}\}/g, (_, key: string) => {
      const resolved = state.environment[key]
      if (resolved === undefined) throw new Error(`${field}中的环境变量 ${key} 未定义`)
      return resolved
    })
  const request = async () => {
    setBusy(true)
    setError('')
    try {
      const parsedHeaders = JSON.parse(substitute(headers, '请求头')) as Record<string, string>
      if (auth.trim()) parsedHeaders.Authorization = `Bearer ${substitute(auth.trim(), '认证')}`
      const resolvedUrl = substitute(url, '请求地址')
      const result = (await api.sendToBackend({
        type: 'request',
        method,
        url: resolvedUrl,
        headers: parsedHeaders,
        body: substitute(body, '请求体')
      })) as {
        status?: number
        statusText?: string
        headers?: Record<string, string>
        body?: string
        durationMs?: number
        error?: string
      }
      if (result.error) throw new Error(result.error)
      setResponse(
        `${result.status} ${result.statusText} · ${result.durationMs} ms\n${JSON.stringify(result.headers, null, 2)}\n\n${result.body ?? ''}`
      )
      persist({
        ...state,
        history: [
          {
            name,
            id: crypto.randomUUID(),
            method,
            url,
            headers,
            body,
            status: result.status ?? 0,
            durationMs: result.durationMs ?? 0,
            at: Date.now()
          },
          ...state.history
        ].slice(0, 100)
      })
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }
  const saveRequest = () => {
    const id = selectedRequestId ?? crypto.randomUUID()
    const saved: SavedRequest = { id, name, method, url, headers, body }
    persist({
      ...state,
      collections: selectedRequestId
        ? state.collections.map((item) => (item.id === id ? saved : item))
        : [...state.collections, saved]
    })
    setSelectedRequestId(id)
  }
  const openRequest = (item: SavedRequest) => {
    setSelectedRequestId(state.collections.some((saved) => saved.id === item.id) ? item.id : null)
    setName(item.name)
    setMethod(item.method)
    setUrl(item.url)
    setHeaders(item.headers)
    setBody(item.body)
  }
  const exportCollection = () => {
    const safeCollections = state.collections.map((item) => {
      let safeHeaders: string
      try {
        const parsed = JSON.parse(item.headers) as Record<string, unknown>
        for (const key of Object.keys(parsed)) {
          if (/^(authorization|cookie|proxy-authorization|x-api-key)$/i.test(key)) {
            delete parsed[key]
          }
        }
        safeHeaders = JSON.stringify(parsed, null, 2)
      } catch {
        safeHeaders = '{}'
      }
      return { ...item, headers: safeHeaders }
    })
    const link = document.createElement('a')
    const objectUrl = URL.createObjectURL(
      new Blob([JSON.stringify({ collections: safeCollections }, null, 2)], {
        type: 'application/json'
      })
    )
    link.href = objectUrl
    link.download = '接口调试集合.json'
    link.click()
    window.setTimeout(() => URL.revokeObjectURL(objectUrl), 1000)
  }
  const importCollection = async (file?: File) => {
    if (!file) return
    try {
      if (file.size > 2 * 1024 * 1024) throw new Error('集合文件不得超过 2 MB')
      const parsed = JSON.parse(await file.text()) as { collections?: unknown }
      if (!Array.isArray(parsed.collections) || parsed.collections.length > 1000) {
        throw new Error('集合文件缺少有效的请求列表')
      }
      const incoming: SavedRequest[] = parsed.collections.map((item: unknown) => {
        if (typeof item !== 'object' || item === null) throw new Error('集合请求格式无效')
        const request = item as Record<string, unknown>
        if (
          !['name', 'method', 'url', 'headers', 'body'].every(
            (key) => typeof request[key] === 'string' && (request[key] as string).length < 100_000
          )
        ) {
          throw new Error('集合请求字段无效')
        }
        return {
          id:
            typeof request.id === 'string' && request.id.length < 128
              ? request.id
              : crypto.randomUUID(),
          name: request.name as string,
          method: request.method as string,
          url: request.url as string,
          headers: request.headers as string,
          body: request.body as string
        }
      })
      const byId = new Map(state.collections.map((item) => [item.id, item]))
      for (const item of incoming) byId.set(item.id, item)
      persist({ ...state, collections: [...byId.values()] })
      setError('')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }
  return (
    <PluginPage>
      <Toolbar>
        <TextInput
          aria-label="请求名称"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <select
          className="cbx-plugin-input"
          aria-label="请求方法"
          value={method}
          onChange={(event) => setMethod(event.target.value)}
        >
          {['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD'].map((item) => (
            <option key={item}>{item}</option>
          ))}
        </select>
        <TextInput
          aria-label="请求地址"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          style={{ flex: '1 1 260px' }}
        />
        <Button variant="primary" disabled={busy} onClick={() => void request()}>
          {busy ? '请求中…' : '发送'}
        </Button>
        <Button onClick={saveRequest}>{selectedRequestId ? '更新集合请求' : '保存到集合'}</Button>
        <Button
          onClick={() => {
            setSelectedRequestId(null)
            setName('未命名请求')
            setMethod('GET')
            setUrl('')
            setHeaders('{}')
            setBody('')
          }}
        >
          新建
        </Button>
      </Toolbar>
      <SplitPane>
        <div>
          <Field label="请求头 JSON">
            <textarea
              aria-label="请求头 JSON"
              className="cbx-plugin-input"
              value={headers}
              onChange={(event) => setHeaders(event.target.value)}
              rows={5}
              style={{ width: '100%', resize: 'vertical' }}
            />
          </Field>
          <Field label="Bearer Token（仅当前请求）">
            <TextInput
              aria-label="Bearer Token"
              type="password"
              value={auth}
              onChange={(event) => setAuth(event.target.value)}
              style={{ width: '100%' }}
            />
          </Field>
          <Field label="请求体／GraphQL JSON">
            <textarea
              aria-label="请求体或 GraphQL JSON"
              className="cbx-plugin-input"
              value={body}
              onChange={(event) => setBody(event.target.value)}
              rows={8}
              style={{ width: '100%', resize: 'vertical' }}
            />
          </Field>
        </div>
        <Field label="响应">
          <textarea
            aria-label="响应"
            className="cbx-plugin-input"
            value={response}
            readOnly
            rows={18}
            style={{ width: '100%', resize: 'vertical' }}
          />
        </Field>
      </SplitPane>
      {error && <ErrorPanel title="请求失败" detail={error} />}
      <details>
        <summary>环境变量（JSON，对地址、请求头和请求体中的双花括号生效）</summary>
        <textarea
          rows={4}
          defaultValue={JSON.stringify(state.environment, null, 2)}
          onBlur={(event) => {
            try {
              persist({
                ...state,
                environment: JSON.parse(event.target.value) as Record<string, string>
              })
              setError('')
            } catch {
              setError('环境变量必须是 JSON 对象')
            }
          }}
          style={{ width: '100%' }}
        />
      </details>
      <section>
        <h3>
          请求集合 <button onClick={exportCollection}>导出集合</button>
          <label style={{ marginLeft: 8 }}>
            导入集合
            <input
              type="file"
              accept="application/json,.json"
              onChange={(event) => void importCollection(event.target.files?.[0])}
            />
          </label>
        </h3>
        {state.collections.map((item) => (
          <div key={item.id}>
            <button onClick={() => openRequest(item)}>
              {item.method} {item.name}
            </button>
            <button
              onClick={() => {
                persist({
                  ...state,
                  collections: state.collections.filter((current) => current.id !== item.id)
                })
                if (selectedRequestId === item.id) setSelectedRequestId(null)
              }}
            >
              删除
            </button>
          </div>
        ))}
      </section>
      <section>
        <h3>历史记录</h3>
        {state.history.slice(0, 25).map((item, index) => (
          <div key={`${item.at}-${index}`}>
            <button onClick={() => openRequest(item)}>
              {item.status} · {item.method} {item.url} · {item.durationMs} ms
            </button>
          </div>
        ))}
      </section>
    </PluginPage>
  )
}

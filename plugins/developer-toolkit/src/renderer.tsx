import React, { useState } from 'react'

const tabs = ['结构化数据', '文本与编码', '差异与查询', '接口调试'] as const
const area: React.CSSProperties = {
  width: '100%',
  minHeight: 260,
  boxSizing: 'border-box',
  padding: 12,
  fontFamily: 'ui-monospace, Consolas, monospace',
  border: '1px solid #cad3df',
  borderRadius: 8
}

export function csvToObjects(input: string) {
  const rows = input
    .trim()
    .split(/\r?\n/)
    .map((line) => line.split(',').map((cell) => cell.trim()))
  const headers = rows.shift() ?? []
  return rows.map((row) =>
    Object.fromEntries(headers.map((header, index) => [header, row[index] ?? '']))
  )
}
function xmlToObject(input: string) {
  const root = new DOMParser().parseFromString(input, 'application/xml').documentElement
  const walk = (node: Element): unknown => {
    const children = Array.from(node.children)
    if (!children.length) return node.textContent ?? ''
    return Object.fromEntries(children.map((child) => [child.tagName, walk(child)]))
  }
  return { [root.tagName]: walk(root) }
}
export function keyValueDocument(input: string) {
  return Object.fromEntries(
    input
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith('#'))
      .map((line) => {
        const separator = line.includes('=') ? '=' : ':'
        const index = line.indexOf(separator)
        const key = line.slice(0, index).trim()
        const raw = line
          .slice(index + 1)
          .trim()
          .replace(/^['"]|['"]$/g, '')
        const value =
          raw === 'true'
            ? true
            : raw === 'false'
              ? false
              : Number.isNaN(Number(raw))
                ? raw
                : Number(raw)
        return [key, value]
      })
  )
}
export function parseJson5(input: string) {
  return JSON.parse(
    input
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/(^|\s)\/\/.*$/gm, '')
      .replace(/,\s*([}\]])/g, '$1')
      .replace(/([{,]\s*)([A-Za-z_$][\w$]*)(\s*:)/g, '$1"$2"$3')
      .replace(/'/g, '"')
  )
}
function decodeJwt(value: string) {
  const [header, payload] = value.split('.')
  const decode = (part: string) =>
    JSON.parse(decodeURIComponent(escape(atob(part.replace(/-/g, '+').replace(/_/g, '/')))))
  return { header: decode(header), payload: decode(payload) }
}

export default function DeveloperToolkit() {
  const [tab, setTab] = useState<(typeof tabs)[number]>('结构化数据')
  const [input, setInput] = useState('{\n  "hello": "world"\n}')
  const [output, setOutput] = useState('')
  const [second, setSecond] = useState('')
  const [method, setMethod] = useState('GET')
  const [url, setUrl] = useState('https://example.com')
  const [headers, setHeaders] = useState('{}')
  const [body, setBody] = useState('')
  const [busy, setBusy] = useState(false)
  const run = (fn: () => unknown) => {
    try {
      const value = fn()
      setOutput(typeof value === 'string' ? value : JSON.stringify(value, null, 2))
    } catch (error) {
      setOutput(`错误：${(error as Error).message}`)
    }
  }
  const hash = async () => {
    const bytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input))
    setOutput(
      Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, '0')).join('')
    )
  }
  const request = async () => {
    setBusy(true)
    try {
      const response = await fetch(url, {
        method,
        headers: JSON.parse(headers || '{}'),
        body: ['GET', 'HEAD'].includes(method) ? undefined : body
      })
      const text = await response.text()
      setOutput(
        `${response.status} ${response.statusText}\n${[...response.headers].map(([key, value]) => `${key}: ${value}`).join('\n')}\n\n${text}`
      )
    } catch (error) {
      setOutput(`请求失败：${(error as Error).message}`)
    } finally {
      setBusy(false)
    }
  }
  return (
    <div style={{ padding: 18, fontFamily: 'system-ui', color: '#172033' }}>
      <h2 style={{ marginTop: 0 }}>数据与接口工具</h2>
      <div style={{ display: 'flex', gap: 8, marginBottom: 15 }}>
        {tabs.map((item) => (
          <button
            key={item}
            onClick={() => setTab(item)}
            style={{
              padding: '8px 14px',
              border: '1px solid #cbd5e1',
              borderRadius: 8,
              background: tab === item ? '#0891b2' : '#fff',
              color: tab === item ? '#fff' : '#172033'
            }}
          >
            {item}
          </button>
        ))}
      </div>
      {tab !== '接口调试' && (
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
          <textarea value={input} onChange={(event) => setInput(event.target.value)} style={area} />
          <textarea value={output} readOnly style={area} />
        </div>
      )}
      {tab === '结构化数据' && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 10 }}>
          <button onClick={() => run(() => JSON.stringify(JSON.parse(input), null, 2))}>
            格式化 JSON
          </button>
          <button onClick={() => run(() => JSON.stringify(JSON.parse(input)))}>压缩 JSON</button>
          <button onClick={() => run(() => JSON.stringify(parseJson5(input), null, 2))}>
            JSON5 转 JSON
          </button>
          <button onClick={() => run(() => csvToObjects(input))}>CSV 转 JSON</button>
          <button onClick={() => run(() => xmlToObject(input))}>XML 转 JSON</button>
          <button onClick={() => run(() => JSON.stringify(keyValueDocument(input), null, 2))}>
            YAML/TOML 简表转 JSON
          </button>
          <button
            onClick={() =>
              run(() =>
                Object.entries(JSON.parse(input) as Record<string, unknown>)
                  .map(([key, value]) => `${key}=${String(value)}`)
                  .join('\n')
              )
            }
          >
            JSON 转键值文本
          </button>
          <button
            onClick={() =>
              run(() => ({
                ...(JSON.parse(input) as object),
                ...(JSON.parse(second || '{}') as object)
              }))
            }
          >
            合并第二份 JSON
          </button>
        </div>
      )}
      {tab === '文本与编码' && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 10 }}>
          <button onClick={() => setOutput(btoa(unescape(encodeURIComponent(input))))}>
            Base64 编码
          </button>
          <button onClick={() => run(() => decodeURIComponent(escape(atob(input))))}>
            Base64 解码
          </button>
          <button onClick={() => setOutput(encodeURIComponent(input))}>URL 编码</button>
          <button onClick={() => run(() => decodeURIComponent(input))}>URL 解码</button>
          <button
            onClick={() =>
              setOutput(
                Array.from(new TextEncoder().encode(input), (byte) =>
                  byte.toString(16).padStart(2, '0')
                ).join(' ')
              )
            }
          >
            十六进制
          </button>
          <button onClick={() => setOutput(crypto.randomUUID())}>UUID</button>
          <button onClick={() => setOutput(`${Date.now()}\n${new Date().toISOString()}`)}>
            当前时间戳
          </button>
          <button onClick={() => run(() => decodeJwt(input))}>解析 JWT</button>
          <button onClick={() => void hash()}>计算 SHA-256</button>
          <p style={{ width: '100%' }}>
            此处的 SHA-256 是用户主动使用的数据工具，不参与宿主下载、安装或门禁。
          </p>
        </div>
      )}
      {tab === '差异与查询' && (
        <div>
          <textarea
            value={second}
            onChange={(event) => setSecond(event.target.value)}
            placeholder="对比文本或点路径，例如 user.profile.name"
            style={{ ...area, minHeight: 100, marginTop: 10 }}
          />
          <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
            <button
              onClick={() =>
                run(() => {
                  const left = input.split(/\r?\n/),
                    right = second.split(/\r?\n/)
                  return Array.from({ length: Math.max(left.length, right.length) }, (_, i) =>
                    left[i] === right[i]
                      ? `  ${left[i] ?? ''}`
                      : `- ${left[i] ?? ''}\n+ ${right[i] ?? ''}`
                  ).join('\n')
                })
              }
            >
              文本差异
            </button>
            <button
              onClick={() =>
                run(() =>
                  second
                    .split('.')
                    .filter(Boolean)
                    .reduce<unknown>(
                      (value, key) => (value as Record<string, unknown>)?.[key],
                      JSON.parse(input)
                    )
                )
              }
            >
              点路径查询
            </button>
            <button
              onClick={() =>
                run(() => {
                  const match = second.match(/^\/(.*)\/([gimsuy]*)$/)
                  const regex = match ? new RegExp(match[1], match[2]) : new RegExp(second, 'g')
                  return Array.from(input.matchAll(regex), (item) => ({
                    value: item[0],
                    index: item.index,
                    groups: item.groups
                  }))
                })
              }
            >
              正则匹配
            </button>
          </div>
        </div>
      )}
      {tab === '接口调试' && (
        <div>
          <div style={{ display: 'flex', gap: 8 }}>
            <select value={method} onChange={(event) => setMethod(event.target.value)}>
              {['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD'].map((item) => (
                <option key={item}>{item}</option>
              ))}
            </select>
            <input
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              style={{ flex: 1 }}
            />
            <button disabled={busy} onClick={() => void request()}>
              {busy ? '请求中…' : '发送请求'}
            </button>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12, marginTop: 10 }}>
            <div>
              <label>请求头 JSON</label>
              <textarea
                value={headers}
                onChange={(event) => setHeaders(event.target.value)}
                style={{ ...area, minHeight: 120 }}
              />
              <label>请求体</label>
              <textarea
                value={body}
                onChange={(event) => setBody(event.target.value)}
                style={{ ...area, minHeight: 160 }}
              />
            </div>
            <textarea value={output} readOnly style={{ ...area, minHeight: 320 }} />
          </div>
          <p>
            支持 REST/GraphQL 请求；WebSocket 和脚本化集合将在后续正式版继续扩展，不提供压力测试。
          </p>
        </div>
      )}
    </div>
  )
}

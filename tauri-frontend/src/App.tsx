import { useCallback, useState } from 'react'

interface ProcessMemory {
  working_set_kib: number
  private_kib: number
  pid: number
}

function App() {
  const [memory, setMemory] = useState<ProcessMemory | null>(null)
  const [error, setError] = useState<string | null>(null)

  const probe = useCallback(async () => {
    try {
      // 在 Tauri 运行时注入 window.__TAURI_INTERNALS__；纯浏览器下不可用
      const core = (await import('@tauri-apps/api/core')).default
      const result = await core.invoke<ProcessMemory>('get_process_memory')
      setMemory(result)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  return (
    <div
      style={{
        padding: 24,
        maxWidth: 720,
        margin: '0 auto',
        background: 'var(--ob-color-bg-container)',
        border: '1px solid var(--ob-color-border)',
        borderRadius: 10,
        boxShadow: '0 4px 16px rgba(0,0,0,0.06)'
      }}
    >
      <h1 style={{ color: 'var(--ob-color-primary)', fontSize: 22, marginBottom: 8 }}>
        CrucibleBox{' '}
        <span style={{ fontSize: 13, color: 'var(--ob-color-text)' }}>Tauri 2 骨架</span>
      </h1>
      <p style={{ marginBottom: 12 }}>
        进程内存基准（P4）：点击按钮读取 Rust core 进程的 working set / private usage。
      </p>
      <button
        onClick={probe}
        style={{
          padding: '8px 16px',
          border: 'none',
          borderRadius: 6,
          background: 'var(--ob-color-primary)',
          color: '#fff',
          cursor: 'pointer',
          marginRight: 8
        }}
      >
        查询进程内存
      </button>
      {memory && (
        <pre
          style={{ marginTop: 12, background: 'var(--ob-color-bg)', padding: 12, borderRadius: 6 }}
        >
          working_set_kib: {memory.working_set_kib}
          {'\n'}private_kib: {memory.private_kib}
          {'\n'}pid: {memory.pid}
        </pre>
      )}
      {error && (
        <pre
          style={{
            marginTop: 12,
            background: '#fff1f0',
            padding: 12,
            borderRadius: 6,
            color: '#cf1322'
          }}
        >
          error: {error}
        </pre>
      )}
      <p style={{ marginTop: 12, fontSize: 12, color: 'var(--ob-color-text)' }}>
        frontend origin: {window.location.origin}
      </p>
    </div>
  )
}

export default App

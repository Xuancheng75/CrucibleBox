import { useCallback, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { check as checkUpdate } from '@tauri-apps/plugin-updater'
import { PluginHost } from './PluginHost'

interface ProcessMemory {
  working_set_kib: number
  private_kib: number
  pid: number
}

function App() {
  const [memory, setMemory] = useState<ProcessMemory | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [updateInfo, setUpdateInfo] = useState<string | null>(null)

  const probe = useCallback(async () => {
    try {
      const result = await invoke<ProcessMemory>('get_process_memory')
      setMemory(result)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const checkUpdates = useCallback(async () => {
    try {
      const update = await checkUpdate()
      if (update === null) {
        setUpdateInfo('当前已是最新版本')
      } else {
        setUpdateInfo(`发现新版本 ${update.version}（${update.date}），可下载更新`)
        // 1.8.4 最小接入：仅提示；完整下载/安装 UI 随前端迁移落地
        // await update.downloadAndInstall()
      }
    } catch (e) {
      setUpdateInfo(`更新检查失败（dev 环境或未配置签名密钥）：${e instanceof Error ? e.message : String(e)}`)
    }
  }, [])

  return (
    <div style={{ minHeight: '100vh', background: 'var(--ob-color-bg)' }}>
      <div
        style={{
          padding: 24,
          maxWidth: 980,
          margin: '0 auto',
          background: 'var(--ob-color-bg-container)',
          border: '1px solid var(--ob-color-border)',
          borderRadius: 10,
          boxShadow: '0 4px 16px rgba(0,0,0,0.06)'
        }}
      >
        <h1 style={{ color: 'var(--ob-color-primary)', fontSize: 22, marginBottom: 8 }}>
          CrucibleBox <span style={{ fontSize: 13, color: 'var(--ob-color-text)' }}>Tauri 2 迁移骨架</span>
        </h1>
        <button
          onClick={probe}
          style={{
            padding: '8px 16px',
            border: 'none',
            borderRadius: 6,
            background: 'var(--ob-color-primary)',
            color: '#fff',
            cursor: 'pointer',
            marginBottom: 16
          }}
        >
          查询进程内存
        </button>
        {memory && (
          <pre style={{ margin: '0 0 16px', background: 'var(--ob-color-bg)', padding: 12, borderRadius: 6 }}>
            working_set_kib: {memory.working_set_kib}
            {'\n'}private_kib: {memory.private_kib}
            {'\n'}pid: {memory.pid}
          </pre>
        )}
        {error && (
          <pre style={{ margin: '0 0 16px', background: '#fff1f0', padding: 12, borderRadius: 6, color: '#cf1322' }}>
            error: {error}
          </pre>
        )}
        <button
          onClick={checkUpdates}
          style={{
            padding: '8px 16px',
            border: 'none',
            borderRadius: 6,
            background: '#10b981',
            color: '#fff',
            cursor: 'pointer',
            marginBottom: 16,
            marginLeft: 8
          }}
        >
          检查更新
        </button>
        {updateInfo && (
          <p style={{ margin: '0 0 16px', fontSize: 13 }}>{updateInfo}</p>
        )}
        <PluginHost />
      </div>
    </div>
  )
}

export default App

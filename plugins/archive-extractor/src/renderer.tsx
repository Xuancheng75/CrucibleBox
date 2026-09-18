import { useCallback, useEffect, useRef, useState } from 'react'
import type { CSSProperties } from 'react'
import type { PluginConfig, PluginRenderProps } from 'cruciblebox-plugin-api'

type ConflictPolicy = 'ask' | 'overwrite' | 'skip' | 'rename'
type DestinationMode = 'same' | 'choose'

interface ArchiveEntry {
  path: string
  isDirectory: boolean
  size: number
}

interface ArchiveInfo {
  source: string
  format: string
  sourceSize: number
  fileCount: number
  directoryCount: number
  unpackedBytes: number
  encrypted: boolean
  suggestedDestination: string
  topLevel: string[]
  entriesPreview: ArchiveEntry[]
}

interface Preflight {
  info: ArchiveInfo
  destination: string
  conflicts: string[]
  disk: { availableBytes: number; requiredBytes: number; low: boolean }
}

interface TaskSnapshot {
  status: 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled'
  progress?: { stage: string; percent: number; message: string }
  result?: { destination: string; files: number; bytes: number; skipped: number; format: string }
  error?: { message?: string }
}

class ServiceError extends Error {
  constructor(readonly code: string, message: string) {
    super(message)
  }
}

const styles: Record<string, CSSProperties> = {
  page: {
    minHeight: '100%',
    padding: 22,
    boxSizing: 'border-box',
    color: 'var(--ob-color-text, #1f2937)',
    background: 'var(--ob-color-bg-layout, #f6f8fb)',
    fontFamily: 'var(--ob-font-family, system-ui, sans-serif)'
  },
  header: { display: 'flex', justifyContent: 'space-between', gap: 16, alignItems: 'flex-start', marginBottom: 18 },
  title: { margin: 0, fontSize: 22, fontWeight: 700 },
  subtitle: { margin: '7px 0 0', color: 'var(--ob-color-text-secondary, #667085)', fontSize: 13 },
  card: { background: 'var(--ob-color-bg-container, #fff)', border: '1px solid var(--ob-color-border-secondary, #e5e7eb)', borderRadius: 14, padding: 18, boxShadow: '0 4px 18px rgba(15, 23, 42, 0.04)' },
  drop: { border: '1px dashed var(--ob-color-primary, #0f766e)', borderRadius: 14, padding: '36px 20px', textAlign: 'center', cursor: 'pointer', background: 'color-mix(in srgb, var(--ob-color-primary, #0f766e) 5%, transparent)' },
  dropTitle: { margin: 0, fontSize: 16, fontWeight: 650 },
  dropText: { margin: '8px 0 0', fontSize: 12, color: 'var(--ob-color-text-secondary, #667085)' },
  button: { border: '1px solid var(--ob-color-border, #d0d5dd)', borderRadius: 8, background: 'var(--ob-color-bg-container, #fff)', color: 'var(--ob-color-text, #1f2937)', padding: '8px 13px', cursor: 'pointer', fontSize: 13 },
  primary: { border: 0, background: 'var(--ob-color-primary, #0f766e)', color: '#fff' },
  danger: { border: 0, background: '#b42318', color: '#fff' },
  muted: { color: 'var(--ob-color-text-secondary, #667085)', fontSize: 12 },
  row: { display: 'flex', justifyContent: 'space-between', gap: 14, alignItems: 'center', padding: '10px 0', borderBottom: '1px solid var(--ob-color-border-secondary, #eef0f3)' },
  progress: { height: 8, borderRadius: 5, overflow: 'hidden', background: 'var(--ob-color-border-secondary, #e5e7eb)', marginTop: 10 },
  progressFill: { height: '100%', borderRadius: 5, background: 'var(--ob-color-primary, #0f766e)', transition: 'width 180ms ease' },
  overlay: { position: 'fixed', inset: 0, zIndex: 10, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'rgba(15, 23, 42, 0.38)' },
  modal: { width: 'min(560px, calc(100vw - 40px))', maxHeight: '80vh', overflow: 'auto', background: 'var(--ob-color-bg-container, #fff)', borderRadius: 14, padding: 20, boxShadow: '0 18px 60px rgba(15, 23, 42, 0.24)' },
  input: { width: '100%', boxSizing: 'border-box', border: '1px solid var(--ob-color-border, #d0d5dd)', borderRadius: 8, padding: '9px 10px', background: 'var(--ob-color-bg-container, #fff)', color: 'var(--ob-color-text, #1f2937)' }
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const index = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)))
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`
}

function configValue<T>(config: PluginConfig, key: string, fallback: T): T {
  return (config[key] as T | undefined) ?? fallback
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export default function ArchiveExtractor({ api, config, onConfigChange }: PluginRenderProps) {
  const [queue, setQueue] = useState<string[]>([])
  const [current, setCurrent] = useState<{ source: string; taskId?: string; status: string; percent: number } | null>(null)
  const [lastError, setLastError] = useState<string | null>(null)
  const [lastResult, setLastResult] = useState<TaskSnapshot['result'] | null>(null)
  const [passwordRequest, setPasswordRequest] = useState<{ resolve: (value: string | null) => void } | null>(null)
  const [password, setPassword] = useState('')
  const [conflictRequest, setConflictRequest] = useState<{ names: string[]; resolve: (value: ConflictPolicy | null) => void } | null>(null)
  const [history, setHistory] = useState<Array<{ source: string; result: NonNullable<TaskSnapshot['result']> }>>([])
  const processing = useRef(false)
  const configRef = useRef(config)
  configRef.current = config

  const request = useCallback(async <T,>(type: string, payload: Record<string, unknown> = {}): Promise<T> => {
    const response = await api.sendToBackend({ type, ...payload }) as { ok?: boolean; code?: string; message?: string; data?: T }
    if (!response?.ok) throw new ServiceError(response?.code ?? 'service-error', response?.message ?? '解压服务返回错误')
    return response.data as T
  }, [api])

  const enqueue = useCallback((paths: string[]) => {
    const cleaned = paths.map((path) => path.trim()).filter(Boolean)
    if (cleaned.length === 0) return
    setLastError(null)
    setQueue((previous) => [...previous, ...cleaned])
  }, [])

  useEffect(() => api.onFilesDropped?.(enqueue), [api, enqueue])

  const choosePassword = useCallback(() => {
    setPassword('')
    return new Promise<string | null>((resolve) => setPasswordRequest({ resolve }))
  }, [])

  const chooseConflict = useCallback((names: string[]) => {
    return new Promise<ConflictPolicy | null>((resolve) => setConflictRequest({ names, resolve }))
  }, [])

  const selectArchives = useCallback(async () => {
    const paths = await api.dialog.open({
      type: 'file',
      multiple: true,
      extensions: ['zip', '7z', 'rar', 'tar', 'gz', 'bz2', 'xz', 'cab']
    })
    enqueue(paths)
  }, [api, enqueue])

  const updateConfig = useCallback((patch: PluginConfig) => onConfigChange({ ...configRef.current, ...patch }), [onConfigChange])

  const processArchive = useCallback(async (source: string) => {
    setCurrent({ source, status: '正在读取归档…', percent: 1 })
    setLastError(null)
    let info: ArchiveInfo
    let suppliedPassword: string | null = null
    try {
      try {
        info = await request<ArchiveInfo>('inspect', { source })
      } catch (error) {
        if (!(error instanceof ServiceError) || error.code !== 'password-required') throw error
        suppliedPassword = await choosePassword()
        if (!suppliedPassword) return
        info = await request<ArchiveInfo>('inspect', { source, password: suppliedPassword })
      }

      const destinationMode = configValue<DestinationMode>(configRef.current, 'destinationMode', 'same')
      let destination = info.suggestedDestination
      if (destinationMode === 'choose') {
        const selected = await api.dialog.open({ type: 'folder' })
        if (!selected[0]) return
        destination = selected[0]
      }

      const preflight = await request<Preflight>('preflight', {
        source,
        destination,
        password: suppliedPassword
      })
      if (preflight.disk.low) {
        const proceed = await api.confirm({
          title: '可用磁盘空间偏低',
          message: `预计需要至少 ${formatBytes(preflight.disk.requiredBytes)}，当前可用 ${formatBytes(preflight.disk.availableBytes)}。仍要继续吗？`,
          confirmLabel: '继续解压',
          cancelLabel: '取消'
        })
        if (!proceed) return
      }

      let conflictPolicy = configValue<ConflictPolicy>(configRef.current, 'conflictPolicy', 'ask')
      if (conflictPolicy === 'ask' && preflight.conflicts.length > 0) {
        const selected = await chooseConflict(preflight.conflicts)
        if (!selected) return
        conflictPolicy = selected
      }
      if (conflictPolicy === 'ask') conflictPolicy = 'overwrite'

      const { taskId } = await request<{ taskId: string }>('start', {
        source,
        destination,
        conflictPolicy,
        password: suppliedPassword,
        openAfterExtract: configValue(configRef.current, 'openAfterExtract', true)
      })
      let snapshot: TaskSnapshot | null = null
      do {
        await wait(220)
        snapshot = await request<TaskSnapshot>('getTask', { taskId })
        setCurrent({ source, taskId, status: snapshot.progress?.message ?? '正在解压…', percent: snapshot.progress?.percent ?? 2 })
      } while (snapshot.status === 'queued' || snapshot.status === 'running')

      if (snapshot.status !== 'succeeded' || !snapshot.result) {
        throw new Error(snapshot.error?.message ?? (snapshot.status === 'cancelled' ? '操作已取消' : '解压失败'))
      }
      setLastResult(snapshot.result)
      setHistory((previous) => [{ source, result: snapshot!.result! }, ...previous].slice(0, 8))
      api.notify('快速解压完成', `${snapshot.result.files} 个文件已解压到 ${snapshot.result.destination}`)
    } catch (error) {
      setLastError(error instanceof Error ? error.message : String(error))
    } finally {
      setCurrent(null)
    }
  }, [api, chooseConflict, choosePassword, request])

  useEffect(() => {
    if (processing.current || queue.length === 0) return
    processing.current = true
    const source = queue[0]
    void processArchive(source).finally(() => {
      setQueue((previous) => previous.slice(1))
      processing.current = false
    })
  }, [processArchive, queue])

  const cancelCurrent = useCallback(async () => {
    if (!current?.taskId) return
    try {
      await request('cancel', { taskId: current.taskId })
      setLastError('已请求停止当前解压任务。')
    } catch (error) {
      setLastError(error instanceof Error ? error.message : String(error))
    }
  }, [current, request])

  const destinationMode = configValue<DestinationMode>(config, 'destinationMode', 'same')
  const conflictPolicy = configValue<ConflictPolicy>(config, 'conflictPolicy', 'ask')

  return (
    <div style={styles.page}>
      <div style={styles.header}>
        <div>
          <h1 style={styles.title}>快速解压</h1>
          <p style={styles.subtitle}>内置 7-Zip，离线处理常见归档格式，多个文件按顺序执行。</p>
        </div>
        <button style={{ ...styles.button, ...styles.primary }} onClick={() => void selectArchives()}>选择归档</button>
      </div>

      <div style={{ display: 'grid', gap: 14 }}>
        <div style={styles.card}>
          <div style={styles.drop} onClick={() => void selectArchives()} role="button" tabIndex={0}>
            <p style={styles.dropTitle}>拖入 ZIP、7z、RAR 或其他归档文件</p>
            <p style={styles.dropText}>支持 ZIP · 7z · RAR · TAR · GZ · BZ2 · XZ · CAB；密码只在本次任务内使用。</p>
          </div>
          {queue.length > 0 && <p style={{ ...styles.muted, margin: '12px 0 0' }}>队列中还有 {queue.length} 个归档。</p>}
        </div>

        <div style={styles.card}>
          <div style={styles.row}>
            <span>默认解压位置</span>
            <select value={destinationMode} onChange={(event) => updateConfig({ destinationMode: event.target.value })}>
              <option value="same">归档旁边的同名文件夹</option>
              <option value="choose">每次选择目录</option>
            </select>
          </div>
          <div style={styles.row}>
            <span>文件冲突</span>
            <select value={conflictPolicy} onChange={(event) => updateConfig({ conflictPolicy: event.target.value })}>
              <option value="ask">询问</option>
              <option value="overwrite">覆盖</option>
              <option value="skip">跳过</option>
              <option value="rename">自动重命名</option>
            </select>
          </div>
          <div style={{ ...styles.row, borderBottom: 0 }}>
            <span>完成后打开目录</span>
            <input type="checkbox" checked={configValue(config, 'openAfterExtract', true)} onChange={(event) => updateConfig({ openAfterExtract: event.target.checked })} />
          </div>
        </div>

        {current && <div style={styles.card}>
          <strong>{current.source.split(/[\\/]/).pop()}</strong>
          <div style={styles.progress}><div style={{ ...styles.progressFill, width: `${current.percent}%` }} /></div>
          <div style={{ ...styles.row, borderBottom: 0 }}><span style={styles.muted}>{current.status}</span><span style={styles.muted}>{current.percent}%</span></div>
          <button style={{ ...styles.button, ...styles.danger }} onClick={() => void cancelCurrent()}>取消任务</button>
        </div>}

        {lastError && <div style={{ ...styles.card, color: '#b42318' }}>{lastError}</div>}
        {lastResult && <div style={styles.card}>
          <strong>最近完成</strong>
          <p style={styles.muted}>{lastResult.files} 个文件 · {formatBytes(lastResult.bytes)} · {lastResult.destination}</p>
          <button style={styles.button} onClick={() => void request('openFolder', { path: lastResult!.destination })}>打开目录</button>
        </div>}
        {history.length > 0 && <div style={styles.card}>
          <strong>本次会话记录</strong>
          {history.map((item) => <div style={styles.row} key={`${item.source}-${item.result.destination}`}><span style={styles.muted}>{item.source.split(/[\\/]/).pop()}</span><span style={styles.muted}>{item.result.files} 个文件</span></div>)}
        </div>}
      </div>

      {passwordRequest && <div style={styles.overlay}><div style={styles.modal}>
        <h3 style={{ marginTop: 0 }}>输入归档密码</h3>
        <p style={styles.muted}>密码不会保存到插件配置，也不会写入解压记录。</p>
        <input autoFocus type="password" value={password} onChange={(event) => setPassword(event.target.value)} style={styles.input} onKeyDown={(event) => { if (event.key === 'Enter') { passwordRequest.resolve(password); setPasswordRequest(null) } }} />
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 16 }}>
          <button style={styles.button} onClick={() => { passwordRequest.resolve(null); setPasswordRequest(null) }}>取消</button>
          <button style={{ ...styles.button, ...styles.primary }} onClick={() => { passwordRequest.resolve(password); setPasswordRequest(null) }}>继续</button>
        </div>
      </div></div>}

      {conflictRequest && <div style={styles.overlay}><div style={styles.modal}>
        <h3 style={{ marginTop: 0 }}>发现同名文件</h3>
        <p style={styles.muted}>这次队列任务统一使用一个处理方式：</p>
        <ul>{conflictRequest.names.slice(0, 20).map((name) => <li key={name}>{name}</li>)}</ul>
        {conflictRequest.names.length > 20 && <p style={styles.muted}>还有 {conflictRequest.names.length - 20} 项未展开。</p>}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, flexWrap: 'wrap' }}>
          <button style={styles.button} onClick={() => { conflictRequest.resolve(null); setConflictRequest(null) }}>取消</button>
          <button style={styles.button} onClick={() => { conflictRequest.resolve('skip'); setConflictRequest(null) }}>跳过</button>
          <button style={styles.button} onClick={() => { conflictRequest.resolve('rename'); setConflictRequest(null) }}>自动重命名</button>
          <button style={{ ...styles.button, ...styles.primary }} onClick={() => { conflictRequest.resolve('overwrite'); setConflictRequest(null) }}>覆盖</button>
        </div>
      </div></div>}
    </div>
  )
}

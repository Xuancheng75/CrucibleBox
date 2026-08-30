import React, { useState, useEffect, useCallback, useRef } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import {
  backendErrorMessage,
  cancelTask,
  clearCache,
  getStatus,
  getTask,
  installModel,
  installModelBundle,
  installRemoteModel,
  isDocumentProgressMessage,
  isOcrResult,
  isParsedDocumentResult,
  isTerminalTask,
  listModels,
  listModelCatalog,
  pauseTask,
  removeModel,
  retryTask,
  resumeTask,
  startBatch,
  startChunk,
  startConvert,
  startParse,
  startOcr,
  updateRemoteModel,
  type DocumentTaskSnapshot,
  type EngineStatus,
  type ModelCatalogEntry,
  type ModelListResponse,
  type OcrProgress,
  type OcrResult,
  type ParsedDocumentResult
} from './engine-api'

// ============================================================
// 内联样式常量 — 复刻 Ant Design 视觉风格（与 UniEnv 对齐）
// ============================================================
const COLORS = {
  primary: 'var(--ob-color-primary, #1677ff)',
  primaryHover: 'var(--ob-color-primary-hover, #4096ff)',
  primaryLight: 'var(--ob-color-primary-bg, #e6f4ff)',
  success: 'var(--ob-color-success, #52c41a)',
  successBg: 'var(--ob-color-success-bg, #f6ffed)',
  successBorder: 'var(--ob-color-success-border, #b7eb8f)',
  warning: 'var(--ob-color-warning, #faad14)',
  warningBg: 'var(--ob-color-warning-bg, #fffbe6)',
  warningBorder: 'var(--ob-color-warning-border, #ffe58f)',
  danger: 'var(--ob-color-error, #ff4d4f)',
  dangerBg: 'var(--ob-color-error-bg, #fff2f0)',
  text: 'var(--ob-color-text, #1f1f1f)',
  textSecondary: 'var(--ob-color-text-secondary, #8c8c8c)',
  textTertiary: 'var(--ob-color-text-tertiary, #bfbfbf)',
  border: 'var(--ob-color-border, #f0f0f0)',
  borderLight: 'var(--ob-color-border-secondary, #f5f5f5)',
  bgWhite: 'var(--ob-color-bg-container, #ffffff)',
  bgGray: 'var(--ob-color-bg, #fafafa)',
  shadow: '0 2px 8px rgba(0,0,0,0.06)'
}

const FONT = {
  family:
    '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
  sizeXs: '11px',
  sizeSm: '12px',
  sizeMd: '13px',
  sizeLg: '14px',
  sizeXl: '16px',
  sizeTitle: '20px'
}

const prettyJson = (value: unknown): string => JSON.stringify(value, null, 2) ?? ''

/** Keep the actual path in state while showing a readable value in narrow inputs. */
const displayPath = (path: string): string => {
  if (path.length <= 72) return path
  return `…${path.slice(-69)}`
}

const NAV_ITEMS = [
  { key: 'overview', label: '概览', icon: '📄' },
  { key: 'ocr', label: 'OCR', icon: '🔍' },
  { key: 'parse', label: 'PDF 解析', icon: '📑' },
  { key: 'convert', label: '转换', icon: '🔄' },
  { key: 'chunk', label: '切分', icon: '✂️' },
  { key: 'batch', label: '批量处理', icon: '📚' },
  { key: 'jobs', label: '任务', icon: '📋' },
  { key: 'history', label: '历史', icon: '🕘' },
  { key: 'models', label: '模型', icon: '🧠' }
]

interface RecentTask {
  id: string
  name: string
  action: string
  status: string
}

interface ImportedDocument {
  path: string
  name: string
  kind: 'pdf' | 'image' | 'document'
  source: 'overview' | 'picker' | 'drop'
}

const STATUS_LABELS: Record<string, string> = {
  queued: '等待中',
  running: '处理中',
  paused: '已暂停',
  succeeded: '完成',
  completed: '完成',
  failed: '失败',
  cancelled: '已取消'
}

const STATUS_COLORS: Record<string, string> = {
  queued: COLORS.textSecondary,
  running: COLORS.primary,
  paused: COLORS.warning,
  completed: COLORS.success,
  succeeded: COLORS.success,
  failed: COLORS.danger,
  cancelled: COLORS.warning
}

// ============================================================
// 组件
// ============================================================

function StatusBadge({ state, label }: { state: 'ok' | 'neutral' | 'error'; label: string }) {
  const ok = state === 'ok'
  const neutral = state === 'neutral'
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 4,
        padding: '2px 8px',
        borderRadius: 4,
        fontSize: FONT.sizeSm,
        background: ok ? COLORS.successBg : neutral ? COLORS.bgGray : COLORS.dangerBg,
        color: ok ? COLORS.success : neutral ? COLORS.textSecondary : COLORS.danger,
        border: `1px solid ${ok ? COLORS.successBorder : neutral ? COLORS.border : 'var(--ob-color-error-border, #ffccc7)'}`
      }}
    >
      {ok ? '✓' : neutral ? '•' : '✕'} {label}
    </span>
  )
}

function DropZone({ onPick }: { onPick: () => void }) {
  return (
    <div
      style={{
        border: `2px dashed ${COLORS.border}`,
        borderRadius: 12,
        padding: '48px 24px',
        textAlign: 'center',
        background: COLORS.bgGray,
        cursor: 'pointer',
        transition: 'all 0.2s'
      }}
      onClick={onPick}
    >
      <div style={{ fontSize: 40, marginBottom: 12 }}>📥</div>
      <div style={{ fontSize: FONT.sizeXl, fontWeight: 600, color: COLORS.text }}>
        拖入文件 / 文件夹
      </div>
      <div style={{ fontSize: FONT.sizeSm, color: COLORS.textSecondary, marginTop: 6 }}>
        支持图片 / PDF / DOCX / Markdown / HTML / TXT
      </div>
    </div>
  )
}

function QuickAction({
  icon,
  label,
  onClick
}: {
  icon: string
  label: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 8,
        padding: '20px 12px',
        background: COLORS.bgWhite,
        border: `1px solid ${COLORS.border}`,
        borderRadius: 10,
        cursor: 'pointer',
        fontSize: FONT.sizeLg,
        color: COLORS.text,
        fontWeight: 500,
        minWidth: 120,
        boxShadow: COLORS.shadow
      }}
    >
      <span style={{ fontSize: 28 }}>{icon}</span>
      {label}
    </button>
  )
}

// ============================================================
// 插件渲染入口
// ============================================================

export default function DocumentEngineUI({ api }: PluginRenderProps) {
  const [activeKey, setActiveKey] = useState('overview')
  const [activeDocument, setActiveDocument] = useState<ImportedDocument | null>(null)
  const [status, setStatus] = useState<EngineStatus | null>(null)
  const [recentTasks, setRecentTasks] = useState<RecentTask[]>([])
  const [loadingStatus, setLoadingStatus] = useState(false)
  const [statusError, setStatusError] = useState<string | null>(null)
  const [ocrPath, setOcrPath] = useState('')
  const [ocrTaskId, setOcrTaskId] = useState<string | null>(null)
  const [ocrTask, setOcrTask] = useState<DocumentTaskSnapshot | null>(null)
  const [ocrProgress, setOcrProgress] = useState<OcrProgress | null>(null)
  const [ocrResult, setOcrResult] = useState<OcrResult | null>(null)
  const [ocrError, setOcrError] = useState<string | null>(null)
  const [ocrBusy, setOcrBusy] = useState(false)
  const [parsePath, setParsePath] = useState('')
  const [parseTaskId, setParseTaskId] = useState<string | null>(null)
  const [parseTask, setParseTask] = useState<DocumentTaskSnapshot | null>(null)
  const [parseProgress, setParseProgress] = useState<OcrProgress | null>(null)
  const [parseResult, setParseResult] = useState<ParsedDocumentResult | null>(null)
  const [parseError, setParseError] = useState<string | null>(null)
  const [parseBusy, setParseBusy] = useState(false)
  const [chunkPath, setChunkPath] = useState('')
  const [chunkTaskId, setChunkTaskId] = useState<string | null>(null)
  const [chunkTask, setChunkTask] = useState<DocumentTaskSnapshot | null>(null)
  const [chunkProgress, setChunkProgress] = useState<OcrProgress | null>(null)
  const [chunkResult, setChunkResult] = useState<unknown>(null)
  const [chunkError, setChunkError] = useState<string | null>(null)
  const [chunkBusy, setChunkBusy] = useState(false)
  const [chunkStrategy, setChunkStrategy] = useState<'hybrid' | 'pages' | 'chapters' | 'structure' | 'semantic'>('hybrid')
  const [convertPath, setConvertPath] = useState('')
  const [convertTarget, setConvertTarget] = useState('md')
  const [convertOutput, setConvertOutput] = useState('')
  const [convertTaskId, setConvertTaskId] = useState<string | null>(null)
  const [convertTask, setConvertTask] = useState<DocumentTaskSnapshot | null>(null)
  const [convertProgress, setConvertProgress] = useState<OcrProgress | null>(null)
  const [convertResult, setConvertResult] = useState<unknown>(null)
  const [convertError, setConvertError] = useState<string | null>(null)
  const [convertBusy, setConvertBusy] = useState(false)
  const [batchPaths, setBatchPaths] = useState('')
  const [batchOperation, setBatchOperation] = useState<'ocr' | 'parse' | 'convert'>('parse')
  const [batchTarget, setBatchTarget] = useState('md')
  const [batchTaskId, setBatchTaskId] = useState<string | null>(null)
  const [batchTask, setBatchTask] = useState<DocumentTaskSnapshot | null>(null)
  const [batchProgress, setBatchProgress] = useState<OcrProgress | null>(null)
  const [batchResult, setBatchResult] = useState<unknown>(null)
  const [batchError, setBatchError] = useState<string | null>(null)
  const [batchBusy, setBatchBusy] = useState(false)
  const [allTasks, setAllTasks] = useState<DocumentTaskSnapshot[]>([])
  const [jobsError, setJobsError] = useState<string | null>(null)
  const [models, setModels] = useState<Array<Record<string, unknown>>>([])
  const [modelCatalog, setModelCatalog] = useState<ModelCatalogEntry[]>([])
  const [modelsError, setModelsError] = useState<string | null>(null)
  const [modelBundles, setModelBundles] = useState<NonNullable<ModelListResponse['bundles']>>([])
  const [modelCatalogError, setModelCatalogError] = useState<string | null>(null)
  const [modelSource, setModelSource] = useState('')
  const [modelName, setModelName] = useState('')
  const [modelUrl, setModelUrl] = useState('')
  const [modelSha256, setModelSha256] = useState('')
  const [modelsBusy, setModelsBusy] = useState(false)
  const mounted = useRef(false)

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  const send = useCallback(async (msg: unknown): Promise<unknown> => api.sendToBackend(msg), [api])

  const selectPaths = useCallback(
    async (options: {
      type: 'file' | 'folder'
      multiple?: boolean
      extensions?: string[]
    }): Promise<string[]> => {
      try {
        return await api.dialog.open(options)
      } catch (error) {
        api.notify('文件选择失败', error instanceof Error ? error.message : String(error))
        return []
      }
    },
    [api]
  )

  const setSharedDocument = useCallback(
    (path: string, kind: ImportedDocument['kind'], source: ImportedDocument['source']) => {
      const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path
      setActiveDocument({ path, name, kind, source })
      if (kind === 'pdf') {
        setParsePath(path)
        setChunkPath(path)
        setConvertPath(path)
      } else if (kind === 'image') {
        setParsePath('')
        setChunkPath('')
        setConvertPath('')
        setOcrPath(path)
      } else {
        setParsePath('')
        setChunkPath('')
        setConvertPath('')
      }
    },
    []
  )

  const clearSharedDocument = useCallback(() => {
    setActiveDocument(null)
    setParsePath('')
    setChunkPath('')
    setConvertPath('')
    setOcrPath('')
  }, [])

  const selectFolderFiles = useCallback(
    async (setPaths: (value: string) => void) => {
      const [folder] = await selectPaths({ type: 'folder' })
      if (!folder) return
      try {
        const response = (await send({ type: 'document.files.enumerate', path: folder })) as {
          paths?: unknown
          error?: string
        }
        if (!Array.isArray(response.paths)) {
          throw new Error(response.error ?? '文件夹中没有支持的文档')
        }
        setPaths(
          response.paths.filter((path): path is string => typeof path === 'string').join('\n')
        )
      } catch (error) {
        api.notify('文件夹导入失败', error instanceof Error ? error.message : String(error))
      }
    },
    [api, selectPaths, send]
  )

  const refreshStatus = useCallback(async () => {
    setLoadingStatus(true)
    try {
      const nextStatus = await getStatus(send)
      if (mounted.current) {
        setStatus(nextStatus)
        setStatusError(null)
      }
    } catch (error) {
      if (mounted.current) {
        setStatus(null)
        setStatusError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mounted.current) setLoadingStatus(false)
    }
  }, [send])

  const refreshJobs = useCallback(async () => {
    try {
      const response = await send({ type: 'document.jobs.list' })
      if (!response || typeof response !== 'object')
        throw new Error(backendErrorMessage(response, '任务列表读取失败：后端未返回有效响应'))
      const candidate = response as { tasks?: unknown }
      if (!Array.isArray(candidate.tasks))
        throw new Error(backendErrorMessage(response, '任务列表读取失败'))
      if (mounted.current) setAllTasks(candidate.tasks as DocumentTaskSnapshot[])
      setJobsError(null)
    } catch (error) {
      if (mounted.current) setJobsError(error instanceof Error ? error.message : String(error))
    }
  }, [send])

  const refreshModels = useCallback(async () => {
    try {
      const response = await listModels(send)
      if (mounted.current) setModels(response.models)
      if (mounted.current) setModelBundles(response.bundles ?? [])
      setModelsError(null)
    } catch (error) {
      if (mounted.current) setModelsError(error instanceof Error ? error.message : String(error))
    }
  }, [send])

  const refreshModelCatalog = useCallback(async () => {
    try {
      const catalog = await listModelCatalog(send)
      if (mounted.current) setModelCatalog(catalog)
      setModelCatalogError(null)
    } catch (error) {
      if (mounted.current)
        setModelCatalogError(error instanceof Error ? error.message : String(error))
    }
  }, [send])

  // Worker 进度通过宿主既有 plugin:message → api.onBackendMessage 桥接到 iframe。
  useEffect(() => {
    return api.onBackendMessage((message) => {
      if (!isDocumentProgressMessage(message)) return
      if (ocrTaskId && message.taskId === ocrTaskId) setOcrProgress(message.progress)
      if (parseTaskId && message.taskId === parseTaskId) setParseProgress(message.progress)
      if (chunkTaskId && message.taskId === chunkTaskId) setChunkProgress(message.progress)
      if (convertTaskId && message.taskId === convertTaskId) setConvertProgress(message.progress)
      if (batchTaskId && message.taskId === batchTaskId) setBatchProgress(message.progress)
    })
  }, [api, ocrTaskId, parseTaskId, chunkTaskId, convertTaskId, batchTaskId])

  // 任务接口是异步的；事件用于即时进度，轮询用于最终结果和崩溃/取消状态。
  useEffect(() => {
    if (!ocrTaskId) return
    let active = true
    const poll = async () => {
      try {
        const snapshot = await getTask(send, ocrTaskId)
        if (!active) return
        setOcrTask(snapshot)
        if (snapshot.progress) setOcrProgress(snapshot.progress)
        if (snapshot.status === 'succeeded' && isOcrResult(snapshot.result))
          setOcrResult(snapshot.result)
        if (snapshot.status === 'failed' || snapshot.status === 'cancelled') {
          setOcrError(
            snapshot.error?.message ??
              (snapshot.status === 'cancelled' ? '任务已取消' : 'OCR 任务失败')
          )
        }
        setRecentTasks((previous) =>
          previous.map((item) =>
            item.id === snapshot.taskId ? { ...item, status: snapshot.status } : item
          )
        )
      } catch (error) {
        if (active) setOcrError(error instanceof Error ? error.message : String(error))
      }
    }
    void poll()
    const timer = window.setInterval(() => {
      if (!ocrTask || !isTerminalTask(ocrTask)) void poll()
    }, 500)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [ocrTask, ocrTaskId, send])

  // PDF 解析同样通过统一任务接口收敛最终结果；进度事件只负责降低轮询延迟。
  useEffect(() => {
    if (!parseTaskId) return
    let active = true
    const poll = async () => {
      try {
        const snapshot = await getTask(send, parseTaskId)
        if (!active) return
        setParseTask(snapshot)
        if (snapshot.progress) setParseProgress(snapshot.progress)
        if (snapshot.status === 'succeeded' && isParsedDocumentResult(snapshot.result)) {
          setParseResult(snapshot.result)
        }
        if (snapshot.status === 'failed' || snapshot.status === 'cancelled') {
          setParseError(
            snapshot.error?.message ??
              (snapshot.status === 'cancelled' ? '任务已取消' : 'PDF 解析任务失败')
          )
        }
        setRecentTasks((previous) =>
          previous.map((item) =>
            item.id === snapshot.taskId ? { ...item, status: snapshot.status } : item
          )
        )
      } catch (error) {
        if (active) setParseError(error instanceof Error ? error.message : String(error))
      }
    }
    void poll()
    const timer = window.setInterval(() => {
      if (!parseTask || !isTerminalTask(parseTask)) void poll()
    }, 500)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [parseTask, parseTaskId, send])

  const watchUtilityTask = useCallback(
    async (
      taskId: string,
      setTask: (task: DocumentTaskSnapshot) => void,
      setProgress: (progress: OcrProgress) => void,
      setResult: (result: unknown) => void,
      setError: (error: string) => void
    ) => {
      try {
        const snapshot = await getTask(send, taskId)
        setTask(snapshot)
        if (snapshot.progress) setProgress(snapshot.progress)
        if (snapshot.status === 'succeeded') setResult(snapshot.result)
        if (snapshot.status === 'failed' || snapshot.status === 'cancelled') {
          setError(
            snapshot.error?.message ?? (snapshot.status === 'cancelled' ? '任务已取消' : '任务失败')
          )
        }
        setRecentTasks((previous) =>
          previous.map((item) =>
            item.id === snapshot.taskId ? { ...item, status: snapshot.status } : item
          )
        )
        return snapshot
      } catch (error) {
        setError(error instanceof Error ? error.message : String(error))
        return null
      }
    },
    [send]
  )

  useEffect(() => {
    if (!chunkTaskId) return
    let active = true
    const poll = async () => {
      const snapshot = await watchUtilityTask(
        chunkTaskId,
        setChunkTask,
        setChunkProgress,
        setChunkResult,
        (error) => setChunkError(error)
      )
      if (!active || !snapshot || snapshot.status !== 'succeeded') return
    }
    void poll()
    const timer = window.setInterval(() => {
      if (!chunkTask || !isTerminalTask(chunkTask)) void poll()
    }, 500)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [chunkTask, chunkTaskId, watchUtilityTask])

  useEffect(() => {
    if (!convertTaskId) return
    let active = true
    const poll = async () => {
      const snapshot = await watchUtilityTask(
        convertTaskId,
        setConvertTask,
        setConvertProgress,
        setConvertResult,
        setConvertError
      )
      if (!active || !snapshot) return
    }
    void poll()
    const timer = window.setInterval(() => {
      if (!convertTask || !isTerminalTask(convertTask)) void poll()
    }, 500)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [convertTask, convertTaskId, watchUtilityTask])

  useEffect(() => {
    if (!batchTaskId) return
    let active = true
    const poll = async () => {
      const snapshot = await watchUtilityTask(
        batchTaskId,
        setBatchTask,
        setBatchProgress,
        setBatchResult,
        setBatchError
      )
      if (!active || !snapshot) return
    }
    void poll()
    const timer = window.setInterval(() => {
      if (!batchTask || !isTerminalTask(batchTask)) void poll()
    }, 500)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [batchTask, batchTaskId, watchUtilityTask])

  const runOcr = useCallback(async () => {
    const path = ocrPath.trim()
    if (!path) {
      api.notify('请选择文件', '请点击“选择图片”，或从概览拖入图片')
      return
    }
    setOcrBusy(true)
    setOcrError(null)
    setOcrResult(null)
    setOcrProgress(null)
    setOcrTask(null)
    try {
      const accepted = await startOcr(send, path)
      setOcrTaskId(accepted.taskId)
      setRecentTasks((previous) =>
        [
          {
            id: accepted.taskId,
            name: path.split(/[\\/]/).pop() || path,
            action: 'OCR',
            status: accepted.status
          },
          ...previous
        ].slice(0, 10)
      )
    } catch (error) {
      setOcrError(error instanceof Error ? error.message : String(error))
    } finally {
      setOcrBusy(false)
    }
  }, [api, ocrPath, send])

  const stopOcr = useCallback(async () => {
    if (!ocrTaskId) return
    try {
      await cancelTask(send, ocrTaskId)
    } catch (error) {
      setOcrError(error instanceof Error ? error.message : String(error))
    }
  }, [ocrTaskId, send])

  const runParse = useCallback(async () => {
    const path = parsePath.trim()
    if (!path) {
      api.notify('请选择 PDF', '请点击“选择 PDF”，或从概览拖入 PDF')
      return
    }
    setParseBusy(true)
    setParseError(null)
    setParseResult(null)
    setParseProgress(null)
    setParseTask(null)
    try {
      const accepted = await startParse(send, path)
      setParseTaskId(accepted.taskId)
      setRecentTasks((previous) =>
        [
          {
            id: accepted.taskId,
            name: path.split(/[\\/]/).pop() || path,
            action: 'PDF 解析',
            status: accepted.status
          },
          ...previous
        ].slice(0, 10)
      )
    } catch (error) {
      setParseError(error instanceof Error ? error.message : String(error))
    } finally {
      setParseBusy(false)
    }
  }, [api, parsePath, send])

  const stopParse = useCallback(async () => {
    if (!parseTaskId) return
    try {
      await cancelTask(send, parseTaskId)
    } catch (error) {
      setParseError(error instanceof Error ? error.message : String(error))
    }
  }, [parseTaskId, send])

  const runChunk = useCallback(async () => {
    const path = chunkPath.trim()
    if (!path) {
      api.notify('请选择文档', '请点击“选择文档”，或从概览拖入文档')
      return
    }
    setChunkBusy(true)
    setChunkError(null)
    setChunkResult(null)
    setChunkProgress(null)
    setChunkTask(null)
    try {
      const accepted = await startChunk(send, path, { strategy: chunkStrategy })
      setChunkTaskId(accepted.taskId)
      setRecentTasks((previous) =>
        [
          {
            id: accepted.taskId,
            name: path.split(/[\\/]/).pop() || path,
            action: '切分',
            status: accepted.status
          },
          ...previous
        ].slice(0, 10)
      )
    } catch (error) {
      setChunkError(error instanceof Error ? error.message : String(error))
    } finally {
      setChunkBusy(false)
    }
  }, [api, chunkPath, chunkStrategy, send])

  const runConvert = useCallback(async () => {
    const path = convertPath.trim()
    if (!path) {
      api.notify('请选择文档', '请点击“选择源文档”，或从概览拖入文档')
      return
    }
    setConvertBusy(true)
    setConvertError(null)
    setConvertResult(null)
    setConvertProgress(null)
    setConvertTask(null)
    try {
      const accepted = await startConvert(
        send,
        path,
        convertTarget,
        convertOutput.trim() || undefined
      )
      setConvertTaskId(accepted.taskId)
      setRecentTasks((previous) =>
        [
          {
            id: accepted.taskId,
            name: path.split(/[\\/]/).pop() || path,
            action: '转换',
            status: accepted.status
          },
          ...previous
        ].slice(0, 10)
      )
    } catch (error) {
      setConvertError(error instanceof Error ? error.message : String(error))
    } finally {
      setConvertBusy(false)
    }
  }, [api, convertOutput, convertPath, convertTarget, send])

  const runBatch = useCallback(async () => {
    const paths = batchPaths
      .split(/\r?\n/)
      .map((path) => path.trim())
      .filter(Boolean)
    if (paths.length === 0) {
      api.notify('请输入文件列表', '请点击“选择多个文件”或“选择文件夹并导入”')
      return
    }
    setBatchBusy(true)
    setBatchError(null)
    setBatchResult(null)
    setBatchProgress(null)
    setBatchTask(null)
    try {
      const accepted = await startBatch(send, paths, batchOperation, batchTarget)
      setBatchTaskId(accepted.taskId)
      setRecentTasks((previous) =>
        [
          {
            id: accepted.taskId,
            name: `${paths.length} 个文件`,
            action: '批量处理',
            status: accepted.status
          },
          ...previous
        ].slice(0, 10)
      )
    } catch (error) {
      setBatchError(error instanceof Error ? error.message : String(error))
    } finally {
      setBatchBusy(false)
    }
  }, [api, batchOperation, batchPaths, batchTarget, send])

  const stopUtility = useCallback(
    async (taskId: string | null, setError: (error: string) => void) => {
      if (!taskId) return
      try {
        await cancelTask(send, taskId)
      } catch (error) {
        setError(error instanceof Error ? error.message : String(error))
      }
    },
    [send]
  )

  const installLocalModel = useCallback(async () => {
    const source = modelSource.trim()
    if (!source) {
      api.notify('请输入模型源路径')
      return
    }
    setModelsBusy(true)
    try {
      await installModel(send, source, modelName.trim() || undefined)
      setModelSource('')
      setModelName('')
      await refreshModels()
    } catch (error) {
      setModelsError(error instanceof Error ? error.message : String(error))
    } finally {
      setModelsBusy(false)
    }
  }, [api, modelName, modelSource, refreshModels, send])

  const installCatalogModel = useCallback(
    async (modelId: string) => {
      setModelsBusy(true)
      setModelsError(null)
      try {
        await installModelBundle(send, modelId)
        await refreshModels()
        api.notify('模型安装完成', 'OCR 模型已通过 SHA-256 校验并启用')
      } catch (error) {
        setModelsError(error instanceof Error ? error.message : String(error))
      } finally {
        setModelsBusy(false)
      }
    },
    [api, refreshModels, send]
  )

  const installRemote = useCallback(async () => {
    const url = modelUrl.trim()
    const sha256 = modelSha256.trim()
    if (!url || !sha256) {
      api.notify('请输入 HTTPS 模型地址和 SHA-256')
      return
    }
    setModelsBusy(true)
    try {
      await installRemoteModel(send, url, sha256, modelName.trim() || undefined)
      setModelUrl('')
      setModelSha256('')
      setModelName('')
      await refreshModels()
    } catch (error) {
      setModelsError(error instanceof Error ? error.message : String(error))
    } finally {
      setModelsBusy(false)
    }
  }, [api, modelName, modelSha256, modelUrl, refreshModels, send])

  const updateRemote = useCallback(
    async (name: string) => {
      const url = modelUrl.trim()
      const sha256 = modelSha256.trim()
      if (!url || !sha256) {
        api.notify('请输入更新地址和 SHA-256')
        return
      }
      setModelsBusy(true)
      try {
        await updateRemoteModel(send, url, sha256, name)
        await refreshModels()
      } catch (error) {
        setModelsError(error instanceof Error ? error.message : String(error))
      } finally {
        setModelsBusy(false)
      }
    },
    [api, modelSha256, modelUrl, refreshModels, send]
  )

  const deleteModel = useCallback(
    async (path: string) => {
      const confirmed = await api.confirm({ title: '删除模型', message: `确定删除 ${path} 吗？` })
      if (!confirmed) return
      setModelsBusy(true)
      try {
        await removeModel(send, path)
        await refreshModels()
      } catch (error) {
        setModelsError(error instanceof Error ? error.message : String(error))
      } finally {
        setModelsBusy(false)
      }
    },
    [api, refreshModels, send]
  )

  const clearEngineCache = useCallback(async () => {
    setModelsBusy(true)
    try {
      const response = (await clearCache(send)) as { removed?: number }
      api.notify('缓存已清理', `移除 ${response.removed ?? 0} 个条目`)
      await refreshStatus()
    } catch (error) {
      setModelsError(error instanceof Error ? error.message : String(error))
    } finally {
      setModelsBusy(false)
    }
  }, [api, refreshStatus, send])

  useEffect(() => {
    void refreshStatus()
  }, [refreshStatus])

  useEffect(() => {
    if (activeKey !== 'jobs' && activeKey !== 'history') return
    void refreshJobs()
    const timer = window.setInterval(() => void refreshJobs(), 1000)
    return () => window.clearInterval(timer)
  }, [activeKey, refreshJobs])

  useEffect(() => {
    if (activeKey === 'models') {
      void refreshModels()
      void refreshModelCatalog()
    }
  }, [activeKey, refreshModelCatalog, refreshModels])

  const handleFiles = useCallback(
    async (files: string[], source: ImportedDocument['source'] = 'drop') => {
      if (files.length === 0) {
        api.notify('请选择文件', '请从概览拖入文件，或点击对应的选择按钮')
        return
      }
      if (!mounted.current) return
      if (files.length > 1) {
        clearSharedDocument()
        setBatchPaths(files.join('\n'))
        setActiveKey('batch')
        return
      }

      const path = files[0]
      try {
        const response = (await send({ type: 'document.files.enumerate', path })) as {
          paths?: unknown
          error?: string
        }
        if (!mounted.current) return
        const enumerated = Array.isArray(response.paths)
          ? response.paths.filter((value): value is string => typeof value === 'string')
          : []
        if (enumerated.length === 0) {
          throw new Error(response.error ?? '未找到可导入的文档')
        }
        // 文件夹会展开为内部文档；即使只有一个文件，也不能把文件夹本身当作 OCR 输入。
        if (enumerated.length !== 1 || enumerated[0] !== path) {
          clearSharedDocument()
          setBatchPaths(enumerated.join('\n'))
          setActiveKey('batch')
          return
        }
      } catch (error) {
        api.notify('文件导入失败', error instanceof Error ? error.message : String(error))
        return
      }

      if (/\.pdf$/i.test(path)) {
        setSharedDocument(path, 'pdf', source)
        setActiveKey('parse')
      } else if (/\.(png|jpe?g|webp|bmp|tiff?)$/i.test(path)) {
        setSharedDocument(path, 'image', source)
        setActiveKey('ocr')
      } else {
        setSharedDocument(path, 'document', source)
        setBatchPaths(path)
        setActiveKey('batch')
      }
    },
    [api, clearSharedDocument, send, setSharedDocument]
  )

  // 兼容尚未升级 frame runtime 的宿主：旧 runtime 没有该可选能力时，
  // 仍允许通过文件选择器使用插件，而不是在 effect 阶段把整个 iframe 弄空。
  useEffect(() => api.onFilesDropped?.(handleFiles), [api, handleFiles])

  const cardStyle: React.CSSProperties = {
    background: COLORS.bgWhite,
    border: `1px solid ${COLORS.border}`,
    borderRadius: 8,
    padding: 16,
    marginBottom: 12
  }

  const renderCurrentDocument = () => {
    if (!activeDocument) return null
    return (
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 10,
          marginBottom: 12,
          padding: '8px 10px',
          border: `1px solid ${COLORS.primary}`,
          borderRadius: 6,
          background: COLORS.primaryLight,
          fontSize: FONT.sizeSm
        }}
      >
        <span title={activeDocument.path} style={{ minWidth: 0 }}>
          当前文档：{activeDocument.name} · {displayPath(activeDocument.path)}
        </span>
        <button
          type="button"
          onClick={clearSharedDocument}
          style={{
            flexShrink: 0,
            border: `1px solid ${COLORS.border}`,
            borderRadius: 4,
            background: COLORS.bgWhite,
            padding: '3px 7px'
          }}
        >
          清除
        </button>
      </div>
    )
  }

  // ============================================================
  // 各导航面板
  // ============================================================

  const renderOverview = () => (
    <div>
      <div style={{ marginBottom: 20 }}>
        <h2 style={{ margin: '0 0 4px', fontSize: FONT.sizeTitle, fontWeight: 600 }}>
          📄 Document Engine
        </h2>
        <p style={{ margin: 0, color: COLORS.textSecondary, fontSize: FONT.sizeLg }}>
          统一本地文档处理基础设施
        </p>
      </div>

      <DropZone
        onPick={() => {
          void selectPaths({ type: 'file', multiple: true }).then((paths) => {
            if (paths.length > 1) {
              clearSharedDocument()
              setBatchPaths(paths.join('\n'))
              setActiveKey('batch')
            } else if (paths[0]) {
              handleFiles(paths, 'overview')
            }
          })
        }}
      />

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(120px, 1fr))',
          gap: 12,
          marginTop: 16
        }}
      >
        <QuickAction icon="🔍" label="OCR" onClick={() => setActiveKey('ocr')} />
        <QuickAction icon="📑" label="PDF 解析" onClick={() => setActiveKey('parse')} />
        <QuickAction icon="🔄" label="转换" onClick={() => setActiveKey('convert')} />
        <QuickAction icon="📚" label="批量处理" onClick={() => setActiveKey('batch')} />
      </div>

      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 14 }}>
        <button
          type="button"
          onClick={async () => {
            const paths = await selectPaths({ type: 'file', multiple: true })
            if (paths.length > 1) {
              clearSharedDocument()
              setBatchPaths(paths.join('\n'))
              setActiveKey('batch')
            } else if (paths[0]) {
              handleFiles(paths, 'overview')
            }
          }}
          style={{
            padding: '8px 12px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6,
            background: COLORS.bgWhite
          }}
        >
          选择文件
        </button>
        <button
          type="button"
          onClick={() =>
            void selectFolderFiles((value) => {
              clearSharedDocument()
              setBatchPaths(value)
              setActiveKey('batch')
            })
          }
          style={{
            padding: '8px 12px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6,
            background: COLORS.bgWhite
          }}
        >
          选择文件夹并导入
        </button>
      </div>

      <div style={{ ...cardStyle, marginTop: 16 }}>
        <div style={{ fontWeight: 600, fontSize: FONT.sizeLg, marginBottom: 12 }}>
          引擎状态
          <button
            type="button"
            onClick={() => void refreshStatus()}
            style={{
              marginLeft: 12,
              fontSize: FONT.sizeSm,
              border: `1px solid ${COLORS.border}`,
              borderRadius: 4,
              background: COLORS.bgWhite,
              cursor: 'pointer',
              padding: '2px 8px'
            }}
          >
            {loadingStatus ? '刷新中…' : '刷新'}
          </button>
        </div>
        {status ? (
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
            {(() => {
              const worker = status.ocrWorker
              const pdfium = status.pdfium
              const defaultModel = status.models?.default
              const gpu = status.gpu
              const gpuRequired = status.config?.device === 'gpu'
              return (
                <>
                  <StatusBadge
                    state={worker ? (worker.available ? 'ok' : 'error') : 'neutral'}
                    label={
                      worker
                        ? `OCR Worker ${worker.available ? '可用' : '不可用'}`
                        : 'OCR Worker 状态未知'
                    }
                  />
                  <StatusBadge
                    state={worker?.running ? 'ok' : 'neutral'}
                    label={
                      worker
                        ? worker.running
                          ? 'Worker 运行中'
                          : 'Worker 待命'
                        : 'Worker 状态未知'
                    }
                  />
                  <StatusBadge
                    state={
                      pdfium
                        ? pdfium.error
                          ? 'error'
                          : pdfium.initialized || pdfium.available
                            ? 'ok'
                            : 'error'
                        : 'neutral'
                    }
                    label={
                      pdfium
                        ? pdfium.error
                          ? 'PDFium 绑定失败'
                          : `PDFium ${pdfium.initialized ? '已绑定' : pdfium.available ? '可用' : '缺失'}`
                        : 'PDFium 状态未知'
                    }
                  />
                  <StatusBadge
                    state={defaultModel?.ready ? 'ok' : defaultModel?.error ? 'error' : 'neutral'}
                    label={
                      defaultModel?.ready
                        ? '默认模型已就绪'
                        : defaultModel?.error
                          ? '默认模型准备失败'
                          : '默认模型未安装'
                    }
                  />
                  <StatusBadge
                    state={gpu?.available ? 'ok' : gpuRequired ? 'error' : 'neutral'}
                    label={
                      gpu?.available
                        ? `GPU ${gpu.device ?? '可用'}`
                        : gpuRequired
                          ? 'GPU 不可用'
                          : 'GPU 未检测到（CPU 回退）'
                    }
                  />
                  {pdfium?.error && (
                    <div
                      style={{
                        flexBasis: '100%',
                        color: COLORS.danger,
                        fontSize: FONT.sizeSm,
                        marginTop: 2
                      }}
                    >
                      PDFium 诊断：{pdfium.error}
                    </div>
                  )}
                </>
              )
            })()}
          </div>
        ) : (
          <div style={{ color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
            {loadingStatus
              ? '检测中…'
              : statusError
                ? `状态读取失败：${statusError}`
                : '未获取到 Worker 状态'}
          </div>
        )}
        {statusError && (
          <div
            style={{
              marginTop: 10,
              padding: 10,
              background: COLORS.dangerBg,
              color: COLORS.danger,
              borderRadius: 6,
              fontSize: FONT.sizeSm
            }}
          >
            后端诊断失败：{statusError}
          </div>
        )}
      </div>

      <div style={{ ...cardStyle }}>
        <div style={{ fontWeight: 600, fontSize: FONT.sizeLg, marginBottom: 12 }}>最近任务</div>
        {recentTasks.length === 0 ? (
          <div style={{ color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>暂无任务</div>
        ) : (
          recentTasks.map((task) => (
            <div
              key={task.id}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '8px 0',
                borderBottom: `1px solid ${COLORS.borderLight}`
              }}
            >
              <span style={{ fontSize: FONT.sizeMd, color: COLORS.text }}>{task.name}</span>
              <span style={{ fontSize: FONT.sizeSm, color: STATUS_COLORS[task.status] }}>
                {STATUS_LABELS[task.status] ?? task.status}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  )

  const renderOcr = () => (
    <div style={{ ...cardStyle }}>
      {renderCurrentDocument()}
      <h3 style={{ margin: '0 0 8px', fontSize: FONT.sizeXl, fontWeight: 600 }}>OCR</h3>
      <p style={{ margin: '0 0 14px', color: COLORS.textSecondary, fontSize: FONT.sizeMd }}>
        使用本地 Rust + PaddleOCR ONNX Worker 识别图片，模型不会上传到网络。
      </p>
      <label
        style={{
          display: 'block',
          fontSize: FONT.sizeSm,
          color: COLORS.textSecondary,
          marginBottom: 6
        }}
      >
        图片文件
      </label>
      <input
        value={displayPath(ocrPath)}
        readOnly
        title={ocrPath || undefined}
        placeholder="请点击“选择图片”，或从概览拖入"
        style={{
          width: '100%',
          boxSizing: 'border-box',
          padding: '9px 10px',
          border: `1px solid ${COLORS.border}`,
          borderRadius: 6,
          fontSize: FONT.sizeMd
        }}
      />
      <button
        type="button"
        onClick={async () => {
          const [path] = await selectPaths({
            type: 'file',
            extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'tif', 'tiff']
          })
          if (path) setSharedDocument(path, 'image', 'picker')
        }}
        style={{
          marginTop: 8,
          padding: '6px 10px',
          border: `1px solid ${COLORS.border}`,
          borderRadius: 5,
          background: COLORS.bgWhite
        }}
      >
        选择图片
      </button>
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button
          type="button"
          onClick={() => void runOcr()}
          disabled={ocrBusy || ocrTask?.status === 'running' || ocrTask?.status === 'queued'}
          style={{
            padding: '8px 14px',
            border: 0,
            borderRadius: 6,
            background: COLORS.primary,
            color: '#fff',
            cursor: 'pointer'
          }}
        >
          {ocrBusy ? '启动中…' : '开始 OCR'}
        </button>
        {(ocrTask?.status === 'running' || ocrTask?.status === 'queued') && (
          <button
            type="button"
            onClick={() => void stopOcr()}
            style={{
              padding: '8px 14px',
              border: `1px solid ${COLORS.warningBorder}`,
              borderRadius: 6,
              background: COLORS.warningBg,
              color: COLORS.warning,
              cursor: 'pointer'
            }}
          >
            取消
          </button>
        )}
      </div>
      {ocrProgress && (
        <div
          style={{ marginTop: 16, padding: 12, background: COLORS.primaryLight, borderRadius: 6 }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: FONT.sizeSm }}>
            <span>{ocrProgress.message}</span>
            <span>{Math.round(ocrProgress.percent)}%</span>
          </div>
          <div style={{ height: 6, background: COLORS.border, borderRadius: 3, marginTop: 8 }}>
            <div
              style={{
                width: `${Math.max(0, Math.min(100, ocrProgress.percent))}%`,
                height: '100%',
                background: COLORS.primary,
                borderRadius: 3
              }}
            />
          </div>
        </div>
      )}
      {ocrError && (
        <div
          style={{
            marginTop: 12,
            padding: 10,
            background: COLORS.dangerBg,
            color: COLORS.danger,
            borderRadius: 6,
            fontSize: FONT.sizeSm
          }}
        >
          {ocrError}
        </div>
      )}
      {ocrResult && (
        <div style={{ marginTop: 16 }}>
          <div style={{ fontWeight: 600, marginBottom: 6 }}>识别结果</div>
          <pre
            style={{
              whiteSpace: 'pre-wrap',
              margin: 0,
              padding: 12,
              background: COLORS.bgGray,
              border: `1px solid ${COLORS.border}`,
              borderRadius: 6,
              fontSize: FONT.sizeMd
            }}
          >
            {ocrResult.text || '（未识别到文字）'}
          </pre>
          <div style={{ marginTop: 6, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
            文本块：{ocrResult.blocks.length}
          </div>
        </div>
      )}
    </div>
  )

  const renderParse = () => {
    const metadata = parseResult?.document.metadata
    return (
      <div style={{ ...cardStyle }}>
        {renderCurrentDocument()}
        <h3 style={{ margin: '0 0 8px', fontSize: FONT.sizeXl, fontWeight: 600 }}>PDF 解析</h3>
        <p style={{ margin: '0 0 14px', color: COLORS.textSecondary, fontSize: FONT.sizeMd }}>
          提取 PDF 文本层并转换为统一 Document JSON；扫描页会标记为待 OCR。
        </p>
        <label
          style={{
            display: 'block',
            fontSize: FONT.sizeSm,
            color: COLORS.textSecondary,
            marginBottom: 6
          }}
        >
          PDF 文件
        </label>
        <input
          value={displayPath(parsePath)}
          readOnly
          title={parsePath || undefined}
          placeholder="请点击“选择 PDF”，或从概览拖入"
          style={{
            width: '100%',
            boxSizing: 'border-box',
            padding: '9px 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6,
            fontSize: FONT.sizeMd
          }}
        />
        <button
          type="button"
          onClick={async () => {
            const [path] = await selectPaths({ type: 'file', extensions: ['pdf'] })
            if (path) setSharedDocument(path, 'pdf', 'picker')
          }}
          style={{
            marginTop: 8,
            padding: '6px 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 5,
            background: COLORS.bgWhite
          }}
        >
          选择 PDF
        </button>
        <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
          <button
            type="button"
            onClick={() => void runParse()}
            disabled={
              parseBusy || parseTask?.status === 'running' || parseTask?.status === 'queued'
            }
            style={{
              padding: '8px 14px',
              border: 0,
              borderRadius: 6,
              background: COLORS.primary,
              color: '#fff',
              cursor: 'pointer'
            }}
          >
            {parseBusy ? '启动中…' : '开始解析'}
          </button>
          {(parseTask?.status === 'running' || parseTask?.status === 'queued') && (
            <button
              type="button"
              onClick={() => void stopParse()}
              style={{
                padding: '8px 14px',
                border: `1px solid ${COLORS.warningBorder}`,
                borderRadius: 6,
                background: COLORS.warningBg,
                color: COLORS.warning,
                cursor: 'pointer'
              }}
            >
              取消
            </button>
          )}
        </div>
        {parseProgress && (
          <div
            style={{ marginTop: 16, padding: 12, background: COLORS.primaryLight, borderRadius: 6 }}
          >
            <div
              style={{ display: 'flex', justifyContent: 'space-between', fontSize: FONT.sizeSm }}
            >
              <span>{parseProgress.message}</span>
              <span>{Math.round(parseProgress.percent)}%</span>
            </div>
            <div style={{ height: 6, background: COLORS.border, borderRadius: 3, marginTop: 8 }}>
              <div
                style={{
                  width: `${Math.max(0, Math.min(100, parseProgress.percent))}%`,
                  height: '100%',
                  background: COLORS.primary,
                  borderRadius: 3
                }}
              />
            </div>
          </div>
        )}
        {parseError && (
          <div
            style={{
              marginTop: 12,
              padding: 10,
              background: COLORS.dangerBg,
              color: COLORS.danger,
              borderRadius: 6,
              fontSize: FONT.sizeSm
            }}
          >
            {parseError}
          </div>
        )}
        {metadata && (
          <div
            style={{
              marginTop: 16,
              display: 'flex',
              flexWrap: 'wrap',
              gap: 8,
              fontSize: FONT.sizeSm
            }}
          >
            <StatusBadge
              state={parseResult?.route === 'native' ? 'ok' : 'neutral'}
              label={`路由：${parseResult?.route}`}
            />
            <StatusBadge
              state={metadata.hasTextLayer ? 'ok' : 'neutral'}
              label={metadata.hasTextLayer ? '含文本层' : '扫描页'}
            />
            <span
              style={{ padding: '2px 8px', border: `1px solid ${COLORS.border}`, borderRadius: 4 }}
            >
              页数：{metadata.pageCount}
            </span>
          </div>
        )}
        {parseResult?.warnings.map((warning) => (
          <div
            key={`${warning.code ?? 'warning'}-${warning.message ?? ''}`}
            style={{
              marginTop: 12,
              padding: 10,
              background: COLORS.warningBg,
              color: COLORS.warning,
              borderRadius: 6,
              fontSize: FONT.sizeSm
            }}
          >
            ⚠ {warning.message ?? warning.code ?? '部分页面需要 OCR'}
          </div>
        ))}
        {parseResult && (
          <div style={{ marginTop: 16 }}>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>解析结果</div>
            <pre
              style={{
                whiteSpace: 'pre-wrap',
                maxHeight: 360,
                overflow: 'auto',
                margin: 0,
                padding: 12,
                background: COLORS.bgGray,
                border: `1px solid ${COLORS.border}`,
                borderRadius: 6,
                fontSize: FONT.sizeSm
              }}
            >
              {parseResult.document.pages
                .map((page) => {
                  const text = page.blocks
                    .map((block) => block.content ?? '')
                    .filter(Boolean)
                    .join('\n')
                  return `第 ${page.number} 页${text ? `\n${text}` : '\n（无文本层）'}`
                })
                .join('\n\n')}
            </pre>
          </div>
        )}
      </div>
    )
  }

  const renderChunk = () => (
    <div style={{ ...cardStyle }}>
      {renderCurrentDocument()}
      <h3 style={{ margin: '0 0 8px', fontSize: FONT.sizeXl, fontWeight: 600 }}>文档切分</h3>
      <p style={{ margin: '0 0 14px', color: COLORS.textSecondary, fontSize: FONT.sizeMd }}>
        按结构/语义边界生成 RAG 可用 Chunk。
      </p>
      <input
        value={displayPath(chunkPath)}
        readOnly
        title={chunkPath || undefined}
        placeholder="请点击“选择文档”，或从概览拖入"
        style={{
          width: '100%',
          boxSizing: 'border-box',
          padding: '9px 10px',
          border: `1px solid ${COLORS.border}`,
          borderRadius: 6,
          fontSize: FONT.sizeMd
        }}
      />
      <button
        type="button"
        onClick={async () => {
          const [path] = await selectPaths({ type: 'file' })
          if (path) {
            setSharedDocument(path, /\.pdf$/i.test(path) ? 'pdf' : 'document', 'picker')
            setChunkPath(path)
          }
        }}
        style={{
          marginTop: 8,
          padding: '6px 10px',
          border: `1px solid ${COLORS.border}`,
          borderRadius: 5,
          background: COLORS.bgWhite
        }}
      >
        选择文档
      </button>
      <label style={{ display: 'block', marginTop: 12, fontSize: FONT.sizeMd, color: COLORS.text }}>
        切分方式
        <select
          value={chunkStrategy}
          onChange={(event) => setChunkStrategy(event.target.value as typeof chunkStrategy)}
          style={{
            display: 'block',
            width: '100%',
            marginTop: 6,
            height: 36,
            padding: '0 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6,
            background: COLORS.bgWhite,
            color: COLORS.text
          }}
        >
          <option value="hybrid">结构 + 长度（推荐）</option>
          <option value="pages">按页切分</option>
          <option value="chapters">按章节/标题切分</option>
          <option value="structure">按结构边界</option>
          <option value="semantic">按语义/长度</option>
        </select>
      </label>
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button
          type="button"
          onClick={() => void runChunk()}
          disabled={chunkBusy || chunkTask?.status === 'running' || chunkTask?.status === 'queued'}
          style={{
            padding: '8px 14px',
            border: 0,
            borderRadius: 6,
            background: COLORS.primary,
            color: '#fff',
            cursor: 'pointer'
          }}
        >
          {chunkBusy ? '启动中…' : '开始切分'}
        </button>
        {(chunkTask?.status === 'running' || chunkTask?.status === 'queued') && (
          <button
            type="button"
            onClick={() => void stopUtility(chunkTaskId, setChunkError)}
            style={{
              padding: '8px 14px',
              border: `1px solid ${COLORS.warningBorder}`,
              borderRadius: 6,
              background: COLORS.warningBg,
              color: COLORS.warning,
              cursor: 'pointer'
            }}
          >
            取消
          </button>
        )}
      </div>
      {chunkProgress && (
        <div style={{ marginTop: 14, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
          {chunkProgress.message}（{Math.round(chunkProgress.percent)}%）
        </div>
      )}
      {chunkError && (
        <div
          style={{
            marginTop: 12,
            padding: 10,
            background: COLORS.dangerBg,
            color: COLORS.danger,
            borderRadius: 6,
            fontSize: FONT.sizeSm
          }}
        >
          {chunkError}
        </div>
      )}
      {chunkResult != null && (
        <pre
          style={{
            marginTop: 14,
            whiteSpace: 'pre-wrap',
            maxHeight: 360,
            overflow: 'auto',
            padding: 12,
            background: COLORS.bgGray,
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6,
            fontSize: FONT.sizeSm
          }}
        >
          {prettyJson(chunkResult)}
        </pre>
      )}
    </div>
  )

  const renderConvert = () => (
    <div style={{ ...cardStyle }}>
      {renderCurrentDocument()}
      <h3 style={{ margin: '0 0 8px', fontSize: FONT.sizeXl, fontWeight: 600 }}>格式转换</h3>
      <p style={{ margin: '0 0 14px', color: COLORS.textSecondary, fontSize: FONT.sizeMd }}>
        所有转换先经过统一 Document 模型。
      </p>
      <input
        value={displayPath(convertPath)}
        readOnly
        title={convertPath || undefined}
        placeholder="请点击“选择源文档”，或从概览拖入"
        style={{
          width: '100%',
          boxSizing: 'border-box',
          padding: '9px 10px',
          border: `1px solid ${COLORS.border}`,
          borderRadius: 6,
          fontSize: FONT.sizeMd
        }}
      />
      <button
        type="button"
        onClick={async () => {
          const [path] = await selectPaths({ type: 'file' })
          if (path) {
            setSharedDocument(path, /\.pdf$/i.test(path) ? 'pdf' : 'document', 'picker')
            setConvertPath(path)
          }
        }}
        style={{
          marginTop: 8,
          padding: '6px 10px',
          border: `1px solid ${COLORS.border}`,
          borderRadius: 5,
          background: COLORS.bgWhite
        }}
      >
        选择源文档
      </button>
      <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
        <select
          value={convertTarget}
          onChange={(event) => setConvertTarget(event.target.value)}
          style={{ padding: '8px 10px', border: `1px solid ${COLORS.border}`, borderRadius: 6 }}
        >
          <option value="md">Markdown</option>
          <option value="txt">TXT</option>
          <option value="html">HTML</option>
          <option value="json">JSON</option>
          <option value="docx">DOCX</option>
          <option value="pdf">PDF</option>
        </select>
        <input
          value={convertOutput}
          onChange={(event) => setConvertOutput(event.target.value)}
          placeholder="输出路径（可选）"
          style={{
            flex: 1,
            padding: '9px 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6,
            fontSize: FONT.sizeMd
          }}
        />
      </div>
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button
          type="button"
          onClick={() => void runConvert()}
          disabled={
            convertBusy || convertTask?.status === 'running' || convertTask?.status === 'queued'
          }
          style={{
            padding: '8px 14px',
            border: 0,
            borderRadius: 6,
            background: COLORS.primary,
            color: '#fff',
            cursor: 'pointer'
          }}
        >
          {convertBusy ? '启动中…' : '开始转换'}
        </button>
        {(convertTask?.status === 'running' || convertTask?.status === 'queued') && (
          <button
            type="button"
            onClick={() => void stopUtility(convertTaskId, setConvertError)}
            style={{
              padding: '8px 14px',
              border: `1px solid ${COLORS.warningBorder}`,
              borderRadius: 6,
              background: COLORS.warningBg,
              color: COLORS.warning,
              cursor: 'pointer'
            }}
          >
            取消
          </button>
        )}
      </div>
      {convertProgress && (
        <div style={{ marginTop: 14, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
          {convertProgress.message}（{Math.round(convertProgress.percent)}%）
        </div>
      )}
      {convertError && (
        <div
          style={{
            marginTop: 12,
            padding: 10,
            background: COLORS.dangerBg,
            color: COLORS.danger,
            borderRadius: 6,
            fontSize: FONT.sizeSm
          }}
        >
          {convertError}
        </div>
      )}
      {convertResult != null && (
        <pre
          style={{
            marginTop: 14,
            whiteSpace: 'pre-wrap',
            padding: 12,
            background: COLORS.successBg,
            border: `1px solid ${COLORS.successBorder}`,
            borderRadius: 6,
            fontSize: FONT.sizeSm
          }}
        >
          {prettyJson(convertResult)}
        </pre>
      )}
    </div>
  )

  const renderBatch = () => (
    <div style={{ ...cardStyle }}>
      <h3 style={{ margin: '0 0 8px', fontSize: FONT.sizeXl, fontWeight: 600 }}>批量处理</h3>
      <p style={{ margin: '0 0 8px', color: COLORS.textSecondary, fontSize: FONT.sizeMd }}>
        每行一个文件绝对路径，最多 1000 个。
      </p>
      <textarea
        value={batchPaths}
        onChange={(event) => setBatchPaths(event.target.value)}
        placeholder={'E:\\Docs\\a.pdf\nE:\\Docs\\b.md'}
        rows={6}
        style={{
          width: '100%',
          boxSizing: 'border-box',
          padding: 10,
          border: `1px solid ${COLORS.border}`,
          borderRadius: 6,
          fontFamily: 'monospace'
        }}
      />
      <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
        <button
          type="button"
          onClick={async () => {
            const paths = await selectPaths({ type: 'file', multiple: true })
            if (paths.length) {
              clearSharedDocument()
              setBatchPaths(paths.join('\n'))
            }
          }}
          style={{
            padding: '6px 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 5,
            background: COLORS.bgWhite
          }}
        >
          选择多个文件
        </button>
        <button
          type="button"
          onClick={() => void selectFolderFiles(setBatchPaths)}
          style={{
            padding: '6px 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 5,
            background: COLORS.bgWhite
          }}
        >
          选择文件夹
        </button>
      </div>
      <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
        <select
          value={batchOperation}
          onChange={(event) => setBatchOperation(event.target.value as 'ocr' | 'parse' | 'convert')}
          style={{ padding: '8px 10px', border: `1px solid ${COLORS.border}`, borderRadius: 6 }}
        >
          <option value="ocr">OCR（图片/PDF）</option>
          <option value="parse">解析</option>
          <option value="convert">转换</option>
        </select>
        <input
          value={batchTarget}
          onChange={(event) => setBatchTarget(event.target.value)}
          placeholder="转换目标（如 md）"
          disabled={batchOperation === 'parse'}
          style={{
            flex: 1,
            padding: '9px 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6
          }}
        />
      </div>
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button
          type="button"
          onClick={() => void runBatch()}
          disabled={batchBusy || batchTask?.status === 'running' || batchTask?.status === 'queued'}
          style={{
            padding: '8px 14px',
            border: 0,
            borderRadius: 6,
            background: COLORS.primary,
            color: '#fff',
            cursor: 'pointer'
          }}
        >
          {batchBusy ? '启动中…' : '开始批量处理'}
        </button>
        {(batchTask?.status === 'running' || batchTask?.status === 'queued') && (
          <button
            type="button"
            onClick={() => void stopUtility(batchTaskId, setBatchError)}
            style={{
              padding: '8px 14px',
              border: `1px solid ${COLORS.warningBorder}`,
              borderRadius: 6,
              background: COLORS.warningBg,
              color: COLORS.warning,
              cursor: 'pointer'
            }}
          >
            取消
          </button>
        )}
      </div>
      {batchProgress && (
        <div style={{ marginTop: 14, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
          {batchProgress.message}（{Math.round(batchProgress.percent)}%）
        </div>
      )}
      {batchError && (
        <div
          style={{
            marginTop: 12,
            padding: 10,
            background: COLORS.dangerBg,
            color: COLORS.danger,
            borderRadius: 6,
            fontSize: FONT.sizeSm
          }}
        >
          {batchError}
        </div>
      )}
      {batchResult != null && (
        <pre
          style={{
            marginTop: 14,
            whiteSpace: 'pre-wrap',
            maxHeight: 360,
            overflow: 'auto',
            padding: 12,
            background: COLORS.bgGray,
            border: `1px solid ${COLORS.border}`,
            borderRadius: 6,
            fontSize: FONT.sizeSm
          }}
        >
          {prettyJson(batchResult)}
        </pre>
      )}
    </div>
  )

  const renderJobs = () => (
    <div style={{ ...cardStyle }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h3 style={{ margin: 0, fontSize: FONT.sizeXl, fontWeight: 600 }}>任务队列</h3>
        <button
          type="button"
          onClick={() => void refreshJobs()}
          style={{
            padding: '5px 10px',
            border: `1px solid ${COLORS.border}`,
            borderRadius: 5,
            background: COLORS.bgWhite
          }}
        >
          刷新
        </button>
      </div>
      {jobsError && (
        <div style={{ marginTop: 12, color: COLORS.danger, fontSize: FONT.sizeSm }}>
          {jobsError}
        </div>
      )}
      {allTasks.length === 0 ? (
        <div style={{ marginTop: 16, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
          暂无任务
        </div>
      ) : (
        allTasks.map((task) => {
          const active =
            task.status === 'queued' || task.status === 'running' || task.status === 'paused'
          const run = async (operation: 'pause' | 'resume' | 'cancel' | 'retry') => {
            try {
              if (operation === 'pause') await pauseTask(send, task.taskId)
              if (operation === 'resume') await resumeTask(send, task.taskId)
              if (operation === 'cancel') await cancelTask(send, task.taskId)
              if (operation === 'retry') await retryTask(send, task.taskId)
              await refreshJobs()
            } catch (error) {
              setJobsError(error instanceof Error ? error.message : String(error))
            }
          }
          return (
            <div
              key={task.taskId}
              style={{
                marginTop: 10,
                padding: 10,
                border: `1px solid ${COLORS.border}`,
                borderRadius: 6
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: 8 }}>
                <span style={{ fontFamily: 'monospace', fontSize: FONT.sizeSm }}>
                  {task.taskId.slice(0, 12)}…
                </span>
                <span style={{ color: STATUS_COLORS[task.status] ?? COLORS.text }}>
                  {STATUS_LABELS[task.status] ?? task.status}
                </span>
              </div>
              <div style={{ marginTop: 4, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
                资源：{task.resourceKey}
                {task.progress
                  ? ` · ${task.progress.message} ${Math.round(task.progress.percent)}%`
                  : ''}
              </div>
              {active && (
                <div style={{ display: 'flex', gap: 6, marginTop: 8 }}>
                  {task.status === 'paused' ? (
                    <button
                      type="button"
                      onClick={() => void run('resume')}
                      style={{
                        padding: '4px 9px',
                        border: `1px solid ${COLORS.border}`,
                        borderRadius: 4,
                        background: COLORS.bgWhite
                      }}
                    >
                      恢复
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => void run('pause')}
                      style={{
                        padding: '4px 9px',
                        border: `1px solid ${COLORS.border}`,
                        borderRadius: 4,
                        background: COLORS.bgWhite
                      }}
                    >
                      暂停
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => void run('cancel')}
                    style={{
                      padding: '4px 9px',
                      border: `1px solid ${COLORS.warningBorder}`,
                      borderRadius: 4,
                      background: COLORS.warningBg,
                      color: COLORS.warning
                    }}
                  >
                    取消
                  </button>
                </div>
              )}
              {(task.status === 'failed' || task.status === 'cancelled') && (
                <button
                  type="button"
                  onClick={() => void run('retry')}
                  style={{
                    marginTop: 8,
                    padding: '4px 9px',
                    border: `1px solid ${COLORS.border}`,
                    borderRadius: 4,
                    background: COLORS.bgWhite
                  }}
                >
                  重试
                </button>
              )}
            </div>
          )
        })
      )}
    </div>
  )

  const renderModels = () => {
    const installedNames = new Set(
      models
        .map((model) => (typeof model.relativePath === 'string' ? model.relativePath : ''))
        .filter(Boolean)
    )
    return (
      <div style={{ ...cardStyle }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h3 style={{ margin: 0, fontSize: FONT.sizeXl, fontWeight: 600 }}>模型与缓存</h3>
          <button
            type="button"
            onClick={() => {
              void refreshModels()
              void refreshModelCatalog()
            }}
            style={{
              padding: '5px 10px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 5,
              background: COLORS.bgWhite
            }}
          >
            刷新
          </button>
        </div>
        <div
          style={{
            marginTop: 16,
            padding: 12,
            border: `1px solid ${COLORS.primary}`,
            borderRadius: 8,
            background: COLORS.primaryLight
          }}
        >
          <div style={{ fontWeight: 600, fontSize: FONT.sizeLg }}>推荐模型</div>
          <div style={{ marginTop: 4, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
            首次使用建议安装标准 PP-OCRv4；包含 OCR 所需的检测、识别和字典文件。
          </div>
          <div style={{ marginTop: 4, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
            下载地址已固定并逐文件校验 SHA-256，安装完成后即可用于本地 OCR。
          </div>
          {modelCatalog.length === 0 ? (
            <div
              style={{
                marginTop: 10,
                color: modelCatalogError ? COLORS.danger : COLORS.textSecondary,
                fontSize: FONT.sizeSm
              }}
            >
              {modelCatalogError ?? '正在读取可下载模型目录…'}
            </div>
          ) : (
            modelCatalog.map((entry) => {
              const bundle = modelBundles.find((candidate) => candidate.id === entry.id)
              const ready =
                bundle?.ready ??
                entry.artifacts.every((artifact) => installedNames.has(artifact.name))
              return (
                <div
                  key={entry.id}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: 12,
                    marginTop: 10,
                    padding: 10,
                    background: COLORS.bgWhite,
                    border: `1px solid ${COLORS.border}`,
                    borderRadius: 6
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div style={{ fontWeight: 600 }}>{entry.name}</div>
                    <div
                      style={{ color: COLORS.textSecondary, fontSize: FONT.sizeSm, marginTop: 3 }}
                    >
                      {entry.description} · {entry.artifacts.length} 个文件 ·{' '}
                      {Math.round((entry.totalBytes ?? 0) / 1024 / 1024)} MiB
                    </div>
                    <div
                      style={{ color: COLORS.textSecondary, fontSize: FONT.sizeSm, marginTop: 3 }}
                    >
                      {ready
                        ? '已校验，可离线使用'
                        : bundle?.missing?.length
                          ? `缺少 ${bundle.missing.length} 个文件`
                          : entry.offline
                            ? '插件内置，可离线安装'
                            : '支持镜像回退下载'}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => void installCatalogModel(entry.id)}
                    disabled={modelsBusy || ready}
                    style={{
                      flexShrink: 0,
                      padding: '7px 12px',
                      border: 0,
                      borderRadius: 5,
                      background: ready ? COLORS.successBg : COLORS.primary,
                      color: ready ? COLORS.success : '#fff'
                    }}
                  >
                    {ready ? '已安装' : modelsBusy ? '安装中…' : '下载并安装'}
                  </button>
                </div>
              )
            })
          )}
        </div>
        <div style={{ marginTop: 16, fontWeight: 600, fontSize: FONT.sizeLg }}>高级安装</div>
        <div style={{ marginTop: 4, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
          已有本地模型目录或受信任下载地址时使用；普通用户直接安装上面的推荐模型即可。
        </div>
        <div style={{ display: 'flex', gap: 8, marginTop: 14 }}>
          <input
            value={modelSource}
            onChange={(event) => setModelSource(event.target.value)}
            placeholder="本地模型文件/目录路径"
            style={{
              flex: 1,
              padding: '8px 10px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 6
            }}
          />
          <button
            type="button"
            onClick={async () => {
              const [path] = await selectPaths({ type: 'folder' })
              if (path) setModelSource(path)
            }}
            style={{
              padding: '8px 10px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 6,
              background: COLORS.bgWhite
            }}
          >
            选择目录
          </button>
          <input
            value={modelName}
            onChange={(event) => setModelName(event.target.value)}
            placeholder="名称（可选）"
            style={{
              width: 130,
              padding: '8px 10px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 6
            }}
          />
          <button
            type="button"
            onClick={() => void installLocalModel()}
            disabled={modelsBusy}
            style={{
              padding: '8px 12px',
              border: 0,
              borderRadius: 6,
              background: COLORS.primary,
              color: '#fff'
            }}
          >
            安装
          </button>
        </div>
        <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
          <input
            value={modelUrl}
            onChange={(event) => setModelUrl(event.target.value)}
            placeholder="远程 HTTPS 模型地址（允许官方域名）"
            style={{
              flex: 2,
              padding: '8px 10px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 6
            }}
          />
          <input
            value={modelSha256}
            onChange={(event) => setModelSha256(event.target.value)}
            placeholder="SHA-256"
            style={{
              flex: 1,
              padding: '8px 10px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 6,
              fontFamily: 'monospace'
            }}
          />
          <button
            type="button"
            onClick={() => void installRemote()}
            disabled={modelsBusy}
            style={{
              padding: '8px 12px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 6,
              background: COLORS.bgWhite
            }}
          >
            远程安装
          </button>
        </div>
        <button
          type="button"
          onClick={() => void clearEngineCache()}
          disabled={modelsBusy}
          style={{
            marginTop: 10,
            padding: '6px 10px',
            border: `1px solid ${COLORS.warningBorder}`,
            borderRadius: 5,
            background: COLORS.warningBg,
            color: COLORS.warning
          }}
        >
          清理缓存
        </button>
        {modelsError && (
          <div style={{ marginTop: 12, color: COLORS.danger, fontSize: FONT.sizeSm }}>
            {modelsError}
          </div>
        )}
        {models.length === 0 ? (
          <div style={{ marginTop: 16, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
            模型目录为空
          </div>
        ) : (
          models.map((model) => {
            const path = typeof model.relativePath === 'string' ? model.relativePath : ''
            return (
              <div
                key={path}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  gap: 8,
                  marginTop: 8,
                  padding: 8,
                  border: `1px solid ${COLORS.border}`,
                  borderRadius: 5,
                  fontSize: FONT.sizeSm
                }}
              >
                <span>
                  {path} · {String(model.bytes ?? 0)} bytes
                </span>
                <span style={{ display: 'flex', gap: 6 }}>
                  <button
                    type="button"
                    onClick={() => void updateRemote(path)}
                    disabled={modelsBusy}
                    style={{
                      border: `1px solid ${COLORS.border}`,
                      borderRadius: 4,
                      background: COLORS.bgWhite
                    }}
                  >
                    更新
                  </button>
                  <button
                    type="button"
                    onClick={() => void deleteModel(path)}
                    disabled={modelsBusy}
                    style={{
                      border: `1px solid ${COLORS.danger}`,
                      borderRadius: 4,
                      background: COLORS.dangerBg,
                      color: COLORS.danger
                    }}
                  >
                    删除
                  </button>
                </span>
              </div>
            )
          })
        )}
      </div>
    )
  }

  const renderHistory = () => {
    const completed = allTasks.filter((task) => isTerminalTask(task))
    return (
      <div style={{ ...cardStyle }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h3 style={{ margin: 0, fontSize: FONT.sizeXl, fontWeight: 600 }}>历史记录</h3>
          <button
            type="button"
            onClick={() => void refreshJobs()}
            style={{
              padding: '5px 10px',
              border: `1px solid ${COLORS.border}`,
              borderRadius: 5,
              background: COLORS.bgWhite
            }}
          >
            刷新
          </button>
        </div>
        {completed.length === 0 ? (
          <div style={{ marginTop: 16, color: COLORS.textSecondary, fontSize: FONT.sizeSm }}>
            暂无已完成任务
          </div>
        ) : (
          completed.map((task) => (
            <div
              key={task.taskId}
              style={{
                marginTop: 10,
                padding: 10,
                border: `1px solid ${COLORS.border}`,
                borderRadius: 6
              }}
            >
              <div
                style={{ display: 'flex', justifyContent: 'space-between', fontSize: FONT.sizeSm }}
              >
                <span>{task.resourceKey}</span>
                <span style={{ color: STATUS_COLORS[task.status] ?? COLORS.text }}>
                  {STATUS_LABELS[task.status] ?? task.status}
                </span>
              </div>
              {task.error && (
                <div style={{ marginTop: 4, color: COLORS.danger, fontSize: FONT.sizeSm }}>
                  {task.error.message}
                </div>
              )}
              {task.result != null && (
                <pre
                  style={{
                    margin: '6px 0 0',
                    whiteSpace: 'pre-wrap',
                    maxHeight: 140,
                    overflow: 'auto',
                    fontSize: FONT.sizeXs
                  }}
                >
                  {prettyJson(task.result)}
                </pre>
              )}
            </div>
          ))
        )}
      </div>
    )
  }

  const renderBody = () => {
    switch (activeKey) {
      case 'overview':
        return renderOverview()
      case 'ocr':
        return renderOcr()
      case 'parse':
        return renderParse()
      case 'convert':
        return renderConvert()
      case 'chunk':
        return renderChunk()
      case 'batch':
        return renderBatch()
      case 'jobs':
        return renderJobs()
      case 'history':
        return renderHistory()
      case 'models':
        return renderModels()
      default:
        return renderOverview()
    }
  }

  return (
    <div
      style={{
        fontFamily: FONT.family,
        color: COLORS.text,
        minHeight: '100%',
        display: 'flex',
        flexDirection: 'column'
      }}
    >
      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        {/* 左侧导航 */}
        <div
          style={{
            width: 180,
            minWidth: 180,
            background: COLORS.bgWhite,
            borderRight: `1px solid ${COLORS.border}`,
            overflowY: 'auto',
            display: 'flex',
            flexDirection: 'column',
            flexShrink: 0
          }}
        >
          <div
            style={{
              padding: '14px 16px',
              borderBottom: `1px solid ${COLORS.border}`,
              fontSize: FONT.sizeXl,
              fontWeight: 600,
              color: COLORS.text
            }}
          >
            📄 Document Engine
          </div>
          {NAV_ITEMS.map((item) => (
            <div
              key={item.key}
              style={{
                padding: '10px 16px',
                cursor: 'pointer',
                borderRadius: 6,
                margin: '2px 8px',
                fontSize: FONT.sizeLg,
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                background: activeKey === item.key ? COLORS.primaryLight : 'transparent',
                color: activeKey === item.key ? COLORS.primary : COLORS.text,
                fontWeight: activeKey === item.key ? 600 : 400
              }}
              onClick={() => setActiveKey(item.key)}
            >
              <span style={{ fontSize: 16 }}>{item.icon}</span>
              {item.label}
            </div>
          ))}
        </div>

        {/* 右侧内容 */}
        <div
          style={{
            flex: 1,
            padding: 20,
            overflowY: 'auto',
            background: COLORS.bgGray
          }}
        >
          {renderBody()}
        </div>
      </div>
    </div>
  )
}

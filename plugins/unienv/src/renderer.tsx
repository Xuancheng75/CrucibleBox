import React, { useState, useEffect, useCallback, useRef } from 'react'
import {
  assertTaskCancellationAccepted,
  isTaskPollingAborted,
  pollTask,
  readStartedTaskId
} from './renderer-task'
import type {
  TaskSnapshot,
  TaskStatus
} from '../../../plugin-system/trusted-services/unienv/task-manager'
import {
  TOOL_VERSION_LIFECYCLE_AS_OF,
  type ToolId,
  formatComboLifecycleSummary,
  formatToolVersionOption,
  getPreferredToolVersion,
  getToolVersionLifecycle,
  orderToolVersionsForDisplay,
  requiresComboVersionConfirmation,
  requiresToolVersionConfirmation
} from './catalog'

// ============================================================
// 内联样式常量 — 复刻 Ant Design 5.x 视觉风格
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
  dangerHover: 'var(--ob-color-error, #ff7875)',
  dangerBg: 'var(--ob-color-error-bg, #fff2f0)',
  text: 'var(--ob-color-text, #1f1f1f)',
  textSecondary: 'var(--ob-color-text-secondary, #8c8c8c)',
  textTertiary: 'var(--ob-color-text-tertiary, #bfbfbf)',
  border: 'var(--ob-color-border, #f0f0f0)',
  borderLight: 'var(--ob-color-border-secondary, #f5f5f5)',
  bgWhite: 'var(--ob-color-bg-container, #ffffff)',
  bgGray: 'var(--ob-color-bg, #fafafa)',
  bgGrayDark: 'var(--ob-color-bg, #f5f5f5)',
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

// ============================================================
// 基础组件
// ============================================================

function Spinner({ size = 32, tip }: { size?: number; tip?: string }) {
  return (
    <div style={{ textAlign: 'center', padding: '40px 0' }}>
      <div
        style={{
          display: 'inline-block',
          width: size,
          height: size,
          border: `3px solid ${COLORS.borderLight}`,
          borderTopColor: COLORS.primary,
          borderRadius: '50%',
          animation: 'unienv-spin 0.8s linear infinite'
        }}
      />
      {tip && (
        <p style={{ color: COLORS.textSecondary, fontSize: FONT.sizeMd, marginTop: 12 }}>{tip}</p>
      )}
      <style>{`@keyframes unienv-spin { to { transform: rotate(360deg); } }`}</style>
    </div>
  )
}

function Toast({
  items,
  onRemove
}: {
  items: { id: number; type: string; content: string }[]
  onRemove: (id: number) => void
}) {
  if (items.length === 0) return null
  const typeStyles: Record<string, React.CSSProperties> = {
    success: {
      background: COLORS.successBg,
      border: `1px solid ${COLORS.successBorder}`,
      color: 'var(--ob-color-success, #389e0d)'
    },
    error: {
      background: COLORS.dangerBg,
      border: '1px solid var(--ob-color-error-border, #ffccc7)',
      color: 'var(--ob-color-error, #cf1322)'
    },
    warning: {
      background: COLORS.warningBg,
      border: `1px solid ${COLORS.warningBorder}`,
      color: 'var(--ob-color-warning, #d48806)'
    },
    info: {
      background: COLORS.primaryLight,
      border: '1px solid var(--ob-color-primary, #91caff)',
      color: 'var(--ob-color-primary, #0958d9)'
    }
  }
  return (
    <div
      style={{
        position: 'fixed',
        top: 24,
        right: 24,
        zIndex: 9999,
        display: 'flex',
        flexDirection: 'column',
        gap: 8
      }}
    >
      {items.map((t) => (
        <div
          key={t.id}
          style={{
            padding: '10px 16px',
            borderRadius: 8,
            fontSize: FONT.sizeLg,
            boxShadow: COLORS.shadow,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            minWidth: 240,
            cursor: 'pointer',
            ...(typeStyles[t.type] || typeStyles.info)
          }}
          onClick={() => onRemove(t.id)}
        >
          <span>
            {t.type === 'success'
              ? '✓'
              : t.type === 'error'
                ? '✕'
                : t.type === 'warning'
                  ? '⚠'
                  : 'ℹ'}
          </span>
          <span>{t.content}</span>
          <span style={{ marginLeft: 'auto', opacity: 0.5, fontSize: FONT.sizeSm }}>✕</span>
        </div>
      ))}
    </div>
  )
}

// ============================================================
// 类型定义
// ============================================================

interface ToolInfo {
  installed: boolean
  version?: string
  path?: string
  error?: string
}

interface ToolItem {
  id: string
  displayName: string
  icon: string
  description: string
}

interface ComboPack {
  id: string
  name: string
  description: string
  items: { toolId: string; version: string }[]
}

interface ProgressData {
  stage: string
  percent: number
  message: string
}

interface InstallTaskResult {
  kind: 'install'
  tool: string
  version: string
  message: string
}

interface ComboTaskItemResult {
  tool: string
  success: boolean
  message: string
}

interface ComboTaskResult {
  kind: 'combo'
  comboId: string
  success: boolean
  results: ComboTaskItemResult[]
  message: string
}

type UniEnvTaskResult = InstallTaskResult | ComboTaskResult
type UiTaskSnapshot = TaskSnapshot<UniEnvTaskResult, ProgressData>

const TASK_STATUS_LABELS: Record<TaskStatus, string> = {
  queued: '等待执行',
  running: '正在执行',
  succeeded: '执行成功',
  failed: '执行失败',
  cancelled: '已取消'
}

function TaskProgressPanel({
  snapshot,
  cancelling,
  onCancel
}: {
  snapshot?: UiTaskSnapshot
  cancelling: boolean
  onCancel(): void
}) {
  if (!snapshot) return null
  const active = snapshot.status === 'queued' || snapshot.status === 'running'
  const percent = Math.max(0, Math.min(100, snapshot.progress?.percent ?? 0))
  const statusColor =
    snapshot.status === 'succeeded'
      ? COLORS.success
      : snapshot.status === 'failed'
        ? COLORS.danger
        : snapshot.status === 'cancelled'
          ? COLORS.warning
          : COLORS.primary
  const message =
    snapshot.progress?.message || snapshot.error?.message || TASK_STATUS_LABELS[snapshot.status]

  return (
    <div
      style={{
        background: COLORS.bgWhite,
        border: `1px solid ${COLORS.border}`,
        borderRadius: 8,
        padding: 12,
        marginBottom: 16
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 12,
          marginBottom: snapshot.progress ? 8 : 0
        }}
      >
        <span style={{ color: statusColor, fontWeight: 600 }}>
          {TASK_STATUS_LABELS[snapshot.status]}
        </span>
        {active && (
          <button
            type="button"
            disabled={cancelling}
            onClick={onCancel}
            style={{
              border: '1px solid var(--ob-color-error-border, #ffccc7)',
              borderRadius: 5,
              background: COLORS.bgWhite,
              color: COLORS.danger,
              cursor: cancelling ? 'not-allowed' : 'pointer',
              padding: '4px 10px',
              opacity: cancelling ? 0.6 : 1
            }}
          >
            {cancelling ? '正在取消…' : '取消任务'}
          </button>
        )}
      </div>
      {snapshot.progress && (
        <>
          <div
            style={{
              height: 8,
              background: COLORS.borderLight,
              borderRadius: 4,
              overflow: 'hidden',
              marginBottom: 6
            }}
          >
            <div
              style={{
                height: '100%',
                width: `${percent}%`,
                background: statusColor,
                borderRadius: 4,
                transition: 'width 0.3s'
              }}
            />
          </div>
          <span style={{ fontSize: FONT.sizeSm, color: COLORS.textSecondary }}>{message}</span>
        </>
      )}
      {!snapshot.progress && snapshot.error && (
        <div style={{ fontSize: FONT.sizeSm, color: COLORS.textSecondary }}>{message}</div>
      )}
    </div>
  )
}

// ============================================================
// 插件渲染入口
// ============================================================

export default function UniEnvUI({
  api
}: {
  config: Record<string, unknown>
  onConfigChange: (config: Record<string, unknown>) => void
  api: {
    sendToBackend(message: unknown): Promise<unknown>
    notify(title: string, body?: string): void
    confirm(options: {
      title: string
      message: string
      confirmLabel?: string
      cancelLabel?: string
    }): Promise<boolean>
    onBackendMessage(handler: (msg: unknown) => void): () => void
  }
}) {
  const [tools, setTools] = useState<ToolItem[]>([])
  const [combos, setCombos] = useState<ComboPack[]>([])
  const [activeKey, setActiveKey] = useState<string>('python')
  const [toolStatus, setToolStatus] = useState<Record<string, ToolInfo>>({})
  const [versions, setVersions] = useState<Record<string, string[]>>({})
  const [initializing, setInitializing] = useState(false)
  const [comboLoading, setComboLoading] = useState(false)
  const [operationLoading, setOperationLoading] = useState<Record<string, boolean>>({})
  const [taskSnapshots, setTaskSnapshots] = useState<Record<string, UiTaskSnapshot>>({})
  const [cancellingTaskIds, setCancellingTaskIds] = useState<Record<string, boolean>>({})
  const [selectedVersions, setSelectedVersions] = useState<Record<string, string | undefined>>({})
  const [toasts, setToasts] = useState<{ id: number; type: string; content: string }[]>([])
  const initialized = useRef(false)
  const mounted = useRef(false)
  const pollControllers = useRef(new Map<string, AbortController>())
  const toastId = useRef(0)
  const toastTimeouts = useRef(new Map<number, ReturnType<typeof setTimeout>>())

  useEffect(() => {
    const controllers = pollControllers.current
    const timeouts = toastTimeouts.current
    mounted.current = true
    return () => {
      mounted.current = false
      for (const controller of controllers.values()) controller.abort()
      controllers.clear()
      for (const timeout of timeouts.values()) clearTimeout(timeout)
      timeouts.clear()
    }
  }, [])

  const toast = useCallback((type: string, content: string) => {
    if (!mounted.current) return
    const id = ++toastId.current
    setToasts((prev) => [...prev, { id, type, content }])
    const timeout = setTimeout(() => {
      toastTimeouts.current.delete(id)
      if (mounted.current) setToasts((prev) => prev.filter((t) => t.id !== id))
    }, 3500)
    toastTimeouts.current.set(id, timeout)
  }, [])

  const removeToast = useCallback((id: number) => {
    const timeout = toastTimeouts.current.get(id)
    if (timeout) clearTimeout(timeout)
    toastTimeouts.current.delete(id)
    setToasts((prev) => prev.filter((t) => t.id !== id))
  }, [])

  const send = useCallback(
    async (msg: Record<string, unknown>) => {
      const result = await api.sendToBackend(msg)
      return result as Record<string, unknown>
    },
    [api]
  )

  const monitorTask = useCallback(
    async (uiKey: string, taskId: string): Promise<UiTaskSnapshot> => {
      const controller = new AbortController()
      pollControllers.current.set(taskId, controller)
      try {
        return await pollTask<UniEnvTaskResult, ProgressData>({
          taskId,
          signal: controller.signal,
          fetchTask: (currentTaskId) => send({ type: 'getTask', taskId: currentTaskId }),
          onSnapshot: (snapshot) => {
            if (!controller.signal.aborted && mounted.current) {
              setTaskSnapshots((prev) => ({ ...prev, [uiKey]: snapshot }))
            }
          }
        })
      } finally {
        if (pollControllers.current.get(taskId) === controller) {
          pollControllers.current.delete(taskId)
        }
      }
    },
    [send]
  )

  const cancelTask = useCallback(
    async (taskId: string) => {
      if (cancellingTaskIds[taskId]) return
      setCancellingTaskIds((prev) => ({ ...prev, [taskId]: true }))
      try {
        assertTaskCancellationAccepted(await send({ type: 'cancelTask', taskId }), taskId)
        toast('info', '已提交取消请求')
      } catch (error) {
        if (!mounted.current) return
        setCancellingTaskIds((prev) => {
          const next = { ...prev }
          delete next[taskId]
          return next
        })
        toast('error', `取消失败: ${(error as Error).message}`)
      }
    },
    [cancellingTaskIds, send, toast]
  )

  const detectTool = useCallback(
    async (toolId: string, silent = false) => {
      if (!silent && mounted.current) {
        setOperationLoading((prev) => ({ ...prev, [toolId]: true }))
      }
      try {
        const result = (await send({ type: 'detect', tool: toolId })) as Record<string, unknown>
        const info = result as unknown as ToolInfo
        if (mounted.current) setToolStatus((prev) => ({ ...prev, [toolId]: info }))
        return info
      } finally {
        if (!silent && mounted.current) {
          setOperationLoading((prev) => ({ ...prev, [toolId]: false }))
        }
      }
    },
    [send]
  )

  const loadVersions = useCallback(
    async (toolId: string) => {
      try {
        const result = (await send({ type: 'listVersions', tool: toolId })) as unknown as string[]
        if (!mounted.current) return
        const typedToolId = toolId as ToolId
        const ordered = orderToolVersionsForDisplay(typedToolId, result)
        setVersions((prev) => ({ ...prev, [toolId]: ordered }))
        if (result.length > 0 && !selectedVersions[toolId]) {
          setSelectedVersions((prev) => ({
            ...prev,
            [toolId]: getPreferredToolVersion(typedToolId, result)
          }))
        }
      } catch {
        // ignore
      }
    },
    [send, selectedVersions]
  )

  /** 检查语言新版本（1.9.13）：强制刷新上游元数据并合并进版本列表 */
  const [checkingOnline, setCheckingOnline] = useState(false)
  const checkOnlineVersions = useCallback(
    async (toolId: string) => {
      setCheckingOnline(true)
      try {
        const resp = (await send({
          type: 'checkOnlineVersions',
          tool: toolId
        })) as { results?: { tool: string; ok: boolean; versions?: string[]; error?: string }[] }
        const entry = resp?.results?.find((r) => r.tool === toolId)
        if (!mounted.current) return
        if (!entry?.ok) {
          toast('error', entry?.error || '在线检查失败')
          return
        }
        const typedToolId = toolId as ToolId
        const ordered = orderToolVersionsForDisplay(typedToolId, entry.versions ?? [])
        setVersions((prev) => ({ ...prev, [toolId]: ordered }))
        const onlineCount = (entry.versions ?? []).length
        toast(
          onlineCount > 0 ? 'success' : 'info',
          onlineCount > 0 ? `发现 ${onlineCount} 个可用版本（含在线新版本）` : '未发现额外在线版本'
        )
      } catch {
        if (mounted.current) toast('error', '在线检查失败，请检查网络后重试')
      } finally {
        if (mounted.current) setCheckingOnline(false)
      }
    },
    [send, toast]
  )

  const installTool = useCallback(
    async (toolId: string) => {
      const version = selectedVersions[toolId]
      if (!version) {
        toast('warning', '请先选择要安装的版本')
        return
      }
      const lifecycle = getToolVersionLifecycle(toolId as ToolId, version)
      const toolName = tools.find((tool) => tool.id === toolId)?.displayName || toolId
      if (requiresToolVersionConfirmation(toolId as ToolId, version)) {
        const confirmed = await api.confirm({
          title: lifecycle.status === 'eol' ? '确认安装已停止维护的版本' : '确认安装固定旧补丁',
          message: `${toolName} ${version}：${lifecycle.note}\n\nUniEnv 会校验制品摘要，但摘要正确不代表该旧版本仍获得安全更新。仅在兼容旧项目时继续。`,
          confirmLabel: '仍要安装',
          cancelLabel: '取消'
        })
        if (!confirmed) return
      }
      setOperationLoading((prev) => ({ ...prev, [toolId]: true }))
      let taskId: string | undefined
      try {
        const startedTaskId = readStartedTaskId(
          await send({ type: 'install', tool: toolId, version })
        )
        taskId = startedTaskId
        if (!mounted.current) return
        setTaskSnapshots((prev) => ({
          ...prev,
          [toolId]: {
            taskId: startedTaskId,
            resourceKey: 'installation',
            status: 'queued',
            createdAt: Date.now()
          }
        }))
        toast('info', '安装已开始，请稍候...')
        const completed = await monitorTask(toolId, startedTaskId)
        if (!mounted.current) return
        if (completed.status === 'succeeded') {
          const result = completed.result
          toast(
            'success',
            result?.kind === 'install' ? result.message : `${toolId} ${version} 安装完成`
          )
        } else if (completed.status === 'cancelled') {
          toast('warning', `${toolId} 安装已取消`)
        } else {
          toast('error', completed.error?.message || `${toolId} 安装失败`)
        }
        await detectTool(toolId, true)
      } catch (error) {
        if (!isTaskPollingAborted(error) && mounted.current) {
          toast('error', `安装失败: ${(error as Error).message}`)
        }
      } finally {
        if (mounted.current) {
          setOperationLoading((prev) => ({ ...prev, [toolId]: false }))
          if (taskId) {
            const completedTaskId = taskId
            setCancellingTaskIds((prev) => {
              const next = { ...prev }
              delete next[completedTaskId]
              return next
            })
          }
        }
      }
    },
    [selectedVersions, tools, api, send, monitorTask, detectTool, toast]
  )

  const uninstallTool = useCallback(
    async (toolId: string) => {
      const name = tools.find((t) => t.id === toolId)?.displayName || toolId
      const confirmed = await api.confirm({
        title: '确认卸载工具',
        message: `确定要卸载 ${name} 吗？此操作会移除由 UniEnv 管理的该工具版本。`,
        confirmLabel: '卸载',
        cancelLabel: '取消'
      })
      if (!confirmed) return
      setOperationLoading((prev) => ({ ...prev, [toolId]: true }))
      try {
        const result = (await send({ type: 'uninstall', tool: toolId })) as Record<string, unknown>
        if (result.error) {
          toast('error', result.error as string)
        } else {
          toast('success', `${name} 已卸载`)
          await detectTool(toolId, true)
        }
      } catch (err) {
        toast('error', `卸载失败: ${(err as Error).message}`)
      } finally {
        setOperationLoading((prev) => ({ ...prev, [toolId]: false }))
      }
    },
    [api, tools, send, detectTool, toast]
  )

  const switchTool = useCallback(
    async (toolId: string) => {
      const version = selectedVersions[toolId]
      if (!version) {
        toast('warning', '请先选择目标版本')
        return
      }
      setOperationLoading((prev) => ({ ...prev, [toolId]: true }))
      try {
        const result = (await send({ type: 'switchVersion', tool: toolId, version })) as Record<
          string,
          unknown
        >
        if (result.error) {
          toast('error', result.error as string)
        } else {
          toast('success', (result.message as string) || `已切换到 ${version}`)
          await detectTool(toolId, true)
        }
      } catch (err) {
        toast('error', `切换失败: ${(err as Error).message}`)
      } finally {
        setOperationLoading((prev) => ({ ...prev, [toolId]: false }))
      }
    },
    [selectedVersions, send, detectTool, toast]
  )

  const detectAll = useCallback(async () => {
    for (const tool of tools) {
      await detectTool(tool.id, true)
    }
  }, [tools, detectTool])

  const installCombo = useCallback(
    async (comboId: string) => {
      const combo = combos.find((c) => c.id === comboId)
      const name = combo?.name || comboId
      const comboItems = (combo?.items || []) as Array<{ toolId: ToolId; version: string }>
      const lifecycleSummary = formatComboLifecycleSummary(comboItems)
      const hasLegacyVersion = requiresComboVersionConfirmation(comboItems)
      const confirmed = await api.confirm({
        title: '确认安装组合包',
        message: `确定要一键安装组合包“${name}”中的全部工具吗？安装过程可能需要较长时间。${lifecycleSummary ? `\n\n版本维护状态（截至 ${TOOL_VERSION_LIFECYCLE_AS_OF}）：\n${lifecycleSummary}${hasLegacyVersion ? '\n\n组合中包含固定旧版本，可能不含最新安全修复；仅在兼容旧项目时继续。' : '\n\n组合中的制品均为当前维护版本。'}` : ''}`,
        confirmLabel: hasLegacyVersion ? '仍要安装' : '开始安装',
        cancelLabel: '取消'
      })
      if (!confirmed) return
      setComboLoading(true)
      const uiKey = `combo:${comboId}`
      let taskId: string | undefined
      try {
        const startedTaskId = readStartedTaskId(await send({ type: 'installCombo', comboId }))
        taskId = startedTaskId
        if (!mounted.current) return
        setTaskSnapshots((prev) => ({
          ...prev,
          [uiKey]: {
            taskId: startedTaskId,
            resourceKey: 'installation',
            status: 'queued',
            createdAt: Date.now()
          }
        }))
        toast('info', `组合包“${name}”安装已开始`)
        const completed = await monitorTask(uiKey, startedTaskId)
        if (!mounted.current) return
        if (completed.status === 'succeeded') {
          const result = completed.result
          if (result?.kind === 'combo') {
            for (const item of result.results) {
              toast(item.success ? 'success' : 'error', item.message)
            }
            toast(result.success ? 'success' : 'warning', result.message)
          } else {
            toast('success', `组合包“${name}”安装完成`)
          }
        } else if (completed.status === 'cancelled') {
          toast('warning', `组合包“${name}”安装已取消`)
        } else {
          toast('error', completed.error?.message || `组合包“${name}”安装失败`)
        }
        await detectAll()
      } catch (error) {
        if (!isTaskPollingAborted(error) && mounted.current) {
          toast('error', `安装失败: ${(error as Error).message}`)
        }
      } finally {
        if (mounted.current) {
          setComboLoading(false)
          if (taskId) {
            const completedTaskId = taskId
            setCancellingTaskIds((prev) => {
              const next = { ...prev }
              delete next[completedTaskId]
              return next
            })
          }
        }
      }
    },
    [api, combos, send, monitorTask, toast, detectAll]
  )

  // ---- 初始化 ----
  useEffect(() => {
    if (initialized.current) return
    initialized.current = true
    const init = async () => {
      setInitializing(true)
      try {
        const toolList = (await send({ type: 'listTools' })) as unknown as ToolItem[]
        if (mounted.current) setTools(toolList)
        const comboList = (await send({ type: 'listCombos' })) as unknown as ComboPack[]
        if (mounted.current) setCombos(comboList || [])
        await Promise.all(toolList.map((t) => detectTool(t.id, true)))
        await Promise.all(toolList.map((t) => loadVersions(t.id)))
      } catch {
        // ignore
      } finally {
        if (mounted.current) setInitializing(false)
      }
    }
    void init()
  }, [send, detectTool, loadVersions, api])

  // ---- 派生数据 ----
  const isComboActive = activeKey.startsWith('combo:')
  const activeComboId = isComboActive ? activeKey.replace('combo:', '') : ''
  const activeTool = tools.find((t) => t.id === activeKey)
  const activeCombo = combos.find((c) => c.id === activeComboId)
  const activeStatus = toolStatus[activeKey]
  const activeTaskSnapshot = taskSnapshots[activeKey]
  const isToolLoading = operationLoading[activeKey] || false
  const activeVersions = versions[activeKey] || []

  /** 该工具是否有在线版本源（node/go/java，与宿主 provider_supports 对齐） */
  const isProviderTool = (toolId: string) => ['node', 'go', 'java'].includes(toolId)
  const selectedVersion = selectedVersions[activeKey]
  const selectedLifecycle =
    activeTool && selectedVersion
      ? getToolVersionLifecycle(activeTool.id as ToolId, selectedVersion)
      : undefined

  // ============================================================
  // 渲染
  // ============================================================

  const menuItemStyle = (key: string): React.CSSProperties => ({
    padding: '10px 16px',
    cursor: 'pointer',
    borderRadius: 6,
    margin: '2px 8px',
    fontSize: FONT.sizeLg,
    display: 'flex',
    alignItems: 'center',
    gap: 8,
    background: activeKey === key ? COLORS.primaryLight : 'transparent',
    color: activeKey === key ? COLORS.primary : COLORS.text,
    fontWeight: activeKey === key ? 600 : 400
  })

  const btnPrimary: React.CSSProperties = {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
    padding: '8px 20px',
    background: COLORS.primary,
    color: '#fff',
    border: 'none',
    borderRadius: 6,
    cursor: 'pointer',
    fontSize: FONT.sizeLg,
    fontWeight: 500,
    width: '100%',
    height: 38,
    boxShadow: '0 2px 0 rgba(5,145,255,0.06)'
  }

  const btnDefault: React.CSSProperties = {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
    padding: '8px 20px',
    background: COLORS.bgWhite,
    color: COLORS.text,
    border: `1px solid ${COLORS.border}`,
    borderRadius: 6,
    cursor: 'pointer',
    fontSize: FONT.sizeLg,
    width: '100%',
    height: 38
  }

  const btnDanger: React.CSSProperties = {
    ...btnDefault,
    border: '1px solid var(--ob-color-error-border, #ffccc7)',
    color: COLORS.danger
  }

  const tagStyle: React.CSSProperties = {
    display: 'inline-flex',
    alignItems: 'center',
    gap: 4,
    padding: '2px 8px',
    borderRadius: 4,
    fontSize: FONT.sizeSm,
    fontWeight: 500
  }

  const cardStyle: React.CSSProperties = {
    background: COLORS.bgWhite,
    border: `1px solid ${COLORS.border}`,
    borderRadius: 8,
    padding: 16,
    marginBottom: 12
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
      {/* ========== Toast ========== */}
      <Toast items={toasts} onRemove={removeToast} />

      {/* ========== 主体三栏 ========== */}
      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        {/* ====== 左侧：工具列表 ====== */}
        <div
          style={{
            width: 200,
            minWidth: 200,
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
              color: COLORS.text,
              display: 'flex',
              alignItems: 'center',
              gap: 8
            }}
          >
            <span style={{ fontSize: 18 }}>&#9776;</span>
            工具与组合包
          </div>

          {/* 工具菜单 */}
          <div style={{ paddingBottom: 4 }}>
            <div
              style={{
                padding: '8px 16px 4px',
                fontSize: FONT.sizeSm,
                color: COLORS.textSecondary,
                fontWeight: 500
              }}
            >
              开发工具
            </div>
            {tools.map((tool) => {
              const s = toolStatus[tool.id]
              return (
                <div
                  key={tool.id}
                  style={menuItemStyle(tool.id)}
                  onClick={() => setActiveKey(tool.id)}
                >
                  <span style={{ fontSize: 16 }}>{tool.icon}</span>
                  <span style={{ flex: 1 }}>{tool.displayName}</span>
                  <span
                    style={{
                      width: 8,
                      height: 8,
                      borderRadius: '50%',
                      background: s?.installed ? COLORS.success : COLORS.textTertiary,
                      display: 'inline-block',
                      flexShrink: 0
                    }}
                  />
                </div>
              )
            })}
          </div>

          {/* 组合包菜单 */}
          {combos.length > 0 && (
            <div style={{ paddingBottom: 4 }}>
              <div
                style={{
                  padding: '12px 16px 4px',
                  fontSize: FONT.sizeSm,
                  color: COLORS.textSecondary,
                  fontWeight: 500
                }}
              >
                组合包
              </div>
              {combos.map((combo) => {
                const key = `combo:${combo.id}`
                return (
                  <div key={key} style={menuItemStyle(key)} onClick={() => setActiveKey(key)}>
                    <span style={{ fontSize: 16 }}>&#9889;</span>
                    <span style={{ flex: 1 }}>{combo.name}</span>
                  </div>
                )
              })}
            </div>
          )}
        </div>

        {/* ====== 中间：状态看板 ====== */}
        <div
          style={{
            flex: 1,
            padding: 20,
            overflowY: 'auto',
            background: COLORS.bgGray
          }}
        >
          {initializing && Object.keys(toolStatus).length === 0 ? (
            <Spinner tip="初始化中..." />
          ) : isComboActive && activeCombo ? (
            /* ---- 组合包详情 ---- */
            <div>
              <div style={{ marginBottom: 20 }}>
                <h2
                  style={{
                    margin: '0 0 4px',
                    fontSize: FONT.sizeTitle,
                    fontWeight: 600,
                    color: COLORS.text
                  }}
                >
                  {activeCombo.name}
                </h2>
                <p style={{ margin: 0, color: COLORS.textSecondary, fontSize: FONT.sizeLg }}>
                  {activeCombo.description}
                </p>
              </div>

              <div style={{ ...cardStyle, borderTop: `3px solid ${COLORS.warning}` }}>
                <div style={{ fontWeight: 600, fontSize: FONT.sizeLg, marginBottom: 12 }}>
                  包含以下工具：
                </div>
                {activeCombo.items.map((item) => {
                  const t = tools.find((tt) => tt.id === item.toolId)
                  const s = toolStatus[item.toolId]
                  return (
                    <div
                      key={item.toolId}
                      style={{
                        padding: '10px 12px',
                        border: `1px solid ${COLORS.border}`,
                        borderRadius: 6,
                        marginBottom: 6,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 10
                      }}
                    >
                      <span style={{ fontSize: 18 }}>{t?.icon || '📦'}</span>
                      <span style={{ fontWeight: 500 }}>{t?.displayName || item.toolId}</span>
                      <span
                        style={{
                          ...tagStyle,
                          background: COLORS.primaryLight,
                          color: COLORS.primary
                        }}
                      >
                        {item.version}
                      </span>
                      {s?.installed ? (
                        <span
                          style={{
                            ...tagStyle,
                            background: COLORS.successBg,
                            color: COLORS.success
                          }}
                        >
                          ✓ 已安装 {s.version}
                        </span>
                      ) : (
                        <span
                          style={{
                            ...tagStyle,
                            background: COLORS.bgGrayDark,
                            color: COLORS.textSecondary
                          }}
                        >
                          ✕ 未安装
                        </span>
                      )}
                    </div>
                  )
                })}
              </div>

              <TaskProgressPanel
                snapshot={activeTaskSnapshot}
                cancelling={Boolean(
                  activeTaskSnapshot && cancellingTaskIds[activeTaskSnapshot.taskId]
                )}
                onCancel={() => {
                  if (activeTaskSnapshot) void cancelTask(activeTaskSnapshot.taskId)
                }}
              />

              <button
                style={{
                  ...btnPrimary,
                  width: '100%',
                  height: 44,
                  fontSize: FONT.sizeXl,
                  marginTop: 16,
                  opacity: comboLoading ? 0.7 : 1
                }}
                disabled={comboLoading}
                onClick={() => installCombo(activeComboId)}
              >
                {comboLoading ? <Spinner size={16} /> : <span>&#9889;</span>}
                一键安装全部
              </button>
              <p
                style={{
                  textAlign: 'center',
                  color: COLORS.textSecondary,
                  fontSize: FONT.sizeSm,
                  marginTop: 8
                }}
              >
                点击后将依次下载并安装组合包中的所有工具
              </p>
            </div>
          ) : activeTool ? (
            /* ---- 工具详情 ---- */
            <div>
              <div style={{ marginBottom: 20 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 6 }}>
                  <span style={{ fontSize: 32 }}>{activeTool.icon}</span>
                  <h2
                    style={{
                      margin: 0,
                      fontSize: FONT.sizeTitle,
                      fontWeight: 600,
                      color: COLORS.text
                    }}
                  >
                    {activeTool.displayName}
                  </h2>
                  {activeStatus?.installed ? (
                    <span
                      style={{
                        ...tagStyle,
                        background: COLORS.successBg,
                        color: COLORS.success,
                        fontSize: FONT.sizeMd
                      }}
                    >
                      ✓ 已安装
                    </span>
                  ) : (
                    <span
                      style={{
                        ...tagStyle,
                        background: COLORS.bgGrayDark,
                        color: COLORS.textSecondary,
                        fontSize: FONT.sizeMd
                      }}
                    >
                      ✕ 未安装
                    </span>
                  )}
                </div>
                <p style={{ margin: 0, color: COLORS.textSecondary, fontSize: FONT.sizeLg }}>
                  {activeTool.description}
                </p>
              </div>

              {/* 状态详情 */}
              {activeStatus?.installed ? (
                <div
                  style={{
                    ...cardStyle,
                    background: COLORS.successBg,
                    border: `1px solid ${COLORS.successBorder}`
                  }}
                >
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                    <div>
                      <span style={{ fontWeight: 600 }}>当前版本：</span>
                      <span style={{ ...tagStyle, background: COLORS.success, color: '#fff' }}>
                        {activeStatus.version}
                      </span>
                    </div>
                    {activeStatus.path ? (
                      <div>
                        <span style={{ fontWeight: 600 }}>安装路径：</span>
                        <code
                          style={{
                            background: COLORS.bgWhite,
                            padding: '2px 6px',
                            borderRadius: 4,
                            fontSize: FONT.sizeSm
                          }}
                        >
                          {activeStatus.path}
                        </code>
                      </div>
                    ) : null}
                  </div>
                </div>
              ) : (
                <div
                  style={{
                    ...cardStyle,
                    background: COLORS.warningBg,
                    border: `1px solid ${COLORS.warningBorder}`
                  }}
                >
                  <span style={{ color: COLORS.warning, marginRight: 8 }}>&#9888;</span>
                  尚未安装 {activeTool.displayName}，请选择版本并点击安装
                </div>
              )}

              <TaskProgressPanel
                snapshot={activeTaskSnapshot}
                cancelling={Boolean(
                  activeTaskSnapshot && cancellingTaskIds[activeTaskSnapshot.taskId]
                )}
                onCancel={() => {
                  if (activeTaskSnapshot) void cancelTask(activeTaskSnapshot.taskId)
                }}
              />

              {/* 已安装版本 */}
              {activeStatus?.installed && (
                <div style={{ ...cardStyle }}>
                  <div style={{ fontWeight: 600, fontSize: FONT.sizeLg, marginBottom: 8 }}>
                    已安装版本
                  </div>
                  <span style={{ ...tagStyle, background: COLORS.success, color: '#fff' }}>
                    {activeStatus.version} (当前)
                  </span>
                  <p style={{ fontSize: FONT.sizeSm, color: COLORS.textSecondary, marginTop: 8 }}>
                    切换版本需先安装其他版本
                  </p>
                </div>
              )}
            </div>
          ) : (
            <div style={{ textAlign: 'center', padding: 60, color: COLORS.textSecondary }}>
              选择左侧工具或组合包查看详情
            </div>
          )}
        </div>

        {/* ====== 右侧：操作区 ====== */}
        {!isComboActive && activeTool && (
          <div
            style={{
              width: 240,
              minWidth: 240,
              background: COLORS.bgWhite,
              borderLeft: `1px solid ${COLORS.border}`,
              padding: 20,
              overflowY: 'auto',
              flexShrink: 0
            }}
          >
            <h3
              style={{
                margin: '0 0 16px',
                fontSize: FONT.sizeXl,
                fontWeight: 600,
                display: 'flex',
                alignItems: 'center',
                gap: 8
              }}
            >
              <span style={{ fontSize: 16 }}>{'</>'}</span>
              操作
            </h3>

            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              {/* 检测 */}
              <button
                style={{ ...btnDefault, justifyContent: 'center' }}
                disabled={isToolLoading}
                onClick={() => detectTool(activeKey)}
              >
                {isToolLoading ? <Spinner size={14} /> : <span>&#128269; 检测安装状态</span>}
              </button>

              <hr
                style={{ border: 'none', borderTop: `1px solid ${COLORS.border}`, margin: '4px 0' }}
              />

              {/* 版本选择 */}
              <div>
                <label
                  style={{
                    display: 'block',
                    fontWeight: 600,
                    marginBottom: 6,
                    fontSize: FONT.sizeMd
                  }}
                >
                  选择版本：
                </label>
                <select
                  style={{
                    width: '100%',
                    height: 36,
                    padding: '0 10px',
                    border: `1px solid ${COLORS.border}`,
                    borderRadius: 6,
                    fontSize: FONT.sizeLg,
                    background: COLORS.bgWhite,
                    color: COLORS.text,
                    cursor: 'pointer'
                  }}
                  value={selectedVersions[activeKey] || ''}
                  onChange={(e) =>
                    setSelectedVersions((prev) => ({
                      ...prev,
                      [activeKey]: e.target.value || undefined
                    }))
                  }
                >
                  <option value="" disabled>
                    请选择版本
                  </option>
                  {activeVersions.map((v) => (
                    <option key={v} value={v}>
                      {formatToolVersionOption(activeKey as ToolId, v)}
                    </option>
                  ))}
                </select>
                {isProviderTool(activeKey) && (
                  <button
                    type="button"
                    disabled={checkingOnline}
                    onClick={() => void checkOnlineVersions(activeKey)}
                    style={{
                      marginTop: 8,
                      width: '100%',
                      padding: '6px 10px',
                      border: `1px solid ${COLORS.border}`,
                      borderRadius: 6,
                      background: COLORS.bgWhite,
                      color: COLORS.textSecondary,
                      fontSize: FONT.sizeSm,
                      cursor: checkingOnline ? 'wait' : 'pointer'
                    }}
                  >
                    {checkingOnline ? '正在检查在线新版本…' : '🔍 检查语言新版本（联网）'}
                  </button>
                )}
                {selectedLifecycle && (
                  <div
                    style={{
                      marginTop: 8,
                      padding: '8px 10px',
                      borderRadius: 6,
                      border: `1px solid ${
                        selectedLifecycle.status === 'current'
                          ? COLORS.successBorder
                          : selectedLifecycle.status === 'eol'
                            ? 'var(--ob-color-error-border, #ffccc7)'
                            : COLORS.warningBorder
                      }`,
                      background:
                        selectedLifecycle.status === 'current'
                          ? COLORS.successBg
                          : selectedLifecycle.status === 'eol'
                            ? COLORS.dangerBg
                            : COLORS.warningBg,
                      color:
                        selectedLifecycle.status === 'current'
                          ? COLORS.success
                          : selectedLifecycle.status === 'eol'
                            ? COLORS.danger
                            : COLORS.textSecondary,
                      fontSize: FONT.sizeXs,
                      lineHeight: 1.5
                    }}
                  >
                    <strong>{selectedLifecycle.label}</strong>
                    <div>{selectedLifecycle.note}</div>
                    <div>状态依据官方生命周期，更新于 {TOOL_VERSION_LIFECYCLE_AS_OF}。</div>
                  </div>
                )}
              </div>

              {/* 安装 */}
              <button
                style={{
                  ...btnPrimary,
                  justifyContent: 'center',
                  height: 42,
                  opacity: isToolLoading ? 0.7 : 1
                }}
                disabled={isToolLoading}
                onClick={() => installTool(activeKey)}
              >
                {isToolLoading ? <Spinner size={14} /> : <span>&#128229; 安装</span>}
              </button>

              {/* 切换版本 */}
              <button
                style={{
                  ...btnDefault,
                  justifyContent: 'center',
                  opacity: !activeStatus?.installed || isToolLoading ? 0.5 : 1,
                  cursor: !activeStatus?.installed || isToolLoading ? 'not-allowed' : 'pointer'
                }}
                disabled={!activeStatus?.installed || isToolLoading}
                onClick={() => switchTool(activeKey)}
              >
                &#128260; 切换版本
              </button>

              {/* 卸载 */}
              <button
                style={{
                  ...btnDanger,
                  justifyContent: 'center',
                  opacity: !activeStatus?.installed || isToolLoading ? 0.5 : 1,
                  cursor: !activeStatus?.installed || isToolLoading ? 'not-allowed' : 'pointer'
                }}
                disabled={!activeStatus?.installed || isToolLoading}
                onClick={() => uninstallTool(activeKey)}
              >
                &#128465; 卸载
              </button>
            </div>

            {/* 操作提示 */}
            <div
              style={{
                marginTop: 24,
                padding: 12,
                background: COLORS.bgGray,
                borderRadius: 8,
                fontSize: FONT.sizeXs,
                color: COLORS.textSecondary,
                lineHeight: 1.6
              }}
            >
              提示：安装过程可能需要几分钟，请耐心等待。安装目录位于配置中指定的根目录下。
            </div>
          </div>
        )}
      </div>

      {/* ========== 底部：组合包快捷操作 ========== */}
      {combos.length > 0 && (
        <div
          style={{
            background: COLORS.bgWhite,
            borderTop: `1px solid ${COLORS.border}`,
            padding: '8px 20px',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            flexShrink: 0
          }}
        >
          <span style={{ color: COLORS.warning, fontSize: 16 }}>&#9889;</span>
          <span
            style={{ fontWeight: 600, fontSize: FONT.sizeSm, marginRight: 8, whiteSpace: 'nowrap' }}
          >
            一键安装组合包：
          </span>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            {combos.slice(0, 5).map((combo) => (
              <button
                key={combo.id}
                style={{
                  padding: '4px 10px',
                  border: `1px dashed ${COLORS.border}`,
                  borderRadius: 4,
                  background: COLORS.bgWhite,
                  cursor: 'pointer',
                  fontSize: FONT.sizeSm,
                  display: 'flex',
                  alignItems: 'center',
                  gap: 4,
                  opacity: comboLoading ? 0.6 : 1
                }}
                disabled={comboLoading}
                onClick={() => installCombo(combo.id)}
              >
                <span style={{ fontSize: 13 }}>&#9889;</span>
                {combo.name}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

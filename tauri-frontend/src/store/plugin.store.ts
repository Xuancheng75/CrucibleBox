import { create } from 'zustand'
import type { PluginMeta, PluginConfig } from '../../../shared/types/plugin.types'
import { PluginLifecycleStatus } from '../../../shared/types/plugin.types'
import { tauriApi, type PluginInstallPreviewResponse } from '../api/tauriApi'
import { useTaskStore } from './task.store'

export interface PendingInstall {
  source: 'zip' | 'directory'
  path: string
}

export interface InstallQueueState {
  /** 待预览的安装队列（全局拖拽/批量导入写入，逐个消费） */
  installQueue: PendingInstall[]
  /** 队列驱动中（正在为队首执行 installPreview） */
  queueProcessing: boolean
  /** 批量会话汇总（enqueue 时清零；队列耗尽且无待确认预览时由 UI 呈现） */
  batchTotal: number
  batchSucceeded: number
  batchSkipped: number
  batchFailures: string[]

  enqueueInstalls: (items: PendingInstall[]) => void
  processNextInQueue: () => Promise<void>
  clearInstallQueue: () => void
}

export interface PluginState {
  plugins: PluginMeta[]
  loading: boolean
  error: null | string

  activePlugins: Record<string, PluginLifecycleStatus>
  /** 插件级生命周期操作状态，防止快速重复点击造成竞态。 */
  pluginOperationBusy: Record<string, boolean>
  batchOperationBusy: boolean
  /** 排序请求单飞，避免快速拖动时旧响应覆盖新顺序。 */
  reorderBusy: boolean
  /** 内部插件卡片拖拽进行中；全局文件拖放层据此忽略内部手势。 */
  internalPluginDragActive: boolean

  /** 待确认的安装预览（installPlugin 成功后由确认弹窗 commit/discard） */
  installPreview: PluginInstallPreviewResponse | null
  /** 当前待确认预览对应的源路径（批量场景展示文件名/失败归因） */
  activeInstallPath: null | string

  fetchPlugins: () => Promise<void>
  installPlugin: (source: 'zip' | 'directory', path: string) => Promise<boolean>
  commitInstall: () => Promise<boolean>
  discardInstall: () => Promise<void>
  uninstallPlugin: (id: string) => Promise<boolean>
  enablePlugin: (id: string) => Promise<boolean>
  disablePlugin: (id: string) => Promise<boolean>
  batchEnablePlugins: (ids: string[]) => Promise<BatchLifecycleResult>
  batchDisablePlugins: (ids: string[]) => Promise<BatchLifecycleResult>
  updatePluginConfig: (id: string, config: PluginConfig) => Promise<boolean>
  reorderPlugins: (orderedIds: string[]) => Promise<boolean>
  setInternalPluginDragActive: (active: boolean) => void
  setPluginStatus: (id: string, status: PluginLifecycleStatus) => void
}

export interface BatchLifecycleFailure {
  id: string
  error: string
}

export interface BatchLifecycleResult {
  succeeded: string[]
  failures: BatchLifecycleFailure[]
}

export type PluginStore = PluginState & InstallQueueState

type PluginStoreSet = (
  partial: Partial<PluginStore> | ((state: PluginStore) => Partial<PluginStore>)
) => void

let pluginsFetchSequence = 0
let activeInstallTaskId: string | null = null

async function runBatchLifecycle(
  get: () => PluginStore,
  set: PluginStoreSet,
  ids: string[],
  enabled: boolean
): Promise<BatchLifecycleResult> {
  if (get().batchOperationBusy || get().reorderBusy) return { succeeded: [], failures: [] }
  const uniqueIds = [...new Set(ids)].filter((id) => get().plugins.some((p) => p.id === id))
  const succeeded: string[] = []
  const failures: BatchLifecycleFailure[] = []
  set({ batchOperationBusy: true, error: null })
  try {
    for (const id of uniqueIds) {
      set((state) => ({
        pluginOperationBusy: { ...state.pluginOperationBusy, [id]: true }
      }))
      try {
        const result = enabled
          ? await tauriApi.plugin.enable(id)
          : await tauriApi.plugin.disable(id)
        if (!result.success) {
          failures.push({ id, error: result.error ?? (enabled ? '启用失败' : '停用失败') })
          continue
        }
        succeeded.push(id)
        set((state) => ({
          plugins: state.plugins.map((plugin) =>
            plugin.id === id ? { ...plugin, enabled } : plugin
          ),
          activePlugins: {
            ...state.activePlugins,
            [id]: enabled ? PluginLifecycleStatus.Active : PluginLifecycleStatus.Inactive
          }
        }))
      } catch (error) {
        failures.push({ id, error: toErrorMessage(error, enabled ? '启用失败' : '停用失败') })
      } finally {
        set((state) => {
          const pluginOperationBusy = { ...state.pluginOperationBusy }
          delete pluginOperationBusy[id]
          return { pluginOperationBusy }
        })
      }
    }
    await get().fetchPlugins()
    if (failures.length > 0) {
      set({ error: `${enabled ? '批量启用' : '批量停用'}失败 ${failures.length} 项` })
    }
    return { succeeded, failures }
  } finally {
    set({ batchOperationBusy: false })
  }
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  plugins: [],
  loading: false,
  error: null,
  activePlugins: {},
  pluginOperationBusy: {},
  batchOperationBusy: false,
  reorderBusy: false,
  internalPluginDragActive: false,
  installPreview: null,
  activeInstallPath: null,
  installQueue: [],
  queueProcessing: false,
  batchTotal: 0,
  batchSucceeded: 0,
  batchSkipped: 0,
  batchFailures: [],

  fetchPlugins: async () => {
    const requestSequence = ++pluginsFetchSequence
    set({ loading: true, error: null })
    try {
      const plugins = await Promise.race([
        tauriApi.plugin.list(),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error('请求超时，请检查主进程连接')), 5000)
        )
      ])
      if (requestSequence !== pluginsFetchSequence) return
      set({
        plugins,
        activePlugins: Object.fromEntries(
          plugins.filter((p) => p.enabled).map((p) => [p.id, PluginLifecycleStatus.Active])
        ),
        loading: false
      })
    } catch (err) {
      if (requestSequence !== pluginsFetchSequence) return
      set({ error: toErrorMessage(err, '请求超时，请检查主进程连接'), loading: false })
    }
  },

  installPlugin: async (source, path) => {
    if (
      get().batchOperationBusy ||
      get().reorderBusy ||
      Object.keys(get().pluginOperationBusy).length > 0
    )
      return false
    const taskId = `plugin-install-${Date.now()}-${Math.random().toString(16).slice(2)}`
    activeInstallTaskId = taskId
    useTaskStore.getState().upsertTask({
      id: taskId,
      title: '准备安装插件',
      detail: path,
      source: 'host',
      status: 'running',
      progress: 10
    })
    set({ loading: true, error: null })
    try {
      // 后端 preview 要求路径必须来自用户对话框/拖拽登记（trusted_paths 防线）
      await tauriApi.plugin.registerImportPath(path)
      const result = await tauriApi.plugin.installPreview({ type: source, path })
      if (result.success && result.installToken) {
        set({ installPreview: result, activeInstallPath: path, loading: false })
        useTaskStore.getState().patchTask(taskId, {
          title: result.data?.isUpgrade ? '等待确认插件升级' : '等待确认插件安装',
          status: 'queued',
          progress: 35
        })
        return true
      }
      useTaskStore.getState().patchTask(taskId, {
        status: 'failed',
        error: result.error ?? '安装预览失败'
      })
      activeInstallTaskId = null
      set({ error: result.error ?? '安装预览失败', loading: false })
      return false
    } catch (err) {
      useTaskStore.getState().patchTask(taskId, {
        status: 'failed',
        error: toErrorMessage(err, '安装预览失败')
      })
      activeInstallTaskId = null
      set({ error: toErrorMessage(err, '安装预览失败'), loading: false })
      return false
    }
  },

  commitInstall: async () => {
    const preview = get().installPreview
    if (!preview?.installToken) return false
    if (
      get().batchOperationBusy ||
      get().reorderBusy ||
      Object.keys(get().pluginOperationBusy).length > 0
    )
      return false
    set({ loading: true, error: null })
    if (activeInstallTaskId) {
      useTaskStore.getState().patchTask(activeInstallTaskId, {
        title: preview.data?.isUpgrade ? '正在升级插件' : '正在安装插件',
        status: 'running',
        progress: 60
      })
    }
    try {
      const result = await tauriApi.plugin.installCommit(preview.installToken)
      if (result.success) {
        await get().fetchPlugins()
        set((state) => ({
          installPreview: null,
          activeInstallPath: null,
          loading: false,
          batchSucceeded: state.batchSucceeded + 1
        }))
        void get().processNextInQueue()
        if (activeInstallTaskId) {
          useTaskStore.getState().patchTask(activeInstallTaskId, {
            status: 'completed',
            progress: 100
          })
          activeInstallTaskId = null
        }
        return true
      }
      if (activeInstallTaskId) {
        useTaskStore.getState().patchTask(activeInstallTaskId, {
          status: 'failed',
          error: result.error ?? '安装失败'
        })
        activeInstallTaskId = null
      }
      set((state) => ({
        error: result.error ?? '安装失败',
        loading: false,
        batchFailures: [...state.batchFailures, failureNameFrom(get().activeInstallPath)]
      }))
      void get().processNextInQueue()
      return false
    } catch (err) {
      if (activeInstallTaskId) {
        useTaskStore.getState().patchTask(activeInstallTaskId, {
          status: 'failed',
          error: toErrorMessage(err, '安装失败')
        })
        activeInstallTaskId = null
      }
      set((state) => ({
        error: toErrorMessage(err, '安装失败'),
        loading: false,
        batchFailures: [...state.batchFailures, failureNameFrom(get().activeInstallPath)]
      }))
      void get().processNextInQueue()
      return false
    }
  },

  discardInstall: async () => {
    const preview = get().installPreview
    set((state) => ({
      installPreview: null,
      activeInstallPath: null,
      batchSkipped: state.batchSkipped + 1
    }))
    if (preview?.installToken) {
      try {
        await tauriApi.plugin.installDiscard(preview.installToken)
      } catch {
        // 丢弃失败不阻断 UI
      }
    }
    if (activeInstallTaskId) {
      useTaskStore.getState().patchTask(activeInstallTaskId, {
        status: 'cancelled',
        detail: '用户取消了安装确认'
      })
      activeInstallTaskId = null
    }
    void get().processNextInQueue()
  },

  uninstallPlugin: async (id) => {
    if (get().pluginOperationBusy[id] || get().batchOperationBusy || get().reorderBusy) return false
    set((state) => ({
      loading: true,
      error: null,
      pluginOperationBusy: { ...state.pluginOperationBusy, [id]: true }
    }))
    try {
      const result = await tauriApi.plugin.uninstall(id)
      if (result.success) {
        await get().fetchPlugins()
        return true
      }
      set({ error: result.error, loading: false })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '卸载失败'), loading: false })
      return false
    } finally {
      set((state) => {
        const pluginOperationBusy = { ...state.pluginOperationBusy }
        delete pluginOperationBusy[id]
        return { pluginOperationBusy }
      })
    }
  },

  enablePlugin: async (id) => {
    if (get().pluginOperationBusy[id] || get().batchOperationBusy || get().reorderBusy) return false
    set((state) => ({
      error: null,
      pluginOperationBusy: { ...state.pluginOperationBusy, [id]: true }
    }))
    try {
      const result = await tauriApi.plugin.enable(id)
      if (result.success) {
        set((state) => ({
          plugins: state.plugins.map((p) => (p.id === id ? { ...p, enabled: true } : p)),
          activePlugins: { ...state.activePlugins, [id]: PluginLifecycleStatus.Active }
        }))
        return true
      }
      set({ error: result.error })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '启用失败') })
      return false
    } finally {
      set((state) => {
        const pluginOperationBusy = { ...state.pluginOperationBusy }
        delete pluginOperationBusy[id]
        return { pluginOperationBusy }
      })
    }
  },

  disablePlugin: async (id) => {
    if (get().pluginOperationBusy[id] || get().batchOperationBusy || get().reorderBusy) return false
    set((state) => ({
      error: null,
      pluginOperationBusy: { ...state.pluginOperationBusy, [id]: true }
    }))
    try {
      const result = await tauriApi.plugin.disable(id)
      if (result.success) {
        set((state) => ({
          plugins: state.plugins.map((p) => (p.id === id ? { ...p, enabled: false } : p)),
          activePlugins: { ...state.activePlugins, [id]: PluginLifecycleStatus.Inactive }
        }))
        return true
      }
      set({ error: result.error })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '停用失败') })
      return false
    } finally {
      set((state) => {
        const pluginOperationBusy = { ...state.pluginOperationBusy }
        delete pluginOperationBusy[id]
        return { pluginOperationBusy }
      })
    }
  },

  batchEnablePlugins: async (ids) => {
    return runBatchLifecycle(get, set, ids, true)
  },

  batchDisablePlugins: async (ids) => {
    return runBatchLifecycle(get, set, ids, false)
  },

  updatePluginConfig: async (id, config) => {
    try {
      const result = await tauriApi.plugin.updateConfig(id, config)
      if (result.success) {
        return true
      }
      set({ error: result.error })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '保存配置失败') })
      return false
    }
  },

  reorderPlugins: async (orderedIds) => {
    const state = get()
    if (
      state.reorderBusy ||
      state.loading ||
      state.batchOperationBusy ||
      state.installPreview ||
      state.queueProcessing ||
      state.installQueue.length > 0 ||
      Object.keys(state.pluginOperationBusy).length > 0
    ) {
      set({ error: '插件当前正在执行其他操作，请稍后再试' })
      return false
    }
    const currentPlugins = get().plugins
    const nextPlugins = orderedIds
      .map((id) => currentPlugins.find((p) => p.id === id))
      .filter((p): p is PluginMeta => p !== undefined)

    if (nextPlugins.length !== orderedIds.length) {
      set({ error: '排序失败：插件 ID 无效' })
      return false
    }

    set({ plugins: nextPlugins, error: null, reorderBusy: true })
    try {
      const result = await tauriApi.plugin.reorder(orderedIds)
      if (!result.success) {
        throw new Error(result.error ?? '排序失败')
      }
      return true
    } catch (err) {
      set({ plugins: currentPlugins, error: toErrorMessage(err, '排序失败') })
      return false
    } finally {
      set({ reorderBusy: false })
    }
  },

  setInternalPluginDragActive: (active) => {
    set({ internalPluginDragActive: active })
  },

  setPluginStatus: (id, status) => {
    set((state) => ({
      activePlugins: { ...state.activePlugins, [id]: status }
    }))
  },

  // ---------------------------------------------------------------------------
  // 批量安装队列（1.9.12：全局拖拽/批量导入共用；逐个 preview→用户确认→commit）
  // ---------------------------------------------------------------------------

  enqueueInstalls: (items) => {
    if (items.length === 0 || get().reorderBusy) return
    set((state) => ({
      installQueue: [...state.installQueue, ...items],
      batchTotal: state.batchTotal + items.length,
      batchSucceeded: 0,
      batchSkipped: 0,
      batchFailures: []
    }))
    void get().processNextInQueue()
  },

  processNextInQueue: async () => {
    const { installQueue, queueProcessing } = get()
    if (queueProcessing || installQueue.length === 0) return
    if (get().installPreview) return // 待确认的预览阻塞队列
    const [head, ...rest] = installQueue
    set({ queueProcessing: true })
    const ok = await get().installPlugin(head.source, head.path)
    set({ installQueue: rest, queueProcessing: false })
    if (ok) {
      // 预览已就绪，等用户在确认弹窗中 commit/discard 后再驱动下一项
      return
    }
    const failureName = previewFailureName(head.path)
    set((state) => ({ batchFailures: [...state.batchFailures, failureName] }))
    await get().processNextInQueue()
  },

  clearInstallQueue: () => {
    set({
      installQueue: [],
      queueProcessing: false,
      batchTotal: 0,
      batchSucceeded: 0,
      batchSkipped: 0,
      batchFailures: []
    })
  }
}))

function toErrorMessage(err: unknown, fallback: string): string {
  if (err instanceof Error) return err.message
  if (typeof err === 'string' && err.trim()) return err
  return fallback
}

function previewFailureName(path: string | null): string {
  return path?.split(/[\\/]/).pop() || path || '未知包'
}

const failureNameFrom = previewFailureName

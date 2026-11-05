import { create } from 'zustand'
import type { PluginMeta, PluginConfig } from '../../../shared/types/plugin.types'
import { PluginLifecycleStatus } from '../../../shared/types/plugin.types'
import {
  tauriApi,
  type PluginInstallApproval,
  type PluginInstallPreviewResponse
} from '../api/tauriApi'
import { useTaskStore } from './task.store'
import { useAppStore } from './app.store'

export interface PendingInstall {
  source: 'zip' | 'directory'
  path: string
  taskId?: string
  title?: string
  marketplaceSourceId?: number
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
  failedInstalls: PendingInstall[]

  enqueueInstalls: (items: PendingInstall[]) => void
  processNextInQueue: () => Promise<void>
  clearInstallQueue: () => void
}

export interface PluginState {
  plugins: PluginMeta[]
  loading: boolean
  error: null | string
  failedOperation:
    | { kind: 'enable' | 'disable' | 'uninstall'; id: string }
    | { kind: 'batch-enable' | 'batch-disable'; ids: string[] }
    | null
  retryFailedOperation: () => Promise<boolean>

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
  activeInstallRequest: PendingInstall | null

  fetchPlugins: () => Promise<void>
  installPlugin: (
    source: 'zip' | 'directory',
    path: string,
    task?: { id: string; title?: string; marketplaceSourceId?: number }
  ) => Promise<boolean>
  commitInstall: (approval?: PluginInstallApproval) => Promise<boolean>
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
let pluginsFetchPromise: Promise<void> | null = null
let activeInstallTaskId: string | null = null
let activeMarketplaceSourceId: number | undefined
let installRequestSequence = 0
let installQueueGeneration = 0

async function runBatchLifecycle(
  get: () => PluginStore,
  set: PluginStoreSet,
  ids: string[],
  enabled: boolean
): Promise<BatchLifecycleResult> {
  if (
    get().batchOperationBusy ||
    get().reorderBusy ||
    Object.keys(get().pluginOperationBusy).length > 0
  )
    return { succeeded: [], failures: [] }
  const uniqueIds = [...new Set(ids)].filter((id) => get().plugins.some((p) => p.id === id))
  const succeeded: string[] = []
  const failures: BatchLifecycleFailure[] = []
  set({ batchOperationBusy: true, error: null, failedOperation: null })
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
        if (!enabled) useAppStore.getState().closePluginView(id)
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
      set({
        error: `${enabled ? '批量启用' : '批量停用'}失败 ${failures.length} 项`,
        failedOperation: {
          kind: enabled ? 'batch-enable' : 'batch-disable',
          ids: failures.map(({ id }) => id)
        }
      })
    }
    return { succeeded, failures }
  } finally {
    set({ batchOperationBusy: false })
    void get().processNextInQueue()
  }
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  plugins: [],
  loading: false,
  error: null,
  failedOperation: null,
  retryFailedOperation: async () => {
    const operation = get().failedOperation
    if (!operation) return false
    if ('id' in operation) {
      if (operation.kind === 'enable') return get().enablePlugin(operation.id)
      if (operation.kind === 'disable') return get().disablePlugin(operation.id)
      return get().uninstallPlugin(operation.id)
    }
    const result =
      operation.kind === 'batch-enable'
        ? await get().batchEnablePlugins(operation.ids)
        : await get().batchDisablePlugins(operation.ids)
    return result.failures.length === 0 && result.succeeded.length === operation.ids.length
  },
  activePlugins: {},
  pluginOperationBusy: {},
  batchOperationBusy: false,
  reorderBusy: false,
  internalPluginDragActive: false,
  installPreview: null,
  activeInstallPath: null,
  activeInstallRequest: null,
  installQueue: [],
  queueProcessing: false,
  batchTotal: 0,
  batchSucceeded: 0,
  batchSkipped: 0,
  batchFailures: [],
  failedInstalls: [],

  fetchPlugins: async () => {
    if (pluginsFetchPromise) return pluginsFetchPromise
    const requestSequence = ++pluginsFetchSequence
    pluginsFetchPromise = (async () => {
      set({ loading: true, error: null, failedOperation: null })
      try {
        const plugins = await tauriApi.plugin.list()
        if (requestSequence !== pluginsFetchSequence) return
        set((state) => ({
          plugins,
          activePlugins: Object.fromEntries(
            plugins
              .filter((plugin) => plugin.enabled && state.activePlugins[plugin.id])
              .map((plugin) => [plugin.id, state.activePlugins[plugin.id]])
          ),
          loading: false
        }))
      } catch (err) {
        if (requestSequence !== pluginsFetchSequence) return
        set({
          error: toErrorMessage(err, '插件列表加载失败'),
          loading: false,
          failedOperation: null
        })
      }
    })()
    try {
      await pluginsFetchPromise
    } finally {
      pluginsFetchPromise = null
    }
  },

  installPlugin: async (source, path, task) => {
    if (
      get().installPreview ||
      activeInstallTaskId ||
      get().batchOperationBusy ||
      get().reorderBusy ||
      Object.keys(get().pluginOperationBusy).length > 0
    )
      return false
    const requestSequence = ++installRequestSequence
    const taskId = task?.id ?? `plugin-install-${crypto.randomUUID()}`
    const request: PendingInstall = {
      source,
      path,
      taskId,
      title: task?.title,
      marketplaceSourceId: task?.marketplaceSourceId
    }
    activeInstallTaskId = taskId
    activeMarketplaceSourceId = task?.marketplaceSourceId
    if (!get().queueProcessing)
      set({
        batchTotal: 1,
        batchSucceeded: 0,
        batchSkipped: 0,
        batchFailures: [],
        failedInstalls: []
      })
    useTaskStore.getState().upsertTask({
      id: taskId,
      title: task?.title ?? '准备安装插件',
      detail: path,
      source: task?.marketplaceSourceId !== undefined ? 'marketplace' : 'host',
      status: 'running',
      progress: 10
    })
    set({ loading: true, error: null, failedOperation: null })
    try {
      await tauriApi.plugin.registerImportPath(path)
      const result = await tauriApi.plugin.installPreview({ type: source, path })
      if (requestSequence !== installRequestSequence) {
        if (result.installToken) await tauriApi.plugin.installDiscard(result.installToken)
        return false
      }
      if (!result.success || !result.installToken) throw new Error(result.error ?? '安装预览失败')
      set({
        installPreview: result,
        activeInstallPath: path,
        activeInstallRequest: request,
        loading: false
      })
      useTaskStore.getState().patchTask(taskId, {
        title: result.data?.isUpgrade ? '等待确认插件升级' : '等待确认插件安装',
        status: 'waiting-user',
        progress: 35
      })
      return true
    } catch (error) {
      if (requestSequence !== installRequestSequence) return false
      const detail = toErrorMessage(error, '安装预览失败')
      useTaskStore.getState().patchTask(taskId, { status: 'failed', error: detail })
      activeInstallTaskId = null
      activeMarketplaceSourceId = undefined
      set((state) => ({
        loading: false,
        error: detail,
        batchFailures: [...state.batchFailures, `${previewFailureName(path)}：${detail}`],
        failedInstalls: [...state.failedInstalls, request]
      }))
      return false
    }
  },

  commitInstall: async (approval) => {
    const preview = get().installPreview
    if (
      !preview?.installToken ||
      get().loading ||
      get().batchOperationBusy ||
      get().reorderBusy ||
      Object.keys(get().pluginOperationBusy).length > 0
    )
      return false
    const taskId = activeInstallTaskId
    const request = get().activeInstallRequest
    set({ loading: true, error: null, failedOperation: null })
    let installed = false
    try {
      const result = await tauriApi.plugin.installCommit(
        preview.installToken,
        activeMarketplaceSourceId,
        taskId ?? undefined,
        approval
      )
      if (!result.success) throw new Error(result.error ?? '安装失败')
      installed = true
      await get().fetchPlugins()
      set((state) => ({ batchSucceeded: state.batchSucceeded + 1 }))
      return true
    } catch (error) {
      const detail = toErrorMessage(error, '安装失败')
      if (taskId) useTaskStore.getState().patchTask(taskId, { status: 'failed', error: detail })
      set((state) => ({
        error: detail,
        batchFailures: [
          ...state.batchFailures,
          `${previewFailureName(request?.path ?? null)}：${detail}`
        ],
        failedInstalls: request ? [...state.failedInstalls, request] : state.failedInstalls
      }))
      return false
    } finally {
      activeMarketplaceSourceId = undefined
      activeInstallTaskId = null
      set({
        installPreview: null,
        activeInstallPath: null,
        activeInstallRequest: null,
        loading: false
      })
      if (!installed)
        await tauriApi.plugin.installDiscard(preview.installToken).catch((error: unknown) => {
          set({
            error: `${get().error ?? '安装失败'}；清理预览失败：${toErrorMessage(error, '未知错误')}`
          })
        })
      void get().processNextInQueue()
    }
  },

  discardInstall: async () => {
    ++installRequestSequence
    activeMarketplaceSourceId = undefined
    const preview = get().installPreview
    set((state) => ({
      installPreview: null,
      activeInstallPath: null,
      activeInstallRequest: null,
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
      failedOperation: null,
      pluginOperationBusy: { ...state.pluginOperationBusy, [id]: true }
    }))
    try {
      const result = await tauriApi.plugin.uninstall(id)
      if (result.success) {
        useAppStore.getState().closePluginView(id)
        await get().fetchPlugins()
        return true
      }
      set({ error: result.error, loading: false, failedOperation: { kind: 'uninstall', id } })
      return false
    } catch (err) {
      set({
        error: toErrorMessage(err, '卸载失败'),
        loading: false,
        failedOperation: { kind: 'uninstall', id }
      })
      return false
    } finally {
      set((state) => {
        const pluginOperationBusy = { ...state.pluginOperationBusy }
        delete pluginOperationBusy[id]
        return { pluginOperationBusy }
      })
      void get().processNextInQueue()
    }
  },

  enablePlugin: async (id) => {
    if (get().pluginOperationBusy[id] || get().batchOperationBusy || get().reorderBusy) return false
    set((state) => ({
      error: null,
      failedOperation: null,
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
      set({ error: result.error, failedOperation: { kind: 'enable', id } })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '启用失败'), failedOperation: { kind: 'enable', id } })
      return false
    } finally {
      set((state) => {
        const pluginOperationBusy = { ...state.pluginOperationBusy }
        delete pluginOperationBusy[id]
        return { pluginOperationBusy }
      })
      void get().processNextInQueue()
    }
  },

  disablePlugin: async (id) => {
    if (get().pluginOperationBusy[id] || get().batchOperationBusy || get().reorderBusy) return false
    set((state) => ({
      error: null,
      failedOperation: null,
      pluginOperationBusy: { ...state.pluginOperationBusy, [id]: true }
    }))
    try {
      const result = await tauriApi.plugin.disable(id)
      if (result.success) {
        useAppStore.getState().closePluginView(id)
        set((state) => ({
          plugins: state.plugins.map((p) => (p.id === id ? { ...p, enabled: false } : p)),
          activePlugins: { ...state.activePlugins, [id]: PluginLifecycleStatus.Inactive }
        }))
        return true
      }
      set({ error: result.error, failedOperation: { kind: 'disable', id } })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '停用失败'), failedOperation: { kind: 'disable', id } })
      return false
    } finally {
      set((state) => {
        const pluginOperationBusy = { ...state.pluginOperationBusy }
        delete pluginOperationBusy[id]
        return { pluginOperationBusy }
      })
      void get().processNextInQueue()
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
      set({ error: result.error, failedOperation: null })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '保存配置失败'), failedOperation: null })
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
      set({ error: '插件当前正在执行其他操作，请稍后再试', failedOperation: null })
      return false
    }
    const currentPlugins = get().plugins
    const nextPlugins = orderedIds
      .map((id) => currentPlugins.find((p) => p.id === id))
      .filter((p): p is PluginMeta => p !== undefined)

    if (nextPlugins.length !== orderedIds.length) {
      set({ error: '排序失败：插件 ID 无效', failedOperation: null })
      return false
    }

    set({ plugins: nextPlugins, error: null, failedOperation: null, reorderBusy: true })
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
      void get().processNextInQueue()
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
    if (items.length === 0) return
    const continuing =
      get().queueProcessing || get().installPreview !== null || get().installQueue.length > 0
    set((state) => ({
      installQueue: [...state.installQueue, ...items],
      batchTotal: (continuing ? state.batchTotal : 0) + items.length,
      ...(continuing
        ? {}
        : {
            batchSucceeded: 0,
            batchSkipped: 0,
            batchFailures: [],
            failedInstalls: [],
            error: null,
            failedOperation: null
          })
    }))
    void get().processNextInQueue()
  },

  processNextInQueue: async () => {
    const { installQueue, queueProcessing, installPreview } = get()
    if (queueProcessing || installQueue.length === 0 || installPreview || activeInstallTaskId)
      return
    if (
      get().batchOperationBusy ||
      get().reorderBusy ||
      Object.keys(get().pluginOperationBusy).length > 0
    )
      return
    const generation = installQueueGeneration
    const [head, ...rest] = installQueue
    set({ installQueue: rest, queueProcessing: true })
    await get().installPlugin(head.source, head.path, {
      id: head.taskId ?? `plugin-install-${crypto.randomUUID()}`,
      title: head.title,
      marketplaceSourceId: head.marketplaceSourceId
    })
    if (generation !== installQueueGeneration) return
    set({ queueProcessing: false })
    if (!get().installPreview) void get().processNextInQueue()
  },

  clearInstallQueue: () => {
    ++installQueueGeneration
    ++installRequestSequence
    for (const request of get().installQueue) {
      if (request.taskId)
        useTaskStore
          .getState()
          .patchTask(request.taskId, { status: 'cancelled', detail: '用户取消了批量安装' })
    }
    if (!get().installPreview && activeInstallTaskId) {
      useTaskStore
        .getState()
        .patchTask(activeInstallTaskId, { status: 'cancelled', detail: '用户取消了安装预检' })
      activeInstallTaskId = null
      activeMarketplaceSourceId = undefined
    }
    set({
      installQueue: [],
      queueProcessing: false,
      batchTotal: 0,
      batchSucceeded: 0,
      batchSkipped: 0,
      batchFailures: [],
      failedInstalls: [],
      loading: false
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

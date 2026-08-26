import { create } from 'zustand'
import type { PluginMeta, PluginConfig } from '../../../shared/types/plugin.types'
import { PluginLifecycleStatus } from '../../../shared/types/plugin.types'
import { tauriApi, type PluginInstallPreviewResponse } from '../api/tauriApi'

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
  updatePluginConfig: (id: string, config: PluginConfig) => Promise<boolean>
  reorderPlugins: (orderedIds: string[]) => Promise<boolean>
  setPluginStatus: (id: string, status: PluginLifecycleStatus) => void
}

export type PluginStore = PluginState & InstallQueueState

export const usePluginStore = create<PluginStore>((set, get) => ({
  plugins: [],
  loading: false,
  error: null,
  activePlugins: {},
  installPreview: null,
  activeInstallPath: null,
  installQueue: [],
  queueProcessing: false,
  batchTotal: 0,
  batchSucceeded: 0,
  batchSkipped: 0,
  batchFailures: [],

  fetchPlugins: async () => {
    set({ loading: true, error: null })
    try {
      const plugins = await Promise.race([
        tauriApi.plugin.list(),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error('请求超时，请检查主进程连接')), 5000)
        )
      ])
      set({
        plugins,
        activePlugins: Object.fromEntries(
          plugins.filter((p) => p.enabled).map((p) => [p.id, PluginLifecycleStatus.Active])
        ),
        loading: false
      })
    } catch (err) {
      set({ error: toErrorMessage(err, '请求超时，请检查主进程连接'), loading: false })
    }
  },

  installPlugin: async (source, path) => {
    set({ loading: true, error: null })
    try {
      // 后端 preview 要求路径必须来自用户对话框/拖拽登记（trusted_paths 防线）
      await tauriApi.plugin.registerImportPath(path)
      const result = await tauriApi.plugin.installPreview({ type: source, path })
      if (result.success && result.installToken) {
        set({ installPreview: result, activeInstallPath: path, loading: false })
        return true
      }
      set({ error: result.error ?? '安装预览失败', loading: false })
      return false
    } catch (err) {
      set({ error: toErrorMessage(err, '安装预览失败'), loading: false })
      return false
    }
  },

  commitInstall: async () => {
    const preview = get().installPreview
    if (!preview?.installToken) return false
    set({ loading: true, error: null })
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
        return true
      }
      set((state) => ({
        error: result.error ?? '安装失败',
        loading: false,
        batchFailures: [...state.batchFailures, failureNameFrom(get().activeInstallPath)]
      }))
      void get().processNextInQueue()
      return false
    } catch (err) {
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
    void get().processNextInQueue()
  },

  uninstallPlugin: async (id) => {
    set({ loading: true, error: null })
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
    }
  },

  enablePlugin: async (id) => {
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
    }
  },

  disablePlugin: async (id) => {
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
    }
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
    const currentPlugins = get().plugins
    const nextPlugins = orderedIds
      .map((id) => currentPlugins.find((p) => p.id === id))
      .filter((p): p is PluginMeta => p !== undefined)

    if (nextPlugins.length !== orderedIds.length) {
      set({ error: '排序失败：插件 ID 无效' })
      return false
    }

    set({ plugins: nextPlugins, error: null })
    try {
      const result = await tauriApi.plugin.reorder(orderedIds)
      if (!result.success) {
        throw new Error(result.error ?? '排序失败')
      }
      return true
    } catch (err) {
      set({ plugins: currentPlugins, error: toErrorMessage(err, '排序失败') })
      return false
    }
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
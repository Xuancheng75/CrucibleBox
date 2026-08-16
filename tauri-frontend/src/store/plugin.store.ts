import { create } from 'zustand'
import type { PluginMeta, PluginConfig } from '../../../shared/types/plugin.types'
import { PluginLifecycleStatus } from '../../../shared/types/plugin.types'
import { tauriApi, type PluginInstallPreviewResponse } from '../api/tauriApi'

export interface PluginState {
  plugins: PluginMeta[]
  loading: boolean
  error: string | null

  activePlugins: Record<string, PluginLifecycleStatus>

  /** 待确认的安装预览（installPlugin 成功后由 PluginImport 弹窗确认 commit/discard） */
  installPreview: PluginInstallPreviewResponse | null

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

export const usePluginStore = create<PluginState>((set, get) => ({
  plugins: [],
  loading: false,
  error: null,
  activePlugins: {},
  installPreview: null,

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
      set({ error: (err as Error).message, loading: false })
    }
  },

  installPlugin: async (source, path) => {
    set({ loading: true, error: null })
    try {
      // 后端 preview 要求路径必须来自用户对话框/拖拽登记（trusted_paths 防线）
      await tauriApi.plugin.registerImportPath(path)
      const result = await tauriApi.plugin.installPreview({ type: source, path })
      if (result.success && result.installToken) {
        set({ installPreview: result, loading: false })
        return true
      }
      set({ error: result.error ?? '安装预览失败', loading: false })
      return false
    } catch (err) {
      set({ error: (err as Error).message, loading: false })
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
        set({ installPreview: null, loading: false })
        return true
      }
      set({ error: result.error ?? '安装失败', loading: false })
      return false
    } catch (err) {
      set({ error: (err as Error).message, loading: false })
      return false
    }
  },

  discardInstall: async () => {
    const preview = get().installPreview
    set({ installPreview: null })
    if (preview?.installToken) {
      try {
        await tauriApi.plugin.installDiscard(preview.installToken)
      } catch {
        // 丢弃失败不阻断 UI
      }
    }
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
      set({ error: (err as Error).message, loading: false })
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
      set({ error: (err as Error).message })
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
      set({ error: (err as Error).message })
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
      set({ error: (err as Error).message })
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
      set({ plugins: currentPlugins, error: (err as Error).message })
      return false
    }
  },

  setPluginStatus: (id, status) => {
    set((state) => ({
      activePlugins: { ...state.activePlugins, [id]: status }
    }))
  }
}))
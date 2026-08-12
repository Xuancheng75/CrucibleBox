import { create } from 'zustand'
import type { PluginMeta, PluginConfig } from '@shared/types/plugin.types'
import { PluginLifecycleStatus } from '@shared/types/plugin.types'
import { pluginApi } from '../api/plugin.api'

export interface PluginState {
  plugins: PluginMeta[]
  loading: boolean
  error: string | null

  activePlugins: Record<string, PluginLifecycleStatus>

  fetchPlugins: () => Promise<void>
  installPlugin: (source: 'zip' | 'directory', path: string) => Promise<boolean>
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

  fetchPlugins: async () => {
    set({ loading: true, error: null })
    try {
      const plugins = await Promise.race([
        pluginApi.list(),
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
      let result
      if (source === 'zip') {
        result = await pluginApi.installFromZip(path)
      } else {
        result = await pluginApi.installFromDirectory(path)
      }
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

  uninstallPlugin: async (id) => {
    set({ loading: true, error: null })
    try {
      const result = await pluginApi.uninstall(id)
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
      const result = await pluginApi.enable(id)
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
      const result = await pluginApi.disable(id)
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
      const result = await pluginApi.updateConfig(id, config)
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
      const result = await pluginApi.reorder(orderedIds)
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

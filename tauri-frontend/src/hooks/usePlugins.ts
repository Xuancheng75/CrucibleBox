import { useEffect } from 'react'
import { usePluginStore } from '../store/plugin.store'
import type {
  PluginMeta,
  PluginConfig,
  PluginLifecycleStatus
} from '../../../shared/types/plugin.types'

export interface UsePluginsReturn {
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
}

export function usePlugins(): UsePluginsReturn {
  const plugins = usePluginStore((s) => s.plugins)
  const loading = usePluginStore((s) => s.loading)
  const error = usePluginStore((s) => s.error)
  const activePlugins = usePluginStore((s) => s.activePlugins)
  const fetchPlugins = usePluginStore((s) => s.fetchPlugins)
  const installPlugin = usePluginStore((s) => s.installPlugin)
  const uninstallPlugin = usePluginStore((s) => s.uninstallPlugin)
  const enablePlugin = usePluginStore((s) => s.enablePlugin)
  const disablePlugin = usePluginStore((s) => s.disablePlugin)
  const updatePluginConfig = usePluginStore((s) => s.updatePluginConfig)
  const reorderPlugins = usePluginStore((s) => s.reorderPlugins)

  useEffect(() => {
    fetchPlugins()
  }, [fetchPlugins])

  return {
    plugins,
    loading,
    error,
    activePlugins,
    fetchPlugins,
    installPlugin,
    uninstallPlugin,
    enablePlugin,
    disablePlugin,
    updatePluginConfig,
    reorderPlugins
  }
}
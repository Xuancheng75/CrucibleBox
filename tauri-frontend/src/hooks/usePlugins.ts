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
  pluginOperationBusy: Record<string, boolean>
  batchOperationBusy: boolean
  reorderBusy: boolean
  fetchPlugins: () => Promise<void>
  installPlugin: (source: 'zip' | 'directory', path: string) => Promise<boolean>
  uninstallPlugin: (id: string) => Promise<boolean>
  enablePlugin: (id: string) => Promise<boolean>
  disablePlugin: (id: string) => Promise<boolean>
  batchEnablePlugins: (
    ids: string[]
  ) => Promise<import('../store/plugin.store').BatchLifecycleResult>
  batchDisablePlugins: (
    ids: string[]
  ) => Promise<import('../store/plugin.store').BatchLifecycleResult>
  updatePluginConfig: (id: string, config: PluginConfig) => Promise<boolean>
  reorderPlugins: (orderedIds: string[]) => Promise<boolean>
}

export function usePlugins(): UsePluginsReturn {
  const plugins = usePluginStore((s) => s.plugins)
  const loading = usePluginStore((s) => s.loading)
  const error = usePluginStore((s) => s.error)
  const activePlugins = usePluginStore((s) => s.activePlugins)
  const pluginOperationBusy = usePluginStore((s) => s.pluginOperationBusy)
  const batchOperationBusy = usePluginStore((s) => s.batchOperationBusy)
  const reorderBusy = usePluginStore((s) => s.reorderBusy)
  const fetchPlugins = usePluginStore((s) => s.fetchPlugins)
  const installPlugin = usePluginStore((s) => s.installPlugin)
  const uninstallPlugin = usePluginStore((s) => s.uninstallPlugin)
  const enablePlugin = usePluginStore((s) => s.enablePlugin)
  const disablePlugin = usePluginStore((s) => s.disablePlugin)
  const batchEnablePlugins = usePluginStore((s) => s.batchEnablePlugins)
  const batchDisablePlugins = usePluginStore((s) => s.batchDisablePlugins)
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
    pluginOperationBusy,
    batchOperationBusy,
    reorderBusy,
    fetchPlugins,
    installPlugin,
    uninstallPlugin,
    enablePlugin,
    disablePlugin,
    batchEnablePlugins,
    batchDisablePlugins,
    updatePluginConfig,
    reorderPlugins
  }
}

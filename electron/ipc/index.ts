import type { PluginManager } from '../../plugin-system/PluginManager'
import { registerPluginIpc } from './plugin.ipc'
import { registerSettingsIpc } from './settings.ipc'
import { registerThemeIpc } from '../theme.service'
import { registerAppIpc } from './app.ipc'
import type { PluginRendererSessionRegistry } from '../../plugin-system/PluginRendererSessionRegistry'
import type { AppUpdateService } from '../AppUpdateService'
import { registerUpdateIpc } from './update.ipc'

export interface PluginRendererIpcDependencies {
  registry: PluginRendererSessionRegistry
  runtimePath: string
}

export function registerAllIpc(
  pluginManager: PluginManager,
  renderer: PluginRendererIpcDependencies,
  updateService: AppUpdateService
): void {
  registerPluginIpc(pluginManager, renderer)
  registerSettingsIpc()
  registerThemeIpc(undefined, (theme) => {
    pluginManager.notifyThemeChanged(theme)
  })
  registerAppIpc()
  registerUpdateIpc(updateService)
}

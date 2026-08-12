import type {
  PluginMeta,
  PluginConfig,
  PluginLogEntry,
  PluginLogFilter
} from '@shared/types/plugin.types'
import type { ToolboxTheme } from '@shared/types/theme.types'
import type { PluginRendererSessionDescriptor } from '@shared/types/ipc.types'
import type { AppUpdateChannel, AppUpdateState } from '@shared/types/ipc.types'

declare global {
  interface Window {
    electronAPI: {
      plugin: {
        install(source: {
          type: 'zip' | 'directory'
          path: string
        }): Promise<{ success: boolean; data?: PluginMeta; error?: string }>
        registerImportPath(path: string): Promise<{ success: boolean }>
        uninstall(id: string): Promise<{ success: boolean; error?: string }>
        list(): Promise<PluginMeta[]>
        get(id: string): Promise<PluginMeta | null>
        enable(id: string): Promise<{ success: boolean; error?: string }>
        disable(id: string): Promise<{ success: boolean; error?: string }>
        reorder(
          orderedIds: string[]
        ): Promise<{ success: boolean; data?: PluginMeta[]; error?: string }>
        updateConfig(
          id: string,
          config: PluginConfig
        ): Promise<{ success: boolean; error?: string }>
        sendMessage(id: string, message: unknown): Promise<{ success: boolean; error?: string }>
        getLogs(
          filter?: PluginLogFilter
        ): Promise<{ success: boolean; data?: PluginLogEntry[]; error?: string }>
        clearLogs(pluginId?: string): Promise<{ success: boolean; error?: string }>
        createRendererSession(id: string): Promise<PluginRendererSessionDescriptor>
        disposeRendererSession(token: string): Promise<{ success: boolean }>
        onMessage(callback: (data: { pluginId: string; message: unknown }) => void): () => void
        onLog(
          callback: (data: { pluginId: string; level: string; message: string }) => void
        ): () => void
        onStatusChange(callback: (data: { pluginId: string; status: string }) => void): () => void
      }
      dialog: {
        openFile(): Promise<string | null>
        openDirectory(): Promise<string | null>
      }
      file: {
        getPath(file: File): string
      }
      settings: {
        get(key: string): Promise<string | null>
        set(key: string, value: string): Promise<boolean>
        getAll(): Promise<Record<string, string>>
      }
      theme: {
        get(): Promise<ToolboxTheme | null>
        set(theme: ToolboxTheme): Promise<ToolboxTheme | null>
        list(): Promise<ToolboxTheme[]>
        onChanged(callback: (theme: ToolboxTheme) => void): () => void
      }
      app: {
        getVersion(): Promise<string>
        getPlatform(): Promise<string>
        update: {
          getState(): Promise<AppUpdateState>
          setChannel(channel: AppUpdateChannel): Promise<AppUpdateState>
          check(): Promise<AppUpdateState>
          download(): Promise<AppUpdateState>
          install(): Promise<AppUpdateState>
          onChanged(callback: (state: AppUpdateState) => void): () => void
        }
      }
      menu: {
        onImportPlugin(callback: () => void): () => void
      }
    }
  }
}

function getAPI() {
  if (!window.electronAPI) {
    throw new Error('electronAPI not available. Ensure preload script is loaded.')
  }
  return window.electronAPI
}

export const pluginApi = {
  installFromZip: async (path: string) => {
    return getAPI().plugin.install({ type: 'zip', path })
  },

  installFromDirectory: async (path: string) => {
    return getAPI().plugin.install({ type: 'directory', path })
  },

  uninstall: async (id: string) => {
    return getAPI().plugin.uninstall(id)
  },

  list: async () => {
    return getAPI().plugin.list()
  },

  get: async (id: string) => {
    return getAPI().plugin.get(id)
  },

  enable: async (id: string) => {
    return getAPI().plugin.enable(id)
  },

  disable: async (id: string) => {
    return getAPI().plugin.disable(id)
  },

  reorder: async (orderedIds: string[]) => {
    return getAPI().plugin.reorder(orderedIds)
  },

  updateConfig: async (id: string, config: PluginConfig) => {
    return getAPI().plugin.updateConfig(id, config)
  },

  sendMessage: async (id: string, message: unknown) => {
    return getAPI().plugin.sendMessage(id, message)
  },

  getLogs: async (filter?: PluginLogFilter) => {
    return getAPI().plugin.getLogs(filter)
  },

  clearLogs: async (pluginId?: string) => {
    return getAPI().plugin.clearLogs(pluginId)
  },

  openFileDialog: async () => {
    return getAPI().dialog.openFile()
  },

  openDirectoryDialog: async () => {
    return getAPI().dialog.openDirectory()
  }
}

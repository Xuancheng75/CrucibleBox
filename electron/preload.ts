import { contextBridge, ipcRenderer, webUtils } from 'electron'
import { IpcChannel } from '../shared/types/ipc.types'
import type { ToolboxTheme } from '../shared/types/theme.types'
import type { AppUpdateChannel, AppUpdateState } from '../shared/types/ipc.types'

const api = {
  // Plugin management
  plugin: {
    install: (source: { type: 'zip' | 'directory'; path: string }) =>
      ipcRenderer.invoke(IpcChannel.PluginInstall, source),
    registerImportPath: (path: string) =>
      ipcRenderer.invoke(IpcChannel.PluginRegisterImportPath, path),
    uninstall: (id: string) => ipcRenderer.invoke(IpcChannel.PluginUninstall, id),
    list: () => ipcRenderer.invoke(IpcChannel.PluginList),
    get: (id: string) => ipcRenderer.invoke(IpcChannel.PluginGet, id),
    enable: (id: string) => ipcRenderer.invoke(IpcChannel.PluginEnable, id),
    disable: (id: string) => ipcRenderer.invoke(IpcChannel.PluginDisable, id),
    reorder: (orderedIds: string[]) => ipcRenderer.invoke(IpcChannel.PluginReorder, orderedIds),
    updateConfig: (id: string, config: Record<string, unknown>) =>
      ipcRenderer.invoke(IpcChannel.PluginUpdateConfig, id, config),
    sendMessage: (id: string, message: unknown) =>
      ipcRenderer.invoke(IpcChannel.PluginSendMessage, id, message),
    getLogs: (filter?: { pluginId?: string; level?: string; limit?: number }) =>
      ipcRenderer.invoke(IpcChannel.PluginGetLogs, filter),
    clearLogs: (pluginId?: string) => ipcRenderer.invoke(IpcChannel.PluginClearLogs, pluginId),
    createRendererSession: (id: string) =>
      ipcRenderer.invoke(IpcChannel.PluginCreateRendererSession, id),
    disposeRendererSession: (token: string) =>
      ipcRenderer.invoke(IpcChannel.PluginDisposeRendererSession, token),
    onMessage: (callback: (data: { pluginId: string; message: unknown }) => void) => {
      const handler = (
        _event: Electron.IpcRendererEvent,
        data: { pluginId: string; message: unknown }
      ) => callback(data)
      ipcRenderer.on(IpcChannel.PluginMessage, handler)
      return () => ipcRenderer.removeListener(IpcChannel.PluginMessage, handler)
    },
    onLog: (callback: (data: { pluginId: string; level: string; message: string }) => void) => {
      const handler = (
        _event: Electron.IpcRendererEvent,
        data: { pluginId: string; level: string; message: string }
      ) => callback(data)
      ipcRenderer.on(IpcChannel.PluginLog, handler)
      return () => ipcRenderer.removeListener(IpcChannel.PluginLog, handler)
    },
    onStatusChange: (callback: (data: { pluginId: string; status: string }) => void) => {
      const handler = (
        _event: Electron.IpcRendererEvent,
        data: { pluginId: string; status: string }
      ) => callback(data)
      ipcRenderer.on(IpcChannel.PluginStatusChange, handler)
      return () => ipcRenderer.removeListener(IpcChannel.PluginStatusChange, handler)
    }
  },

  // Dialogs
  dialog: {
    openFile: () => ipcRenderer.invoke(IpcChannel.DialogOpenFile),
    openDirectory: () => ipcRenderer.invoke(IpcChannel.DialogOpenDirectory)
  },

  file: {
    getPath: (file: File) => webUtils.getPathForFile(file)
  },

  // Settings
  settings: {
    get: (key: string) => ipcRenderer.invoke(IpcChannel.SettingsGet, key),
    set: (key: string, value: string) => ipcRenderer.invoke(IpcChannel.SettingsSet, key, value),
    getAll: () => ipcRenderer.invoke(IpcChannel.SettingsGetAll)
  },

  // Theme
  theme: {
    get: () => ipcRenderer.invoke(IpcChannel.ThemeGet),
    set: (theme: ToolboxTheme) => ipcRenderer.invoke(IpcChannel.ThemeSet, theme),
    list: () => ipcRenderer.invoke(IpcChannel.ThemeList),
    onChanged: (callback: (theme: ToolboxTheme) => void) => {
      const handler = (_event: Electron.IpcRendererEvent, theme: ToolboxTheme) => callback(theme)
      ipcRenderer.on(IpcChannel.ThemeChanged, handler)
      return () => ipcRenderer.removeListener(IpcChannel.ThemeChanged, handler)
    }
  },

  // App info
  app: {
    getVersion: () => ipcRenderer.invoke(IpcChannel.AppGetVersion),
    getPlatform: () => ipcRenderer.invoke(IpcChannel.AppGetPlatform),
    update: {
      getState: (): Promise<AppUpdateState> => ipcRenderer.invoke(IpcChannel.AppUpdateGetState),
      setChannel: (channel: AppUpdateChannel): Promise<AppUpdateState> =>
        ipcRenderer.invoke(IpcChannel.AppUpdateSetChannel, channel),
      check: (): Promise<AppUpdateState> => ipcRenderer.invoke(IpcChannel.AppUpdateCheck),
      download: (): Promise<AppUpdateState> => ipcRenderer.invoke(IpcChannel.AppUpdateDownload),
      install: (): Promise<AppUpdateState> => ipcRenderer.invoke(IpcChannel.AppUpdateInstall),
      onChanged: (callback: (state: AppUpdateState) => void) => {
        const handler = (_event: Electron.IpcRendererEvent, state: AppUpdateState) =>
          callback(state)
        ipcRenderer.on(IpcChannel.AppUpdateChanged, handler)
        return () => ipcRenderer.removeListener(IpcChannel.AppUpdateChanged, handler)
      }
    }
  },

  // Menu events (main -> renderer)
  menu: {
    onImportPlugin: (callback: () => void) => {
      const handler = () => callback()
      ipcRenderer.on('menu:import-plugin', handler)
      return () => ipcRenderer.removeListener('menu:import-plugin', handler)
    }
  }
}

contextBridge.exposeInMainWorld('electronAPI', api)

export type ElectronAPI = typeof api

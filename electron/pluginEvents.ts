import { BrowserWindow } from 'electron'
import { IpcChannel } from '@shared/types/ipc.types'
import type { PluginManager } from '../plugin-system/PluginManager'

function broadcast(channel: string, data: unknown): void {
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.isDestroyed() && !win.webContents.isDestroyed()) {
      win.webContents.send(channel, data)
    }
  }
}

export function registerPluginEventBridge(pluginManager: PluginManager): void {
  pluginManager.onEvent('plugin:log', (data) => {
    broadcast(IpcChannel.PluginLog, data)
  })
  pluginManager.onEvent('plugin:status', (data) => {
    broadcast(IpcChannel.PluginStatusChange, data)
  })
  pluginManager.onEvent('plugin:message', (data) => {
    broadcast(IpcChannel.PluginMessage, data)
  })
}

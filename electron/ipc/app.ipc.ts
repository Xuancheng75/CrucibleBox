// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { ipcMain, app } from 'electron'
import { IpcChannel } from '@shared/types/ipc.types'
import { assertTrustedSender } from './ipcGuard'

export function registerAppIpc(): void {
  ipcMain.handle(IpcChannel.AppGetVersion, (event) => {
    assertTrustedSender(event)
    try {
      return app.getVersion()
    } catch (err) {
      console.error('[IPC] AppGetVersion error:', err)
      return ''
    }
  })

  ipcMain.handle(IpcChannel.AppGetPlatform, (event) => {
    assertTrustedSender(event)
    try {
      return process.platform
    } catch (err) {
      console.error('[IPC] AppGetPlatform error:', err)
      return ''
    }
  })
}
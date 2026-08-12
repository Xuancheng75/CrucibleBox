import { ipcMain } from 'electron'
import { IpcChannel } from '@shared/types/ipc.types'
import type { AppUpdateService } from '../AppUpdateService'
import { assertTrustedSender } from './ipcGuard'

export function registerUpdateIpc(service: AppUpdateService): void {
  ipcMain.handle(IpcChannel.AppUpdateGetState, (event) => {
    assertTrustedSender(event)
    return service.getState()
  })
  ipcMain.handle(IpcChannel.AppUpdateSetChannel, (event, channel: unknown) => {
    assertTrustedSender(event)
    return service.setChannel(channel)
  })
  ipcMain.handle(IpcChannel.AppUpdateCheck, async (event) => {
    assertTrustedSender(event)
    return service.check()
  })
  ipcMain.handle(IpcChannel.AppUpdateDownload, async (event) => {
    assertTrustedSender(event)
    return service.download()
  })
  ipcMain.handle(IpcChannel.AppUpdateInstall, async (event) => {
    assertTrustedSender(event)
    return service.install()
  })
}

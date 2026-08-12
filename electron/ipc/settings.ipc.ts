import { ipcMain } from 'electron'
import { IpcChannel } from '@shared/types/ipc.types'
import { SettingsRepository } from '@database/repositories/settings.repository'
import { assertTrustedSender, assertAllowedSettingsKey } from './ipcGuard'

export function registerSettingsIpc(): void {
  ipcMain.handle(IpcChannel.SettingsGet, async (event, key: string) => {
    assertTrustedSender(event)
    try {
      return SettingsRepository.get(key)
    } catch (err) {
      console.error('[IPC] SettingsGet error:', err)
      return null
    }
  })

  ipcMain.handle(IpcChannel.SettingsSet, async (event, key: string, value: string) => {
    assertTrustedSender(event)
    try {
      assertAllowedSettingsKey(key)
      SettingsRepository.set(key, value)
      return true
    } catch (err) {
      console.error('[IPC] SettingsSet error:', err)
      return false
    }
  })

  ipcMain.handle(IpcChannel.SettingsGetAll, async (event) => {
    assertTrustedSender(event)
    try {
      return SettingsRepository.getAll()
    } catch (err) {
      console.error('[IPC] SettingsGetAll error:', err)
      return {}
    }
  })
}

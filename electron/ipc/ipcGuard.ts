import { BrowserWindow, type IpcMainInvokeEvent } from 'electron'
import { HOST_RENDERER_URL } from '../HostRendererProtocol'
import { isAllowedHostRendererUrl } from './ipcGuardPolicy'

const PACKAGED_RENDERER_URL = HOST_RENDERER_URL

/**
 * Assert an IPC call originates from the app's own main window webContents.
 * This prevents plugin renderer code (which runs in the app renderer) or any
 * injected script from invoking privileged main-process channels.
 */
export function assertTrustedSender(event: IpcMainInvokeEvent): void {
  const senderId = event.sender.id
  const windows = BrowserWindow.getAllWindows()
  const mainWindow = windows.find((w) => w.webContents.id === senderId)
  const senderFrame = event.senderFrame
  const isMainFrame = senderFrame !== null && senderFrame === event.sender.mainFrame
  const isAllowedUrl =
    senderFrame !== null &&
    isAllowedHostRendererUrl(
      senderFrame.url,
      process.env['ELECTRON_RENDERER_URL'],
      PACKAGED_RENDERER_URL
    )
  if (!mainWindow || !isMainFrame || !isAllowedUrl) {
    console.error(
      '[ipcGuard] 拒绝不受信任的 IPC 来源:',
      JSON.stringify({
        senderId,
        senderUrl: senderFrame?.url ?? null,
        isMainFrame,
        windowCount: windows.length,
        windowIds: windows.map((w) => w.webContents.id)
      })
    )
    throw new Error('IPC 调用来源不受信任')
  }
}

/** Keys the renderer is allowed to write via settings:set. */
const ALLOWED_SETTINGS_KEYS = new Set(['theme'])

export function assertAllowedSettingsKey(key: string): void {
  if (!ALLOWED_SETTINGS_KEYS.has(key)) {
    throw new Error(`不允许写入设置项: ${key}`)
  }
}

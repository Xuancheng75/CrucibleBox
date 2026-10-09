import { open, confirm } from '@tauri-apps/plugin-dialog'
import { notification } from 'antd'
import { createNextThemeController } from './NextThemeController'
export function createNextUiController() {
  let active = true
  const theme = createNextThemeController()
  return {
    async execute(operation: string, params: Record<string, unknown>): Promise<unknown> {
      if (!active) throw new Error('SESSION_DENIED')
      if (operation.startsWith('theme.')) return theme.execute(operation, params)
      if (operation === 'dialog.open') {
        const selected = await open({
          directory: params.type === 'folder',
          multiple: params.multiple === true,
          ...(Array.isArray(params.extensions) && params.extensions.length
            ? { filters: [{ name: '文件', extensions: params.extensions as string[] }] }
            : {})
        })
        if (!active) throw new Error('SESSION_DENIED')
        return selected === null ? null : Array.isArray(selected) ? selected : [selected]
      }
      if (operation === 'dialog.confirm') {
        const accepted = await confirm(params.message as string, {
          title: params.title as string,
          okLabel: params.confirmLabel as string | undefined,
          cancelLabel: params.cancelLabel as string | undefined
        })
        if (!active) throw new Error('SESSION_DENIED')
        return accepted
      }
      if (operation === 'notification.show') {
        notification.info({ message: params.title as string, description: params.body as string })
        return true
      }
      throw new Error('INVALID_REQUEST')
    },
    dispose() {
      active = false
      theme.dispose()
    }
  }
}

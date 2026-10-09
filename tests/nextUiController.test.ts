import { beforeEach, expect, test, vi } from 'vitest'
const mock = vi.hoisted(() => ({ open: vi.fn(), confirm: vi.fn(), notice: vi.fn() }))
vi.mock('../tauri-frontend/node_modules/@tauri-apps/plugin-dialog/dist-js/index.js', () => ({
  open: mock.open,
  confirm: mock.confirm
}))
vi.mock('antd', () => ({ notification: { info: mock.notice } }))
vi.mock('../tauri-frontend/src/features/plugins/runtime/NextThemeController', () => ({
  createNextThemeController: () => ({ execute: vi.fn(), dispose: vi.fn() })
}))
import { createNextUiController } from '../tauri-frontend/src/features/plugins/runtime/NextUiController'
beforeEach(() => {
  mock.open.mockReset()
  mock.confirm.mockReset()
  mock.notice.mockReset()
})
test('native selection normalizes single, multiple and cancelled results', async () => {
  const c = createNextUiController()
  mock.open
    .mockResolvedValueOnce('C:/file.txt')
    .mockResolvedValueOnce(['C:/a', 'C:/b'])
    .mockResolvedValueOnce(null)
  expect(await c.execute('dialog.open', { type: 'file', extensions: ['txt'] })).toEqual([
    'C:/file.txt'
  ])
  expect(mock.open).toHaveBeenLastCalledWith({
    directory: false,
    multiple: false,
    filters: [{ name: '文件', extensions: ['txt'] }]
  })
  expect(await c.execute('dialog.open', { type: 'file', multiple: true })).toEqual(['C:/a', 'C:/b'])
  expect(await c.execute('dialog.open', { type: 'folder' })).toBeNull()
})
test('session close suppresses late native response and future side effects', async () => {
  let release!: (value: string) => void
  mock.open.mockImplementation(
    () =>
      new Promise((resolve) => {
        release = resolve
      })
  )
  const c = createNextUiController()
  const pending = c.execute('dialog.open', { type: 'file' })
  c.dispose()
  release('C:/private.txt')
  await expect(pending).rejects.toThrow('SESSION_DENIED')
  await expect(c.execute('notification.show', { title: 'late', body: 'body' })).rejects.toThrow(
    'SESSION_DENIED'
  )
  expect(mock.notice).not.toHaveBeenCalled()
})

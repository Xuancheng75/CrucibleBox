import { beforeEach, expect, test, vi } from 'vitest'
import { DEFAULT_THEME } from '../shared/themes/presets'
const mock = vi.hoisted(() => ({ theme: null as unknown, persist: vi.fn() }))
vi.mock('../tauri-frontend/src/store/theme.store', () => ({
  useThemeStore: {
    getState: () => ({ theme: mock.theme, setTheme: mock.persist }),
    setState: (v: { theme: unknown }) => {
      mock.theme = v.theme
    }
  }
}))
beforeEach(() => {
  vi.resetModules()
  mock.theme = DEFAULT_THEME
  mock.persist.mockReset()
  mock.persist.mockImplementation(async (theme) => {
    mock.theme = theme
    return true
  })
})
const next = {
  ...DEFAULT_THEME,
  id: 'custom',
  tokens: { ...DEFAULT_THEME.tokens, colorPrimary: '#123456' }
}
test('preview is ephemeral, exclusive and restores only its own current theme', async () => {
  const { createNextThemeController } =
    await import('../tauri-frontend/src/features/plugins/runtime/NextThemeController')
  const a = createNextThemeController(),
    b = createNextThemeController()
  expect(await a.execute('theme.preview', { theme: next })).toBe(true)
  expect(mock.persist).not.toHaveBeenCalled()
  await expect(b.execute('theme.preview', { theme: next })).rejects.toThrow('BUSY')
  a.dispose()
  await b.execute('theme.get', {})
  expect(mock.theme).toEqual(DEFAULT_THEME)
  await b.execute('theme.preview', { theme: next })
  mock.theme = DEFAULT_THEME
  b.dispose()
  await Promise.resolve()
  expect(mock.theme).toEqual(DEFAULT_THEME)
})
test('closed queued preview never executes and a successful commit survives disposal', async () => {
  const { createNextThemeController } =
    await import('../tauri-frontend/src/features/plugins/runtime/NextThemeController')
  const a = createNextThemeController()
  const pending = a.execute('theme.preview', { theme: next })
  a.dispose()
  await expect(pending).rejects.toThrow('SESSION_DENIED')
  const b = createNextThemeController()
  await b.execute('theme.preview', { theme: next })
  expect(await b.execute('theme.commit', {})).toBe(true)
  b.dispose()
  await Promise.resolve()
  expect(mock.theme).toEqual(next)
  expect(mock.persist).toHaveBeenCalledTimes(1)
})

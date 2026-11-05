import { beforeEach, expect, test, vi } from 'vitest'
import { DEFAULT_THEME } from '../shared/themes/presets'
const mock = vi.hoisted(() => ({ theme: null as unknown, persist: vi.fn(), prepare: vi.fn() }))
vi.mock('../tauri-frontend/src/theme/prepare', () => ({ prepareThemeStyle: mock.prepare }))
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
  mock.prepare.mockReset().mockResolvedValue({ version: 2, css: '', motion: 'off', layers: [] })
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
test('a preview closed during asset preparation cannot apply a late theme', async () => {
  const { createNextThemeController } =
    await import('../tauri-frontend/src/features/plugins/runtime/NextThemeController')
  let release!: () => void
  mock.prepare.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        release = resolve
      })
  )
  const a = createNextThemeController()
  const pending = a.execute('theme.preview', { theme: next })
  await Promise.resolve()
  a.dispose()
  release()
  await expect(pending).rejects.toThrow('SESSION_DENIED')
  expect(mock.theme).toEqual(DEFAULT_THEME)
})
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
test('an explicit default reset runs after an in-flight preview and survives its disposal', async () => {
  const { createNextThemeController, restoreDefaultTheme } =
    await import('../tauri-frontend/src/features/plugins/runtime/NextThemeController')
  let release!: () => void
  mock.prepare.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        release = resolve
      })
  )
  const controller = createNextThemeController()
  const preview = controller.execute('theme.preview', { theme: next })
  await Promise.resolve()
  const reset = restoreDefaultTheme()
  release()
  await preview
  expect(await reset).toBe(true)
  controller.dispose()
  await Promise.resolve()
  expect(mock.theme).toEqual(DEFAULT_THEME)
})

test('leaving a retained plugin rolls back without disposing it, and committed changes survive', async () => {
  const { createNextThemeController } =
    await import('../tauri-frontend/src/features/plugins/runtime/NextThemeController')
  const controller = createNextThemeController()
  await controller.execute('theme.preview', { theme: next })
  controller.setVisible(false)
  expect(await controller.execute('theme.get', {})).toEqual(DEFAULT_THEME)
  await expect(controller.execute('theme.preview', { theme: next })).rejects.toThrow(
    'SESSION_DENIED'
  )
  await expect(controller.execute('theme.commit', {})).rejects.toThrow('SESSION_DENIED')
  controller.setVisible(true)
  await controller.execute('theme.preview', { theme: next })
  await controller.execute('theme.commit', {})
  controller.setVisible(false)
  expect(await controller.execute('theme.get', {})).toEqual(next)
})

test('leaving during asset preparation prevents a late unsaved preview', async () => {
  const { createNextThemeController } =
    await import('../tauri-frontend/src/features/plugins/runtime/NextThemeController')
  let release!: () => void
  mock.prepare.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        release = resolve
      })
  )
  const controller = createNextThemeController()
  const pending = controller.execute('theme.preview', { theme: next })
  await Promise.resolve()
  controller.setVisible(false)
  release()
  await expect(pending).rejects.toThrow('SESSION_DENIED')
  expect(await controller.execute('theme.get', {})).toEqual(DEFAULT_THEME)
  expect(mock.persist).not.toHaveBeenCalled()
})

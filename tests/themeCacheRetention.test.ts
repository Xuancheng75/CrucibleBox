import { beforeEach, expect, test, vi } from 'vitest'
import { DEFAULT_THEME } from '../shared/themes/presets'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('../tauri-frontend/node_modules/@tauri-apps/api/core.js', () => ({ invoke }))
beforeEach(() => {
  vi.resetModules()
  invoke.mockReset()
})
for (const id of ['light', 'leaf', 'custom-saved']) {
  test(`startup preserves saved ${id} colours without a settings write`, async () => {
    const saved = {
      ...DEFAULT_THEME,
      id,
      name: 'Saved colours',
      tokens: { ...DEFAULT_THEME.tokens, colorPrimary: '#123456' }
    }
    const raw = JSON.stringify(saved)
    invoke.mockResolvedValue(raw)
    const cache = await import('../tauri-frontend/src/themeCache')
    const loaded = await cache.loadTheme()
    expect(loaded).toEqual(saved)
    expect(invoke.mock.calls).toEqual([['settings_get', { key: 'theme' }]])
  })
}
test('parallel startup reads share one request and malformed values are not overwritten', async () => {
  let resolve!: (value: string) => void
  invoke.mockImplementation(
    () =>
      new Promise<string>((done) => {
        resolve = done
      })
  )
  const cache = await import('../tauri-frontend/src/themeCache')
  const a = cache.loadTheme(),
    b = cache.loadTheme()
  expect(invoke).toHaveBeenCalledTimes(1)
  resolve('malformed user data')
  expect(await a).toEqual(DEFAULT_THEME)
  expect(await b).toEqual(DEFAULT_THEME)
  expect(invoke).toHaveBeenCalledTimes(1)
})
test('failed explicit persistence does not report success or replace the cached theme', async () => {
  invoke
    .mockResolvedValueOnce(JSON.stringify(DEFAULT_THEME))
    .mockRejectedValueOnce(new Error('disk unavailable'))
  const cache = await import('../tauri-frontend/src/themeCache')
  await cache.loadTheme()
  const log = vi.spyOn(console, 'error').mockImplementation(() => undefined)
  try {
    expect(await cache.setTheme({ ...DEFAULT_THEME, id: 'custom-unsaved' })).toBeNull()
    expect(cache.getThemeSync()).toEqual(DEFAULT_THEME)
  } finally {
    log.mockRestore()
  }
})

import { beforeEach, expect, test, vi } from 'vitest'

const fixture = vi.hoisted(() => ({
  plugins: [
    { id: 'one', enabled: true, version: '1' },
    { id: 'two', enabled: true, version: '1' }
  ],
  openedPluginIds: ['one', 'two'],
  currentPage: 'pluginView'
}))
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useEffect: vi.fn(),
  useState: () => ['', vi.fn()]
}))
vi.mock('../tauri-frontend/src/hooks/useGlobalPluginDrop', () => ({
  useGlobalPluginDrop: () => ({ dragActive: false, dragTarget: null })
}))
vi.mock('../tauri-frontend/src/store/app.store', () => ({
  useAppStore: (selector: (state: unknown) => unknown) =>
    selector({
      ...fixture,
      visitedPages: ['home'],
      activePluginId: 'one',
      setCurrentPage: vi.fn(),
      setActivityTab: vi.fn(),
      setPluginImportOpen: vi.fn()
    })
}))
vi.mock('../tauri-frontend/src/store/plugin.store', () => ({
  usePluginStore: (selector: (state: unknown) => unknown) => selector(fixture)
}))
import App from '../tauri-frontend/src/App'

const frames = () => App().props.children[0].props.children[1]
beforeEach(() => {
  fixture.plugins = [
    { id: 'one', enabled: true, version: '1' },
    { id: 'two', enabled: true, version: '1' }
  ]
  fixture.currentPage = 'pluginView'
})
test('catalog reorder and navigation never reorder retained iframe containers', () => {
  const first = frames().map((view: { key: string }) => view.key)
  fixture.plugins.reverse()
  fixture.currentPage = 'home'
  expect(frames().map((view: { key: string }) => view.key)).toEqual(first)
  fixture.currentPage = 'pluginView'
  expect(frames().map((view: { key: string }) => view.key)).toEqual(first)
})
test('disabled views disappear and upgraded views get a new identity', () => {
  fixture.plugins[0].enabled = false
  fixture.plugins[1].version = '2'
  expect(frames().map((view: { key: string }) => view.key)).toEqual(['two:2'])
})

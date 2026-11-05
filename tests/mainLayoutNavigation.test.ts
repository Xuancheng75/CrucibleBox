import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import type { ReactElement } from 'react'
import { useLayoutEffect } from 'react'
const fixture = vi.hoisted(() => ({
  page: 'home',
  previousPage: 'home',
  activePluginId: 'one',
  compact: false,
  navigation: 'menu',
  animation: { currentTime: 200, play: vi.fn() }
}))
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useEffect: vi.fn(),
  useLayoutEffect: vi.fn(),
  useRef: () => ({ current: { getAnimations: () => [fixture.animation] } }),
  useState: () => [
    fixture.compact,
    (value: boolean) => {
      fixture.compact = value
    }
  ]
}))
vi.mock('antd', () => ({
  Layout: Object.assign(() => null, { Header: 'header', Sider: 'aside', Content: 'main' }),
  Badge: 'span',
  Button: 'button',
  theme: { useToken: () => ({ token: {} }) }
}))
vi.mock('../tauri-frontend/src/components/IconRail', () => ({ default: () => null }))
vi.mock('../tauri-frontend/src/components/CommandPalette', () => ({ default: () => null }))
vi.mock('../tauri-frontend/src/store/app.store', () => ({
  useAppStore: (selector: (s: unknown) => unknown) =>
    selector({
      currentPage: fixture.page,
      previousPage: fixture.previousPage,
      activePluginId: fixture.activePluginId,
      setCurrentPage: vi.fn()
    })
}))
vi.mock('../tauri-frontend/src/store/theme.store', () => ({
  useThemeStore: (selector: (s: unknown) => unknown) =>
    selector({
      theme: {
        name: 'Test',
        design: { layout: { navigation: fixture.navigation, sidebarWidth: 224 } }
      }
    })
}))
vi.mock('../tauri-frontend/src/store/task.store', () => ({
  useTaskStore: (selector: (s: unknown) => unknown) => selector({ tasks: [] })
}))
import MainLayout from '../tauri-frontend/src/layouts/MainLayout'

type Node = ReactElement<{
  className?: string
  children?: unknown
  width?: number
  compact?: boolean
  onCompactChange?: () => void
  'data-page-direction'?: string
  'aria-label'?: string
}>
function find(node: unknown, predicate: (node: Node) => boolean): Node | undefined {
  if (Array.isArray(node)) return node.flatMap((n) => find(n, predicate) ?? [])[0]
  if (!node || typeof node !== 'object' || !('props' in node)) return undefined
  const element = node as Node
  return predicate(element) ? element : find(element.props.children, predicate)
}
const render = () => MainLayout({ children: null, appVersion: 'test' })
afterEach(() => vi.unstubAllGlobals())
beforeEach(() => {
  fixture.page = 'home'
  fixture.previousPage = 'home'
  fixture.activePluginId = 'one'
  fixture.compact = false
  fixture.navigation = 'menu'
  fixture.animation.currentTime = 200
  fixture.animation.play.mockClear()
})

test('page direction reaches the viewport and only its own animation restarts', () => {
  const dataset = {}
  vi.stubGlobal('document', { documentElement: { dataset } })
  fixture.previousPage = 'home'
  fixture.page = 'marketplace'
  const layout = render()
  expect(
    find(layout, (n) => n.props.className === 'cbx-page-stage')?.props['data-page-direction']
  ).toBe('up')
  expect(find(layout, (n) => n.props.className === 'cbx-header-location')).toBeUndefined()
  expect(find(layout, (n) => n.props.className === 'ob-main-content')?.props['aria-label']).toBe(
    '插件市场'
  )
  vi.mocked(useLayoutEffect).mock.calls.at(-1)![0]()
  expect(fixture.animation.currentTime).toBe(0)
  expect(fixture.animation.play).toHaveBeenCalledOnce()
  expect(dataset).toEqual({ obPageDirection: 'up', obPageTransition: 'none' })
  expect(vi.mocked(useLayoutEffect).mock.calls.at(-1)![1]).toEqual(['marketplace', 'up', 'none'])
})

test('menu sidebar stays expanded on every page until the user folds it', () => {
  for (const page of ['home', 'pluginView', 'marketplace', 'tasks', 'logs', 'settings', 'home']) {
    fixture.page = page
    const layout = render()
    expect(find(layout, (n) => n.type === 'aside')?.props.width).toBe(224)
    expect(find(layout, (n) => typeof n.props.onCompactChange === 'function')?.props.compact).toBe(
      false
    )
  }
  find(render(), (n) => typeof n.props.onCompactChange === 'function')!.props.onCompactChange!()
  for (const page of ['pluginView', 'home', 'settings']) {
    fixture.page = page
    expect(find(render(), (n) => n.type === 'aside')?.props.width).toBe(56)
  }
})

test('top navigation hides the sider, and transition identity follows route and plugin without keying content', () => {
  fixture.navigation = 'top'
  expect(find(render(), (n) => n.type === 'aside')).toBeUndefined()
  const curtain = () => find(render(), (n) => n.props.className === 'cbx-page-transition')!
  expect(curtain().key).toBe('home')
  fixture.page = 'pluginView'
  expect(curtain().key).toBe('plugin:one')
  fixture.activePluginId = 'two'
  expect(curtain().key).toBe('plugin:two')
  expect(find(render(), (n) => n.props.className === 'cbx-page-stage')?.key).toBeNull()
})

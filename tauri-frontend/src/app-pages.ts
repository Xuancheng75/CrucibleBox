import type { ComponentType } from 'react'

export const APP_PAGE_IDS = [
  'home',
  'marketplace',
  'tasks',
  'logs',
  'pluginView',
  'settings'
] as const

export type AppPage = (typeof APP_PAGE_IDS)[number]

const NAV_ORDER: Record<AppPage, number> = {
  home: 0,
  marketplace: 1,
  tasks: 2,
  logs: 2,
  settings: 3,
  pluginView: 0
}

export function pageTransitionDirection(
  from: AppPage,
  to: AppPage,
  navigation: 'rail' | 'menu' | 'top'
): 'up' | 'down' | 'left' | 'right' | 'none' {
  if (to === 'pluginView') return 'left'
  if (from === 'pluginView') return 'right'
  const step = NAV_ORDER[to] - NAV_ORDER[from]
  if (step === 0) return 'none'
  if (navigation === 'top') return step > 0 ? 'left' : 'right'
  return step > 0 ? 'up' : 'down'
}

type PageModule = { default: ComponentType }

export const APP_PAGE_LOADERS: Record<AppPage, () => Promise<PageModule>> = {
  home: () => import('./pages/HomeFocus'),
  marketplace: () => import('./pages/Marketplace'),
  tasks: () => import('./pages/TaskCenter'),
  logs: () => import('./pages/PluginLogs'),
  pluginView: () => import('./components/PluginView'),
  settings: () => import('./pages/Settings')
}

export function isAppPage(value: string): value is AppPage {
  return APP_PAGE_IDS.some((page) => page === value)
}

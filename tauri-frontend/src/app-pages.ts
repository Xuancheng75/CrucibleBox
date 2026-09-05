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

type PageModule = { default: ComponentType }

export const APP_PAGE_LOADERS: Record<AppPage, () => Promise<PageModule>> = {
  home: () => import('./pages/Home'),
  marketplace: () => import('./pages/Marketplace'),
  tasks: () => import('./pages/TaskCenter'),
  logs: () => import('./pages/PluginLogs'),
  pluginView: () => import('./components/PluginView'),
  settings: () => import('./pages/Settings')
}

export function isAppPage(value: string): value is AppPage {
  return APP_PAGE_IDS.some((page) => page === value)
}

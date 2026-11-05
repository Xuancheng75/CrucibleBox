import { beforeEach, describe, expect, it } from 'vitest'
import { useAppStore } from '../tauri-frontend/src/store/app.store'
import { pageTransitionDirection } from '../tauri-frontend/src/app-pages'

describe('workspace navigation state', () => {
  beforeEach(() => useAppStore.setState(useAppStore.getInitialState(), true))

  it('retains opened plugin identities while navigating away and back', () => {
    const store = useAppStore.getState()
    store.setActivePluginId('retained-a')
    store.setCurrentPage('pluginView')
    store.setCurrentPage('marketplace')
    store.setActivePluginId('retained-b')
    store.setCurrentPage('pluginView')
    store.setActivePluginId('retained-a')
    expect(useAppStore.getState().openedPluginIds).toEqual(['retained-a', 'retained-b'])
    expect(useAppStore.getState().visitedPages).toEqual(['home', 'pluginView', 'marketplace'])
  })

  it('closing an inactive plugin leaves the current plugin selected', () => {
    const store = useAppStore.getState()
    store.setActivePluginId('retained-a')
    store.setActivePluginId('retained-b')
    store.setCurrentPage('pluginView')
    store.closePluginView('retained-a')
    expect(useAppStore.getState()).toMatchObject({
      activePluginId: 'retained-b',
      openedPluginIds: ['retained-b'],
      currentPage: 'pluginView'
    })
  })

  it('task entry selects tasks after the user previously selected runtime logs', () => {
    const store = useAppStore.getState()
    store.setActivityTab('logs')
    store.setCurrentPage('home')
    store.setCurrentPage('tasks')
    expect(useAppStore.getState()).toMatchObject({ currentPage: 'tasks', activityTab: 'tasks' })
  })

  it('keeps the last route origin across repeat selections and plugin changes', () => {
    const store = useAppStore.getState()
    store.setCurrentPage('marketplace')
    store.setCurrentPage('tasks')
    store.setCurrentPage('tasks')
    expect(useAppStore.getState().previousPage).toBe('marketplace')
    store.setActivePluginId('one')
    store.setCurrentPage('pluginView')
    expect(useAppStore.getState().previousPage).toBe('tasks')
    store.setActivePluginId('two')
    expect(useAppStore.getState().previousPage).toBe('pluginView')
    store.closePluginView('two')
    expect(useAppStore.getState()).toMatchObject({
      currentPage: 'home',
      previousPage: 'pluginView',
      openedPluginIds: ['one']
    })
  })

  it('follows navigation order vertically and plugin depth horizontally', () => {
    const pages = ['home', 'marketplace', 'tasks', 'settings'] as const
    for (let i = 0; i < pages.length - 1; i++) {
      expect(pageTransitionDirection(pages[i], pages[i + 1], 'menu')).toBe('up')
      expect(pageTransitionDirection(pages[i + 1], pages[i], 'menu')).toBe('down')
    }
    expect(pageTransitionDirection('home', 'settings', 'menu')).toBe('up')
    expect(pageTransitionDirection('settings', 'home', 'menu')).toBe('down')
    expect(pageTransitionDirection('tasks', 'pluginView', 'menu')).toBe('left')
    expect(pageTransitionDirection('pluginView', 'home', 'menu')).toBe('right')
    expect(pageTransitionDirection('pluginView', 'pluginView', 'menu')).toBe('left')
    expect(pageTransitionDirection('tasks', 'logs', 'menu')).toBe('none')
    expect(pageTransitionDirection('home', 'home', 'menu')).toBe('none')
    expect(pageTransitionDirection('home', 'marketplace', 'top')).toBe('left')
    expect(pageTransitionDirection('marketplace', 'home', 'top')).toBe('right')
  })
})

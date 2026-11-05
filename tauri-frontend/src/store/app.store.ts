import { create } from 'zustand'
import type { AppPage } from '../app-pages'
import { changePage, discardPageChange } from '../theme/page-handoff'

interface AppState {
  sidebarCollapsed: boolean
  toggleSidebar: () => void
  setSidebarCollapsed: (collapsed: boolean) => void

  currentPage: AppPage
  previousPage: AppPage
  visitedPages: AppPage[]
  setCurrentPage: (page: AppPage) => void

  activityTab: 'tasks' | 'logs'
  setActivityTab: (tab: 'tasks' | 'logs') => void

  activePluginId: string | null
  openedPluginIds: string[]
  setActivePluginId: (id: string | null) => void
  closePluginView: (id: string) => void

  pluginImportOpen: boolean
  setPluginImportOpen: (open: boolean) => void

  commandOpen: boolean
  setCommandOpen: (open: boolean) => void

  loading: boolean
  setLoading: (loading: boolean) => void
}

export const useAppStore = create<AppState>((set, get) => ({
  sidebarCollapsed: false,
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),

  currentPage: 'home',
  previousPage: 'home',
  visitedPages: ['home'],
  setCurrentPage: (page) =>
    changePage(
      () =>
        set((state) => ({
          currentPage: page,
          previousPage: page === state.currentPage ? state.previousPage : state.currentPage,
          ...(page === 'tasks' ? { activityTab: 'tasks' as const } : {}),
          visitedPages: state.visitedPages.includes(page)
            ? state.visitedPages
            : [...state.visitedPages, page]
        })),
      page !== get().currentPage
    ),

  activityTab: 'tasks',
  setActivityTab: (tab) => set({ activityTab: tab }),

  activePluginId: null,
  openedPluginIds: [],
  setActivePluginId: (id) => {
    const update = () =>
      set((state) => ({
        activePluginId: id,
        ...(state.currentPage === 'pluginView' && id !== state.activePluginId
          ? { previousPage: state.currentPage }
          : {}),
        openedPluginIds:
          id && !state.openedPluginIds.includes(id)
            ? [...state.openedPluginIds, id]
            : state.openedPluginIds
      }))
    if (get().currentPage === 'pluginView') changePage(update, id !== get().activePluginId)
    else update()
  },
  closePluginView: (id) => {
    if (get().activePluginId === id) discardPageChange()
    set((state) => ({
      openedPluginIds: state.openedPluginIds.filter((opened) => opened !== id),
      ...(state.activePluginId === id
        ? {
            activePluginId: null,
            previousPage:
              state.currentPage === 'pluginView' ? state.currentPage : state.previousPage,
            currentPage: state.currentPage === 'pluginView' ? 'home' : state.currentPage
          }
        : {})
    }))
  },

  pluginImportOpen: false,
  setPluginImportOpen: (open) => set({ pluginImportOpen: open }),

  commandOpen: false,
  setCommandOpen: (open) => set({ commandOpen: open }),

  loading: false,
  setLoading: (loading) => set({ loading })
}))

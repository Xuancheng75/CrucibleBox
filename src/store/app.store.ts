import { create } from 'zustand'
import type { AppPage } from '../app-pages'

interface AppState {
  sidebarCollapsed: boolean
  toggleSidebar: () => void
  setSidebarCollapsed: (collapsed: boolean) => void

  currentPage: AppPage
  setCurrentPage: (page: AppPage) => void

  activePluginId: string | null
  setActivePluginId: (id: string | null) => void

  pluginImportOpen: boolean
  setPluginImportOpen: (open: boolean) => void

  commandOpen: boolean
  setCommandOpen: (open: boolean) => void

  loading: boolean
  setLoading: (loading: boolean) => void
}

export const useAppStore = create<AppState>((set) => ({
  sidebarCollapsed: false,
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),

  currentPage: 'home',
  setCurrentPage: (page) => set({ currentPage: page }),

  activePluginId: null,
  setActivePluginId: (id) => set({ activePluginId: id }),

  pluginImportOpen: false,
  setPluginImportOpen: (open) => set({ pluginImportOpen: open }),

  commandOpen: false,
  setCommandOpen: (open) => set({ commandOpen: open }),

  loading: false,
  setLoading: (loading) => set({ loading })
}))

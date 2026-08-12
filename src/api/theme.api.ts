import type { ToolboxTheme } from '@shared/types/theme.types'

function getAPI() {
  if (!window.electronAPI) {
    throw new Error('electronAPI not available. Ensure preload script is loaded.')
  }
  return window.electronAPI
}

export const themeApi = {
  get: async (): Promise<ToolboxTheme | null> => {
    return getAPI().theme.get()
  },

  set: async (theme: ToolboxTheme): Promise<ToolboxTheme | null> => {
    return getAPI().theme.set(theme)
  },

  list: async (): Promise<ToolboxTheme[]> => {
    return getAPI().theme.list()
  },

  onChanged: (callback: (theme: ToolboxTheme) => void): (() => void) => {
    return getAPI().theme.onChanged(callback)
  }
}
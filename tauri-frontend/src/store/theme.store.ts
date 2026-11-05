import { create } from 'zustand'
import type { RendererThemeStyle, ToolboxTheme } from '../../../shared/types/theme.types'
import { DEFAULT_THEME } from '../../../shared/themes/presets'
import { themeApi } from '../api/theme.api'
import { prepareThemeStyle } from '../theme/prepare'

interface ThemeState {
  theme: ToolboxTheme
  style: RendererThemeStyle
  initialized: boolean
  init: () => Promise<void>
  setTheme: (theme: ToolboxTheme) => Promise<boolean>
}

export const useThemeStore = create<ThemeState>((set) => ({
  theme: DEFAULT_THEME,
  style: { version: 2, css: '', motion: 'off', layers: [] },
  initialized: false,

  init: async () => {
    const current = await themeApi.get().catch(() => null)
    set({
      theme: current || DEFAULT_THEME,
      initialized: true
    })
  },

  setTheme: async (theme) => {
    await prepareThemeStyle(theme)
    const applied = await themeApi.set(theme).catch(() => null)
    if (applied) {
      set({ theme })
      return true
    }
    return false
  }
}))

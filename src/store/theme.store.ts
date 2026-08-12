import { create } from 'zustand'
import type { ToolboxTheme } from '@shared/types/theme.types'
import { DEFAULT_THEME } from '@shared/themes/presets'
import { themeApi } from '../api/theme.api'

interface ThemeState {
  theme: ToolboxTheme
  initialized: boolean
  init: () => Promise<void>
}

export const useThemeStore = create<ThemeState>((set) => {
  let unsubscribe: (() => void) | null = null

  return {
    theme: DEFAULT_THEME,
    initialized: false,

    init: async () => {
      const current = await themeApi.get().catch(() => null)

      if (unsubscribe) {
        unsubscribe()
        unsubscribe = null
      }
      unsubscribe = themeApi.onChanged((next) => {
        set({ theme: next, initialized: true })
      })

      set({
        theme: current || DEFAULT_THEME,
        initialized: true
      })
    }
  }
})

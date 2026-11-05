import type { ToolboxTheme } from '../../../../../shared/types/theme.types'
import { normalizeTheme } from '../../../../../shared/themes/normalize'
import { DEFAULT_THEME, PRESET_THEMES } from '../../../../../shared/themes/presets'
import { useThemeStore } from '../../../store/theme.store'
import { prepareThemeStyle } from '../../../theme/prepare'
let holder: symbol | null = null
let queue: Promise<unknown> = Promise.resolve()
function enqueue<T>(operation: () => Promise<T>): Promise<T> {
  const next = queue.then(operation, operation)
  queue = next.then(
    () => undefined,
    () => undefined
  )
  return next
}
export function createNextThemeController() {
  const identity = Symbol()
  let active = true
  let visible = true
  let original: ToolboxTheme | null = null
  let preview: ToolboxTheme | null = null
  const rollback = () => {
    if (holder === identity) holder = null
    if (original && preview && useThemeStore.getState().theme === preview) {
      useThemeStore.setState({ theme: original })
      original = null
      preview = null
      return true
    }
    original = null
    preview = null
    return false
  }
  return {
    execute(operation: string, params: Record<string, unknown>): Promise<unknown> {
      return enqueue(async () => {
        if (!active) throw new Error('SESSION_DENIED')
        if (!visible && ['theme.preview', 'theme.commit', 'theme.set'].includes(operation))
          throw new Error('SESSION_DENIED')
        if (operation === 'theme.get') return useThemeStore.getState().theme
        if (operation === 'theme.list') return PRESET_THEMES
        if (operation === 'theme.rollback') return rollback()
        if (operation === 'theme.commit') {
          if (!preview || useThemeStore.getState().theme !== preview) {
            original = null
            preview = null
            if (holder === identity) holder = null
            return false
          }
          const applied = await useThemeStore.getState().setTheme(preview)
          if (applied) {
            original = null
            preview = null
            if (holder === identity) holder = null
          }
          return applied
        }
        const theme = normalizeTheme(params.theme)
        if (!theme) throw new Error('INVALID_REQUEST')
        if (operation === 'theme.set') {
          const applied = await useThemeStore.getState().setTheme(theme)
          if (applied) {
            original = null
            preview = null
            if (holder === identity) holder = null
          }
          return applied
        }
        if (operation === 'theme.preview') {
          if (holder !== null && holder !== identity) throw new Error('BUSY')
          await prepareThemeStyle(theme)
          if (!active || !visible) throw new Error('SESSION_DENIED')
          holder = identity
          if (!preview || useThemeStore.getState().theme !== preview)
            original = useThemeStore.getState().theme
          preview = theme
          useThemeStore.setState({ theme })
          return true
        }
        throw new Error('INVALID_REQUEST')
      })
    },
    setVisible(value: boolean) {
      visible = value
      if (!value) void enqueue(async () => rollback())
    },
    dispose() {
      active = false
      void enqueue(async () => {
        rollback()
      })
    }
  }
}

// Share the queue with in-flight preview/commit work so an explicit reset wins.
export function restoreDefaultTheme() {
  return enqueue(async () => {
    holder = null
    return useThemeStore.getState().setTheme(DEFAULT_THEME)
  })
}

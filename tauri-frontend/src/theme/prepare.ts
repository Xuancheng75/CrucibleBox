import { invoke } from '@tauri-apps/api/core'
import type {
  RendererThemeStyle,
  ThemeAsset,
  ToolboxTheme
} from '../../../shared/types/theme.types'

const prepared = new WeakMap<ToolboxTheme, Map<boolean, Promise<RendererThemeStyle>>>()

export function prepareThemeStyle(
  theme: ToolboxTheme,
  reduced = false
): Promise<RendererThemeStyle> {
  let modes = prepared.get(theme)
  if (!modes) {
    modes = new Map()
    prepared.set(theme, modes)
  }
  let result = modes.get(reduced)
  if (!result) {
    result = compile(theme, reduced)
    modes.set(reduced, result)
    void result.catch(() => modes.delete(reduced))
  }
  return result
}

async function compile(theme: ToolboxTheme, reduced = false): Promise<RendererThemeStyle> {
  if (!theme.design) return { version: 2, css: '', motion: 'off', layers: [] }
  const assets = theme.design.assets?.length
    ? await invoke<ThemeAsset[]>(
        'theme_assets_describe',
        { ids: theme.design.assets },
        { headers: { Origin: window.location.origin } }
      )
    : []
  const { compileThemeStyle } = await import('../../../shared/themes/style')
  return compileThemeStyle(theme, assets, reduced)
}

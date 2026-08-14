// theme 缓存（1.9.2-c）
// PluginFrameBridge 的 theme.get 是同步调用（this.options.getTheme()），而 settings
// 持久化是 async invoke——用模块级缓存桥接：挂载时 async 载入 settings，读走缓存，
// 写同步更新缓存 + async 持久化（对等 Electron 侧 zustand store 的角色）。
import { invoke } from '@tauri-apps/api/core'
import type { ToolboxTheme } from '../../shared/types/theme.types'
import { DEFAULT_THEME, PRESET_THEMES } from '../../shared/themes/presets'

const THEME_SETTING_KEY = 'theme'

let cached: ToolboxTheme | null = null
let loaded = false

function parseTheme(raw: string | null): ToolboxTheme | null {
  if (raw === null) return null
  try {
    const parsed = JSON.parse(raw) as unknown
    if (parsed && typeof parsed === 'object') return parsed as ToolboxTheme
  } catch {
    return null
  }
  return null
}

/** 启动时载入持久化主题（返回当前主题）。幂等：只加载一次。 */
export async function loadTheme(): Promise<ToolboxTheme> {
  if (!loaded) {
    loaded = true
    try {
      const raw = await invoke<string | null>('settings_get', { key: THEME_SETTING_KEY })
      cached = parseTheme(raw)
    } catch {
      cached = null
    }
  }
  return cached ?? DEFAULT_THEME
}

/** 同步读当前主题（PluginFrameBridge theme.get 需要同步返回值）。 */
export function getThemeSync(): ToolboxTheme | null {
  return cached
}

/** 内置主题列表（静态数据，前端直读，不跨 Rust 边界）。 */
export function listThemes(): ToolboxTheme[] {
  return PRESET_THEMES
}

/** 应用主题：更新缓存 + async 持久化。返回应用后的主题。 */
export async function setTheme(theme: ToolboxTheme): Promise<ToolboxTheme | null> {
  cached = theme
  try {
    await invoke('settings_set', { key: THEME_SETTING_KEY, value: JSON.stringify(theme) })
  } catch (e) {
    console.error('[theme] persist failed', e)
  }
  return theme
}

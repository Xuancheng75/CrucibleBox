// 主题 API（1.9.3）
// Tauri 侧主题持久化走 settings（白名单 key 'theme'），同步读缓存由 themeCache 提供。
// 对等 Electron 版 src/api/theme.api.ts 的 get/set/list/onChanged 语义：
//   - get()  → 载入持久化主题（幂等，仅首次真正读后端）
//   - set()  → 更新缓存 + 持久化，返回应用后的主题
//   - list() → 内置主题列表（静态数据，前端直读）
//   - onChanged() → Tauri 后端当前无 theme:changed 事件；返回 noop 以保持调用方兼容
import type { ToolboxTheme } from '../../../shared/types/theme.types'
import { loadTheme, getThemeSync, listThemes, setTheme as persistTheme } from '../themeCache'

export const themeApi = {
  get: async (): Promise<ToolboxTheme | null> => {
    const theme = await loadTheme()
    return theme
  },

  set: async (theme: ToolboxTheme): Promise<ToolboxTheme | null> => {
    return persistTheme(theme)
  },

  list: async (): Promise<ToolboxTheme[]> => {
    return listThemes()
  },

  /** 同步读当前主题（供 PluginFrameBridge theme.get 使用） */
  getSync: (): ToolboxTheme | null => getThemeSync(),

  /** Tauri 后端暂无 theme:changed 事件；返回 noop 保持调用方兼容 */
  onChanged: (_callback: (theme: ToolboxTheme) => void): (() => void) => {
    return () => undefined
  }
}
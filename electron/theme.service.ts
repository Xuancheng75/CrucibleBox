// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { BrowserWindow, ipcMain } from 'electron'
import { IpcChannel } from '@shared/types/ipc.types'
import type { ToolboxTheme } from '@shared/types/theme.types'
import { DEFAULT_THEME, PRESET_THEMES, getPresetTheme } from '@shared/themes/presets'
import { normalizeTheme } from '@shared/themes/normalize'
import { SettingsRepository } from '@database/repositories/settings.repository'
import { assertTrustedSender } from './ipc/ipcGuard'

export const THEME_SETTING_KEY = 'theme'

export function getCurrentTheme(): ToolboxTheme {
  const raw = SettingsRepository.get(THEME_SETTING_KEY)
  if (!raw) return DEFAULT_THEME
  try {
    const parsed = JSON.parse(raw) as unknown
    if (parsed && typeof parsed === 'object') {
      const parsedId = (parsed as { id?: unknown }).id
      const preset = typeof parsedId === 'string' ? getPresetTheme(parsedId) : undefined
      return preset ?? normalizeTheme(parsed) ?? DEFAULT_THEME
    }
  } catch {
    // fall through to default
  }
  return DEFAULT_THEME
}

export function setCurrentTheme(theme: ToolboxTheme): ToolboxTheme {
  const sanitized = sanitizeTheme(getPresetTheme(theme.id) ?? theme)
  SettingsRepository.set(THEME_SETTING_KEY, JSON.stringify(sanitized))
  return sanitized
}

export function sanitizeTheme(theme: ToolboxTheme): ToolboxTheme {
  const normalized = normalizeTheme(theme)
  if (!normalized) throw new Error('Invalid theme')
  return normalized
}

export function broadcastTheme(theme: ToolboxTheme): void {
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.isDestroyed()) {
      win.webContents.send(IpcChannel.ThemeChanged, theme)
    }
  }
}

export function registerThemeIpc(
  broadcast: (theme: ToolboxTheme) => void = broadcastTheme,
  onThemeChanged?: (theme: ToolboxTheme) => void
): void {
  ipcMain.handle(IpcChannel.ThemeGet, (event) => {
    assertTrustedSender(event)
    try {
      return getCurrentTheme()
    } catch (err) {
      console.error('[IPC] ThemeGet error:', err)
      return DEFAULT_THEME
    }
  })

  ipcMain.handle(IpcChannel.ThemeSet, (event, next: ToolboxTheme) => {
    assertTrustedSender(event)
    try {
      const theme = setCurrentTheme(next)
      broadcast(theme)
      onThemeChanged?.(theme)
      return theme
    } catch (err) {
      console.error('[IPC] ThemeSet error:', err)
      return null
    }
  })

  ipcMain.handle(IpcChannel.ThemeList, (event) => {
    assertTrustedSender(event)
    try {
      return PRESET_THEMES
    } catch (err) {
      console.error('[IPC] ThemeList error:', err)
      return []
    }
  })
}

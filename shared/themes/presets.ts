import type { ThemeTokens, ToolboxTheme } from '../types/theme.types'

const DEFAULT_FONT =
  "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, 'Noto Sans', sans-serif"

export function createLightTokens(
  primary: string,
  primaryHover: string,
  primaryBg: string
): ThemeTokens {
  return {
    colorBg: '#fafafa',
    colorBgLayout: '#f5f6fa',
    colorBgContainer: '#ffffff',
    colorBgElevated: '#ffffff',
    colorPrimary: primary,
    colorPrimaryHover: primaryHover,
    colorPrimaryBg: primaryBg,
    colorText: '#333333',
    colorTextSecondary: '#888888',
    colorTextTertiary: '#c0c0c0',
    colorBorder: '#e8e8e8',
    colorBorderSecondary: '#f0f0f0',
    colorSuccess: '#52c41a',
    colorSuccessBg: '#f6ffed',
    colorWarning: '#faad14',
    colorWarningBg: '#fffbe6',
    colorError: '#ff4d4f',
    colorErrorBg: '#fff2f0',
    colorLink: '#6366f1',
    borderRadius: 10,
    fontFamily: DEFAULT_FONT
  }
}

export function createDarkTokens(
  primary: string,
  primaryHover: string,
  primaryBg: string
): ThemeTokens {
  return {
    colorBg: '#0a0c10',
    colorBgLayout: '#08090d',
    colorBgContainer: '#171a21',
    colorBgElevated: '#1e2230',
    colorPrimary: primary,
    colorPrimaryHover: primaryHover,
    colorPrimaryBg: primaryBg,
    colorText: '#e8e8e8',
    colorTextSecondary: '#9c9c9c',
    colorTextTertiary: '#6b6b6b',
    colorBorder: '#2a2e3a',
    colorBorderSecondary: '#232736',
    colorSuccess: '#49aa19',
    colorSuccessBg: '#162312',
    colorWarning: '#d89614',
    colorWarningBg: '#2b2111',
    colorError: '#dc4446',
    colorErrorBg: '#2a1215',
    colorLink: '#818cf8',
    borderRadius: 10,
    fontFamily: DEFAULT_FONT
  }
}

function createBlueWorkbenchTokens(): ThemeTokens {
  return {
    ...createLightTokens('#2f78c2', '#2068ad', '#dfedff'),
    colorBg: '#f7fbff',
    colorBgLayout: '#f2f7fd',
    colorBgContainer: '#ffffff',
    colorBgElevated: '#ffffff',
    colorText: '#1b3552',
    colorTextSecondary: '#607f9e',
    colorTextTertiary: '#829db8',
    colorBorder: '#cbdeef',
    colorBorderSecondary: '#dfeaf5',
    colorLink: '#2f78c2',
    borderRadius: 10
  }
}

export const PRESET_THEMES: ToolboxTheme[] = [
  {
    id: 'light',
    name: '蓝白工作台',
    mode: 'light',
    tokens: createBlueWorkbenchTokens()
  },
  {
    id: 'dark',
    name: '深色',
    mode: 'dark',
    tokens: createDarkTokens('#818cf8', '#a5b4fc', '#2a2b52')
  }
]

export const DEFAULT_THEME: ToolboxTheme = PRESET_THEMES[0]

export function getPresetTheme(id: string): ToolboxTheme | undefined {
  return PRESET_THEMES.find((theme) => theme.id === id)
}

export function isPresetTheme(id: string): boolean {
  return getPresetTheme(id) !== undefined
}

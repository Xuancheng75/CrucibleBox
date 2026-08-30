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

const CYBER_FONT =
  "'Rajdhani', 'Teko', 'Bahnschrift', 'Microsoft YaHei UI', 'Microsoft YaHei', 'Noto Sans SC', 'Segoe UI', -apple-system, BlinkMacSystemFont, monospace"

export function createCyberTokens(): ThemeTokens {
  return {
    colorBg: '#0a0e14',
    colorBgLayout: '#060a10',
    colorBgContainer: '#0d121a',
    colorBgElevated: '#131a24',
    colorPrimary: '#00e5ff',
    colorPrimaryHover: '#66f2ff',
    colorPrimaryBg: '#06212e',
    colorText: '#d9e4e8',
    colorTextSecondary: '#8aa2ad',
    colorTextTertiary: '#4d6a75',
    colorBorder: '#1e3a4d',
    colorBorderSecondary: '#14232e',
    colorSuccess: '#00ff9d',
    colorSuccessBg: '#0a2b1f',
    colorWarning: '#fce205',
    colorWarningBg: '#2e2a08',
    colorError: '#ff003c',
    colorErrorBg: '#330414',
    colorLink: '#00e5ff',
    borderRadius: 8,
    fontFamily: CYBER_FONT
  }
}

const NEON_DISTRICT_FONT =
  "'Bahnschrift', 'Rajdhani', 'Microsoft YaHei UI', 'Microsoft YaHei', 'Noto Sans SC', 'Segoe UI', sans-serif"

export function createNeonDistrictTokens(): ThemeTokens {
  return {
    colorBg: '#080d19',
    colorBgLayout: '#040711',
    colorBgContainer: '#0a1220',
    colorBgElevated: '#101c2d',
    colorPrimary: '#00e5ff',
    colorPrimaryHover: '#7df6ff',
    colorPrimaryBg: '#052b38',
    colorText: '#e6f7fa',
    colorTextSecondary: '#91adb6',
    colorTextTertiary: '#52727c',
    colorBorder: '#175064',
    colorBorderSecondary: '#102f3d',
    colorSuccess: '#39ff88',
    colorSuccessBg: '#09281c',
    colorWarning: '#fce205',
    colorWarningBg: '#302b06',
    colorError: '#ff2b78',
    colorErrorBg: '#33091d',
    colorLink: '#00e5ff',
    borderRadius: 2,
    fontFamily: NEON_DISTRICT_FONT
  }
}

export const PRESET_THEMES: ToolboxTheme[] = [
  {
    id: 'light',
    name: '亮色（默认）',
    mode: 'light',
    tokens: createLightTokens('#6366f1', '#818cf8', '#eef2ff')
  },
  {
    id: 'dark',
    name: '深色',
    mode: 'dark',
    tokens: createDarkTokens('#818cf8', '#a5b4fc', '#2a2b52')
  },
  {
    id: 'leaf',
    name: '清新绿',
    mode: 'light',
    tokens: createLightTokens('#52c41a', '#73d13d', '#f6ffed')
  },
  {
    id: 'ocean',
    name: '海洋蓝',
    mode: 'dark',
    tokens: createDarkTokens('#1677ff', '#4096ff', '#0d2137')
  },
  {
    id: 'cyber',
    name: '科幻面板',
    mode: 'dark',
    tokens: createCyberTokens()
  },
  {
    id: 'neon-district',
    name: '零号城区',
    mode: 'dark',
    tokens: createNeonDistrictTokens()
  },
  {
    id: 'warm-sun',
    name: '暖阳米白',
    mode: 'light',
    tokens: createLightTokens('#d97706', '#f59e0b', '#fff7ed')
  },
  {
    id: 'sakura',
    name: '樱花粉',
    mode: 'light',
    tokens: createLightTokens('#db2777', '#ec4899', '#fdf2f8')
  },
  {
    id: 'amber-autumn',
    name: '琥珀秋日',
    mode: 'light',
    tokens: createLightTokens('#b45309', '#d97706', '#fffbeb')
  },
  {
    id: 'mono',
    name: '极简黑白',
    mode: 'light',
    tokens: createLightTokens('#374151', '#4b5563', '#f3f4f6')
  },
  {
    id: 'mist-blue',
    name: '雾霾蓝',
    mode: 'light',
    tokens: createLightTokens('#2563eb', '#3b82f6', '#eff6ff')
  },
  {
    id: 'lavender',
    name: '薰衣草',
    mode: 'light',
    tokens: createLightTokens('#7c3aed', '#8b5cf6', '#f5f3ff')
  },
  {
    id: 'ink-oriental',
    name: '墨色东方',
    mode: 'dark',
    tokens: createDarkTokens('#ef4444', '#f87171', '#3b1518')
  },
  {
    id: 'aurora-night',
    name: '极光夜色',
    mode: 'dark',
    tokens: createDarkTokens('#14b8a6', '#2dd4bf', '#0c2928')
  }
]

export const DEFAULT_THEME: ToolboxTheme = PRESET_THEMES[0]

export function getPresetTheme(id: string): ToolboxTheme | undefined {
  return PRESET_THEMES.find((theme) => theme.id === id)
}

export function isPresetTheme(id: string): boolean {
  return getPresetTheme(id) !== undefined
}

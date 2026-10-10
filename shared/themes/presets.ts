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

function createEditorialTokens(): ThemeTokens {
  return {
    ...createLightTokens('#0f766e', '#14b8a6', '#ccfbf1'),
    colorBg: '#f4f7f6',
    colorBgLayout: '#e9f0ee',
    colorBgContainer: '#fffdf8',
    colorBgElevated: '#ffffff',
    colorText: '#173b3a',
    colorTextSecondary: '#52706d',
    colorBorder: '#c8dad5',
    colorBorderSecondary: '#dce9e5',
    borderRadius: 4,
    fontFamily: "Georgia, 'Noto Serif SC', serif"
  }
}

function createClayTokens(): ThemeTokens {
  return {
    ...createLightTokens('#b45309', '#c2410c', '#ffedd5'),
    colorBg: '#fbf3ea',
    colorBgLayout: '#f2e4d5',
    colorBgContainer: '#fffaf4',
    colorBgElevated: '#ffffff',
    colorText: '#4b2e1f',
    colorTextSecondary: '#89634d',
    colorBorder: '#e2c1a5',
    colorBorderSecondary: '#efd9c4',
    borderRadius: 18,
    fontFamily: "'Avenir Next', 'Noto Sans SC', sans-serif"
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
  },
  {
    id: 'cyber',
    name: '科幻面板',
    mode: 'dark',
    tokens: createCyberTokens()
  },
  {
    id: 'neon-district',
    name: '像素街机',
    mode: 'dark',
    // Keep the structural token contract stable for existing configurations;
    // the preset-specific arcade surface treatment lives in theme-presets.css.
    tokens: createNeonDistrictTokens()
  },
  {
    id: 'warm-sun',
    name: '陶土工坊',
    mode: 'light',
    tokens: createClayTokens()
  },
  {
    id: 'editorial-paper',
    name: '纸张编辑',
    mode: 'light',
    tokens: createEditorialTokens()
  }
]

export const DEFAULT_THEME: ToolboxTheme = PRESET_THEMES[0]

export function getPresetTheme(id: string): ToolboxTheme | undefined {
  return PRESET_THEMES.find((theme) => theme.id === id)
}

export function isPresetTheme(id: string): boolean {
  return getPresetTheme(id) !== undefined
}

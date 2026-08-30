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

function createTerminalTokens(): ThemeTokens {
  return {
    ...createDarkTokens('#a3e635', '#bef264', '#1a2e05'),
    colorBg: '#080b08',
    colorBgLayout: '#0d120d',
    colorBgContainer: '#111a11',
    colorBgElevated: '#182418',
    colorText: '#e7f7e7',
    colorTextSecondary: '#93b493',
    colorBorder: '#294229',
    colorBorderSecondary: '#1d301d',
    borderRadius: 2,
    fontFamily: "'Cascadia Code', Consolas, monospace"
  }
}

function createForestTokens(): ThemeTokens {
  return {
    ...createLightTokens('#2f855a', '#276749', '#e6fffa'),
    colorBg: '#f2f7f1',
    colorBgLayout: '#e4efe3',
    colorBgContainer: '#fbfffa',
    colorBgElevated: '#ffffff',
    colorText: '#1f3b2d',
    colorTextSecondary: '#527a60',
    colorBorder: '#b9d4bd',
    colorBorderSecondary: '#d7e8d9',
    borderRadius: 14,
    fontFamily: "'Trebuchet MS', 'Noto Sans SC', sans-serif"
  }
}

function createBlueprintTokens(): ThemeTokens {
  return {
    ...createLightTokens('#155eaa', '#1d4ed8', '#dbeafe'),
    colorBg: '#eef4fb',
    colorBgLayout: '#e2ecf8',
    colorBgContainer: '#f8fbff',
    colorBgElevated: '#ffffff',
    colorText: '#16324f',
    colorTextSecondary: '#52708f',
    colorBorder: '#abc3dc',
    colorBorderSecondary: '#d5e2ef',
    borderRadius: 3,
    fontFamily: "'Courier New', 'Noto Sans SC', monospace"
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

function createArtDecoTokens(): ThemeTokens {
  return {
    ...createLightTokens('#9f1239', '#be123c', '#ffe4e6'),
    colorBg: '#fff7fb',
    colorBgLayout: '#f7e8f0',
    colorBgContainer: '#fffafd',
    colorBgElevated: '#ffffff',
    colorText: '#4a1830',
    colorTextSecondary: '#92516d',
    colorBorder: '#e7b9cc',
    colorBorderSecondary: '#f3d9e5',
    borderRadius: 0,
    fontFamily: "'Century Gothic', 'Noto Sans SC', sans-serif"
  }
}

function createIndustrialTokens(): ThemeTokens {
  return {
    ...createLightTokens('#b45309', '#92400e', '#fef3c7'),
    colorBg: '#f1f3f4',
    colorBgLayout: '#e2e5e7',
    colorBgContainer: '#fafafa',
    colorBgElevated: '#ffffff',
    colorText: '#202428',
    colorTextSecondary: '#5f6870',
    colorBorder: '#abb4bb',
    colorBorderSecondary: '#d1d6da',
    borderRadius: 2,
    fontFamily: "'Arial Narrow', 'Noto Sans SC', sans-serif"
  }
}

function createSwissTokens(): ThemeTokens {
  return {
    ...createLightTokens('#111827', '#374151', '#f3f4f6'),
    colorBg: '#ffffff',
    colorBgLayout: '#f2f2f2',
    colorBgContainer: '#ffffff',
    colorBgElevated: '#ffffff',
    colorText: '#111111',
    colorTextSecondary: '#4b4b4b',
    colorBorder: '#111111',
    colorBorderSecondary: '#d1d1d1',
    borderRadius: 0,
    fontFamily: "Helvetica, Arial, 'Noto Sans SC', sans-serif"
  }
}

function createAccessibleTokens(): ThemeTokens {
  return {
    ...createLightTokens('#005fcc', '#004a99', '#d9ecff'),
    colorBg: '#ffffff',
    colorBgLayout: '#f4f7fb',
    colorBgContainer: '#ffffff',
    colorBgElevated: '#ffffff',
    colorText: '#101820',
    colorTextSecondary: '#324a5f',
    colorTextTertiary: '#536b80',
    colorBorder: '#486581',
    colorBorderSecondary: '#9fb3c8',
    borderRadius: 6,
    fontFamily: "Verdana, 'Noto Sans SC', sans-serif"
  }
}

function createBauhausTokens(): ThemeTokens {
  return {
    ...createLightTokens('#c026d3', '#a21caf', '#fae8ff'),
    colorBg: '#fffdf5',
    colorBgLayout: '#f7f0dc',
    colorBgContainer: '#fffefb',
    colorBgElevated: '#ffffff',
    colorText: '#241b35',
    colorTextSecondary: '#675276',
    colorBorder: '#d9c8e6',
    colorBorderSecondary: '#eee4f3',
    borderRadius: 24,
    fontFamily: "'Futura', 'Century Gothic', 'Noto Sans SC', sans-serif"
  }
}

function createInkTokens(): ThemeTokens {
  return {
    ...createDarkTokens('#f59e0b', '#fbbf24', '#3d2b09'),
    colorBg: '#171717',
    colorBgLayout: '#101010',
    colorBgContainer: '#222222',
    colorBgElevated: '#2d2d2d',
    colorText: '#f5f5f4',
    colorTextSecondary: '#b8b5ad',
    colorBorder: '#514b43',
    colorBorderSecondary: '#37332e',
    borderRadius: 4,
    fontFamily: "'Noto Serif SC', SimSun, serif"
  }
}

function createAuroraTokens(): ThemeTokens {
  return {
    ...createDarkTokens('#22d3ee', '#67e8f9', '#073642'),
    colorBg: '#0a1020',
    colorBgLayout: '#050a14',
    colorBgContainer: '#101a2c',
    colorBgElevated: '#182944',
    colorText: '#e5f9ff',
    colorTextSecondary: '#98b7c7',
    colorBorder: '#285878',
    colorBorderSecondary: '#1a3b55',
    colorSuccess: '#a3e635',
    colorSuccessBg: '#22340a',
    borderRadius: 16,
    fontFamily: "'Segoe UI Variable', 'Noto Sans SC', sans-serif"
  }
}

function createConsoleTokens(): ThemeTokens {
  return {
    ...createTerminalTokens(),
    colorBg: '#050706',
    colorBgLayout: '#020302',
    colorBgContainer: '#0b100c',
    colorBgElevated: '#111a12',
    colorPrimary: '#00d26a',
    colorPrimaryHover: '#36f28f',
    colorPrimaryBg: '#062b16',
    colorText: '#d5ffe3',
    colorTextSecondary: '#7cc493',
    colorBorder: '#1e5b32',
    colorBorderSecondary: '#123b21',
    borderRadius: 0
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
    name: '森林木屋',
    mode: 'light',
    tokens: createForestTokens()
  },
  {
    id: 'ocean',
    name: '蓝图工坊',
    mode: 'dark',
    tokens: createBlueprintTokens()
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
    id: 'sakura',
    name: '装饰艺术',
    mode: 'light',
    tokens: createArtDecoTokens()
  },
  {
    id: 'amber-autumn',
    name: '工业仪表',
    mode: 'light',
    tokens: createIndustrialTokens()
  },
  {
    id: 'mono',
    name: '瑞士网格',
    mode: 'light',
    tokens: createSwissTokens()
  },
  {
    id: 'mist-blue',
    name: '高对比无障碍',
    mode: 'light',
    tokens: createAccessibleTokens()
  },
  {
    id: 'lavender',
    name: '包豪斯几何',
    mode: 'light',
    tokens: createBauhausTokens()
  },
  {
    id: 'ink-oriental',
    name: '水墨东方',
    mode: 'dark',
    tokens: createInkTokens()
  },
  {
    id: 'aurora-night',
    name: '极光玻璃',
    mode: 'dark',
    tokens: createAuroraTokens()
  },
  {
    id: 'editorial-paper',
    name: '纸张编辑',
    mode: 'light',
    tokens: createEditorialTokens()
  },
  {
    id: 'terminal-green',
    name: '终端控制台',
    mode: 'dark',
    tokens: createConsoleTokens()
  }
]

export const DEFAULT_THEME: ToolboxTheme = PRESET_THEMES[0]

export function getPresetTheme(id: string): ToolboxTheme | undefined {
  return PRESET_THEMES.find((theme) => theme.id === id)
}

export function isPresetTheme(id: string): boolean {
  return getPresetTheme(id) !== undefined
}

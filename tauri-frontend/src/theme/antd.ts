import type { ThemeConfig } from 'antd'
import { theme as antdTheme } from 'antd'
import type { ToolboxTheme } from '../../../shared/types/theme.types'

export function antdThemeConfig(toolboxTheme: ToolboxTheme): ThemeConfig {
  const tokens = toolboxTheme.tokens
  return {
    algorithm:
      toolboxTheme.mode === 'dark'
        ? antdTheme.darkAlgorithm
        : antdTheme.defaultAlgorithm,
    token: {
      colorPrimary: tokens.colorPrimary,
      colorInfo: tokens.colorLink,
      colorSuccess: tokens.colorSuccess,
      colorWarning: tokens.colorWarning,
      colorError: tokens.colorError,
      colorBgLayout: tokens.colorBgLayout,
      colorBgContainer: tokens.colorBgContainer,
      colorBgElevated: tokens.colorBgElevated,
      colorText: tokens.colorText,
      colorTextSecondary: tokens.colorTextSecondary,
      colorTextTertiary: tokens.colorTextTertiary,
      colorBorder: tokens.colorBorder,
      colorBorderSecondary: tokens.colorBorderSecondary,
      borderRadius: tokens.borderRadius,
      fontFamily: tokens.fontFamily
    }
  }
}
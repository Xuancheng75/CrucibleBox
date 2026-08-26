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
    },
    components: {
      Modal: {
        // Modal 面板（header/content）与 .ob-modal-surface 内容面统一用
        // colorBgContainer：默认 headerBg/contentBg 是 colorBgElevated，与主题 CSS
        // 强制的 container 背景形成色差长条（Bug D）。footer 保持透明叠在 content 上。
        headerBg: tokens.colorBgContainer,
        contentBg: tokens.colorBgContainer
      }
    }
  }
}
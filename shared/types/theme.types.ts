export type ThemeMode = 'light' | 'dark'
export type {
  ThemeDesign,
  ThemeLayer,
  ThemeAsset,
  RendererThemeStyle
} from '../../packages/cruciblebox-next2-api/src/generated.mjs'

export interface ThemeTokens {
  colorBg: string
  colorBgLayout: string
  colorBgContainer: string
  colorBgElevated: string
  colorPrimary: string
  colorPrimaryHover: string
  colorPrimaryBg: string
  colorText: string
  colorTextSecondary: string
  colorTextTertiary: string
  colorBorder: string
  colorBorderSecondary: string
  colorSuccess: string
  colorSuccessBg: string
  colorWarning: string
  colorWarningBg: string
  colorError: string
  colorErrorBg: string
  colorLink: string
  borderRadius: number
  fontFamily: string
}

export interface ToolboxTheme {
  id: string
  name: string
  mode: ThemeMode
  tokens: ThemeTokens
  design?: import('../../packages/cruciblebox-next2-api/src/generated.mjs').ThemeDesign
}

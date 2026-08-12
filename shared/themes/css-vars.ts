import type { ThemeTokens, ToolboxTheme } from '../types/theme.types'

const TOKEN_CSS_VARS: Record<keyof ThemeTokens, string> = {
  colorBg: '--ob-color-bg',
  colorBgLayout: '--ob-color-bg-layout',
  colorBgContainer: '--ob-color-bg-container',
  colorBgElevated: '--ob-color-bg-elevated',
  colorPrimary: '--ob-color-primary',
  colorPrimaryHover: '--ob-color-primary-hover',
  colorPrimaryBg: '--ob-color-primary-bg',
  colorText: '--ob-color-text',
  colorTextSecondary: '--ob-color-text-secondary',
  colorTextTertiary: '--ob-color-text-tertiary',
  colorBorder: '--ob-color-border',
  colorBorderSecondary: '--ob-color-border-secondary',
  colorSuccess: '--ob-color-success',
  colorSuccessBg: '--ob-color-success-bg',
  colorWarning: '--ob-color-warning',
  colorWarningBg: '--ob-color-warning-bg',
  colorError: '--ob-color-error',
  colorErrorBg: '--ob-color-error-bg',
  colorLink: '--ob-color-link',
  borderRadius: '--ob-radius',
  fontFamily: '--ob-font-family'
}

export function themeToCssVars(theme: ToolboxTheme): Record<string, string> {
  const vars: Record<string, string> = {
    '--ob-mode': theme.mode,
    '--ob-theme-id': theme.id,
    '--ob-color-success-border': theme.tokens.colorSuccess,
    '--ob-color-warning-border': theme.tokens.colorWarning,
    '--ob-color-error-border': theme.tokens.colorError
  }
  for (const [key, varName] of Object.entries(TOKEN_CSS_VARS)) {
    const value = theme.tokens[key as keyof ThemeTokens]
    const rendered = typeof value === 'number' ? `${value}px` : value
    vars[varName] = rendered
    vars[`--ob-${key}`] = rendered
  }
  return vars
}

export function getThemeCssVarKeys(): string[] {
  return [
    '--ob-mode',
    '--ob-theme-id',
    '--ob-color-success-border',
    '--ob-color-warning-border',
    '--ob-color-error-border',
    ...Object.values(TOKEN_CSS_VARS),
    ...Object.keys(TOKEN_CSS_VARS).map((key) => `--ob-${key}`)
  ]
}

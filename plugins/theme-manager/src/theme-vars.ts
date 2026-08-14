/**
 * @openbox/ui 内联副本（1.9.0 插件 SDK 独立化）。
 * 与宿主 shared/themes/css-vars.ts 单源对齐：CSS 变量命名
 * （--ob-color-*、--ob-radius、--ob-font-family）与原始语义保持逐字一致。
 */
export type CrucibleBoxColorToken =
  | 'bg'
  | 'bg-layout'
  | 'bg-container'
  | 'bg-elevated'
  | 'primary'
  | 'primary-hover'
  | 'primary-bg'
  | 'text'
  | 'text-secondary'
  | 'text-tertiary'
  | 'border'
  | 'border-secondary'
  | 'success'
  | 'success-bg'
  | 'warning'
  | 'warning-bg'
  | 'error'
  | 'error-bg'
  | 'link'

export function themeColorVar(token: CrucibleBoxColorToken, fallback: string): string {
  return `var(--ob-color-${token}, ${fallback})`
}

export function themeRadiusVar(fallback = '8px'): string {
  return `var(--ob-radius, ${fallback})`
}

export function themeFontFamilyVar(fallback = 'sans-serif'): string {
  return `var(--ob-font-family, ${fallback})`
}

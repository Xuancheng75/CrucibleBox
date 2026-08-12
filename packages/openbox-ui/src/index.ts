export type OpenBoxColorToken =
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

export function themeColorVar(token: OpenBoxColorToken, fallback: string): string {
  return `var(--ob-color-${token}, ${fallback})`
}

export function themeRadiusVar(fallback = '8px'): string {
  return `var(--ob-radius, ${fallback})`
}

export function themeFontFamilyVar(fallback = 'sans-serif'): string {
  return `var(--ob-font-family, ${fallback})`
}

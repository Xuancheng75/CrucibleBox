import type { ThemeMode, ThemeTokens, ToolboxTheme } from '../types/theme.types'
import { createDarkTokens, createLightTokens } from './presets'

const HEX_COLOR = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/
const RGB_COLOR = /^rgba?\(\s*\d{1,3}\s*,\s*\d{1,3}\s*,\s*\d{1,3}\s*(?:,\s*[\d.]+\s*)?\)$/
const FONT_CHARS = /^[\w\s.,'"(){}!@#$%^&*+/\\:=;-]+$/
const TOKEN_KEYS = [
  'colorBg',
  'colorBgLayout',
  'colorBgContainer',
  'colorBgElevated',
  'colorPrimary',
  'colorPrimaryHover',
  'colorPrimaryBg',
  'colorText',
  'colorTextSecondary',
  'colorTextTertiary',
  'colorBorder',
  'colorBorderSecondary',
  'colorSuccess',
  'colorSuccessBg',
  'colorWarning',
  'colorWarningBg',
  'colorError',
  'colorErrorBg',
  'colorLink'
] as const satisfies readonly (keyof ThemeTokens)[]

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function defaultTokens(mode: ThemeMode): ThemeTokens {
  return mode === 'dark'
    ? createDarkTokens('#818cf8', '#a5b4fc', '#2a2b52')
    : createLightTokens('#6366f1', '#818cf8', '#eef2ff')
}

export function normalizeTheme(value: unknown): ToolboxTheme | null {
  if (!isRecord(value) || !isRecord(value.tokens)) return null
  if (value.mode !== 'light' && value.mode !== 'dark') return null
  if (typeof value.id !== 'string' || value.id.length < 1 || value.id.length > 64) return null
  if (typeof value.name !== 'string' || value.name.length < 1 || value.name.length > 80) return null

  const tokens = defaultTokens(value.mode)
  for (const key of TOKEN_KEYS) {
    const candidate = value.tokens[key]
    if (typeof candidate === 'string' && candidate.length <= 128) {
      if (HEX_COLOR.test(candidate) || RGB_COLOR.test(candidate)) tokens[key] = candidate
    }
  }

  const radius = value.tokens.borderRadius
  if (typeof radius === 'number' && Number.isFinite(radius)) {
    tokens.borderRadius = Math.max(0, Math.min(32, Math.round(radius)))
  }

  const font = value.tokens.fontFamily
  if (
    typeof font === 'string' &&
    font.length > 0 &&
    font.length <= 128 &&
    FONT_CHARS.test(font) &&
    !font.includes('url(') &&
    !font.includes(';')
  ) {
    tokens.fontFamily = font
  }

  return { id: value.id, name: value.name, mode: value.mode, tokens }
}

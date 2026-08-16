import { theme } from 'antd'

function hashName(name: string): number {
  let hash = 0
  for (let i = 0; i < name.length; i += 1) {
    hash = (hash * 31 + name.charCodeAt(i)) >>> 0
  }
  return hash
}

function initialOf(name: string): string {
  const trimmed = name.trim()
  if (!trimmed) return '?'
  return trimmed[0].toUpperCase()
}

function darken(hex: string, amount: number): string {
  const value = hex.replace('#', '')
  const full = value.length === 3 ? value.split('').map((c) => c + c).join('') : value
  const num = parseInt(full, 16)
  const r = Math.max(0, Math.min(255, Math.round(((num >> 16) & 255) * amount)))
  const g = Math.max(0, Math.min(255, Math.round(((num >> 8) & 255) * amount)))
  const b = Math.max(0, Math.min(255, Math.round((num & 255) * amount)))
  return `rgb(${r}, ${g}, ${b})`
}

interface PluginGlyphProps {
  name: string
  icon?: string
  size?: number
  radius?: number
  fontSize?: number
}

export default function PluginGlyph({ name, icon, size = 56, radius = 14, fontSize }: PluginGlyphProps) {
  const { token } = theme.useToken()
  const primary = token.colorPrimary

  const hash = hashName(name)
  const angle = 100 + (hash % 80)
  const tone = 0.55 + ((hash % 20) / 100)
  const deep = darken(primary, tone)
  const gradient = `linear-gradient(${angle}deg, ${primary}, ${deep})`

  if (icon) {
    return (
      <div
        className="ob-plugin-glyph"
        style={{
          width: size,
          height: size,
          borderRadius: radius,
          overflow: 'hidden',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          flexShrink: 0
        }}
      >
        <img src={icon} alt="" style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
      </div>
    )
  }

  return (
    <div
      className="ob-plugin-glyph"
      style={{
        width: size,
        height: size,
        borderRadius: radius,
        background: gradient,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        color: '#fff',
        fontSize: fontSize ?? Math.round(size * 0.42),
        fontWeight: 700,
        flexShrink: 0,
        userSelect: 'none',
        textShadow: '0 1px 2px rgba(0,0,0,0.35)',
        boxShadow: `0 4px 12px ${primary}44`
      }}
    >
      {initialOf(name)}
    </div>
  )
}
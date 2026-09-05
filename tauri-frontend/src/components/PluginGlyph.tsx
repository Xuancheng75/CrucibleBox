import type { ComponentType, CSSProperties } from 'react'
import {
  BgColorsOutlined,
  CodeOutlined,
  CopyOutlined,
  DashboardOutlined,
  FileSearchOutlined,
  GiftOutlined,
  GlobalOutlined,
  PictureOutlined,
  ReadOutlined,
  RocketOutlined,
  TableOutlined
} from '@ant-design/icons'
import { pluginIdentity } from '../plugin-identity'

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
  pluginId?: string
  name: string
  icon?: string
  size?: number
  radius?: number
  fontSize?: number
}

const OFFICIAL_ICONS: Record<string, ComponentType<{ style?: CSSProperties }>> = {
  'document-engine': FileSearchOutlined,
  unienv: RocketOutlined,
  diary: ReadOutlined,
  'gif-editor': PictureOutlined,
  'clipboard-manager': CopyOutlined,
  'json-toolkit': CodeOutlined,
  turntable: GiftOutlined,
  'dice-roller': TableOutlined,
  'exchange-rates': GlobalOutlined,
  'system-info': DashboardOutlined,
  'theme-manager': BgColorsOutlined
}

export default function PluginGlyph({ pluginId = '', name, icon, size = 56, radius = 14, fontSize }: PluginGlyphProps) {
  const identity = pluginIdentity(pluginId)
  const hash = hashName(pluginId || name)
  const angle = 100 + (hash % 80)
  const tone = 0.62 + ((hash % 20) / 100)
  const deep = darken(identity.accentAlt, tone)
  const gradient = `linear-gradient(${angle}deg, ${identity.accent}, ${deep})`
  const OfficialIcon = OFFICIAL_ICONS[pluginId]

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
        border: '1px solid color-mix(in srgb, white 28%, transparent)'
      }}
    >
      {OfficialIcon ? <OfficialIcon style={{ fontSize: fontSize ?? Math.round(size * 0.42) }} /> : initialOf(name)}
    </div>
  )
}

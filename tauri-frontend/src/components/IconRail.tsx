import { Tooltip, theme } from 'antd'
import {
  AppstoreOutlined,
  ClockCircleOutlined,
  SettingOutlined,
  ShopOutlined
} from '@ant-design/icons'
import openboxIcon from '../assets/openbox-neon-64.png'
import type { AppPage } from '../app-pages'

const NAV_ITEMS: Array<{ key: AppPage; icon: React.ReactNode; label: string }> = [
  { key: 'home', icon: <AppstoreOutlined />, label: '工作台' },
  { key: 'marketplace', icon: <ShopOutlined />, label: '插件市场' },
  { key: 'tasks', icon: <ClockCircleOutlined />, label: '任务中心' },
  { key: 'settings', icon: <SettingOutlined />, label: '设置' }
]

interface IconRailProps {
  selectedKey: AppPage
  onChange: (key: AppPage) => void
}

export default function IconRail({ selectedKey, onChange }: IconRailProps) {
  const { token } = theme.useToken()

  return (
    <nav
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%'
      }}
    >
      <div
        className="ob-monogram"
        style={{
          height: 64,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          borderBottom: `1px solid ${token.colorBorderSecondary}`
        }}
      >
        <img
          src={openboxIcon}
          alt="OpenBox"
          style={{
            width: 32,
            height: 32,
            objectFit: 'contain',
            userSelect: 'none'
          }}
        />
      </div>
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 10,
          padding: '16px 8px'
        }}
      >
        {NAV_ITEMS.map((item) => {
          const active = selectedKey === item.key
          return (
            <Tooltip key={item.key} title={item.label} placement="right">
              <button
                className="ob-rail-btn"
                data-active={active ? 'true' : undefined}
                onClick={() => onChange(item.key)}
                aria-label={item.label}
                style={{
                  width: '100%',
                  height: 56,
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  justifyContent: 'center',
                  gap: 4,
                  border: 'none',
                  borderRadius: 12,
                  cursor: 'pointer',
                  fontSize: 20,
                  color: active ? '#fff' : token.colorTextSecondary,
                  background: active
                    ? `linear-gradient(135deg, ${token.colorPrimary}, ${token.colorPrimaryHover})`
                    : 'transparent',
                  boxShadow: 'none',
                  transition: 'all 0.2s ease'
                }}
              >
                {item.icon}
                <span
                  style={{
                    fontSize: 10,
                    lineHeight: 1,
                    color: active ? 'rgba(255,255,255,0.92)' : token.colorTextTertiary
                  }}
                >
                  {item.label}
                </span>
              </button>
            </Tooltip>
          )
        })}
      </div>
    </nav>
  )
}

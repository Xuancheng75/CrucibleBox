import { Tooltip } from 'antd'
import type { KeyboardEvent, ReactNode } from 'react'
import {
  AppstoreOutlined,
  ClockCircleOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  SettingOutlined,
  ShopOutlined
} from '@ant-design/icons'
import type { AppPage } from '../app-pages'
import crucibleboxIcon from '../assets/cruciblebox-icon.svg?no-inline'

const NAV_ITEMS: Array<{ key: AppPage; icon: ReactNode; label: string }> = [
  { key: 'home', icon: <AppstoreOutlined />, label: '工作台' },
  { key: 'marketplace', icon: <ShopOutlined />, label: '插件市场' },
  { key: 'tasks', icon: <ClockCircleOutlined />, label: '任务中心' }
]

interface IconRailProps {
  appVersion: string
  selectedKey: AppPage
  onChange: (key: AppPage) => void
  compact: boolean
  onCompactChange: () => void
  orientation?: 'rail' | 'top' | 'menu'
}

export default function IconRail({
  appVersion,
  selectedKey,
  onChange,
  compact,
  onCompactChange,
  orientation = 'rail'
}: IconRailProps) {
  const moveFocus = (event: KeyboardEvent<HTMLElement>) => {
    if (
      event.altKey ||
      event.ctrlKey ||
      event.metaKey ||
      event.shiftKey ||
      event.nativeEvent.isComposing
    )
      return
    const buttons = Array.from(
      event.currentTarget.querySelectorAll<HTMLButtonElement>('.cbx-side-link')
    )
    const index = buttons.indexOf(event.target as HTMLButtonElement)
    if (index < 0) return
    const forward = orientation === 'top' ? 'ArrowRight' : 'ArrowDown'
    const backward = orientation === 'top' ? 'ArrowLeft' : 'ArrowUp'
    const next =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? buttons.length - 1
          : event.key === forward
            ? (index + 1) % buttons.length
            : event.key === backward
              ? (index + buttons.length - 1) % buttons.length
              : null
    if (next === null) return
    event.preventDefault()
    buttons[next].focus()
  }
  const navButton = (key: AppPage, label: string, icon: ReactNode) => (
    <Tooltip key={key} title={compact ? label : undefined} placement="right">
      <button
        type="button"
        className="cbx-side-link"
        data-active={selectedKey === key ? 'true' : undefined}
        aria-current={selectedKey === key ? 'page' : undefined}
        aria-label={label}
        onClick={() => onChange(key)}
      >
        {icon}
        {!compact && <span>{label}</span>}
      </button>
    </Tooltip>
  )

  return (
    <nav
      className="cbx-side-nav"
      data-orientation={orientation}
      data-compact={compact ? 'true' : undefined}
      aria-label="应用导航"
      onKeyDown={moveFocus}
    >
      <button
        type="button"
        className="cbx-side-brand"
        onClick={() => onChange('home')}
        aria-label="返回工作台"
      >
        <img className="cbx-brand-mark" src={crucibleboxIcon} alt="" aria-hidden="true" />
        {!compact && <strong>CrucibleBox</strong>}
      </button>
      <div className="cbx-side-links" id="app-sidebar-links">
        {NAV_ITEMS.map((item) => navButton(item.key, item.label, item.icon))}
      </div>
      <div className="cbx-side-bottom">
        <small aria-hidden={compact}>{compact ? '\u00a0' : appVersion}</small>
        {navButton('settings', '设置', <SettingOutlined />)}
        {orientation !== 'top' && (
          <Tooltip title={compact ? '展开侧边栏' : '收起侧边栏'} placement="right">
            <button
              type="button"
              className="cbx-side-collapse"
              aria-expanded={!compact}
              aria-controls="app-sidebar-links"
              onClick={onCompactChange}
              aria-label={compact ? '展开侧边栏' : '收起侧边栏'}
            >
              {compact ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
              {!compact && <span>收起侧边栏</span>}
            </button>
          </Tooltip>
        )}
      </div>
    </nav>
  )
}

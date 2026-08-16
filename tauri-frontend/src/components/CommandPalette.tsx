import { useEffect, useMemo, useRef, useState } from 'react'
import { theme } from 'antd'
import { AppstoreOutlined, SettingOutlined, FileTextOutlined } from '@ant-design/icons'
import { useAppStore } from '../store/app.store'
import { usePluginStore } from '../store/plugin.store'

interface PageItem {
  key: string
  label: string
  icon: React.ReactNode
  action: () => void
}

export default function CommandPalette() {
  const { token } = theme.useToken()
  const open = useAppStore((s) => s.commandOpen)
  const setOpen = useAppStore((s) => s.setCommandOpen)
  const setCurrentPage = useAppStore((s) => s.setCurrentPage)
  const setActivePluginId = useAppStore((s) => s.setActivePluginId)
  const plugins = usePluginStore((s) => s.plugins)
  const [query, setQuery] = useState('')
  const [index, setIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setOpen(!open)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [open, setOpen])

  useEffect(() => {
    if (open) {
      setQuery('')
      setIndex(0)
      setTimeout(() => inputRef.current?.focus(), 0)
    }
  }, [open])

  const results = useMemo(() => {
    const pageItems: PageItem[] = [
      {
        key: 'home',
        label: '工作台',
        icon: <AppstoreOutlined />,
        action: () => setCurrentPage('home')
      },
      {
        key: 'logs',
        label: '插件日志',
        icon: <FileTextOutlined />,
        action: () => setCurrentPage('logs')
      },
      {
        key: 'settings',
        label: '设置',
        icon: <SettingOutlined />,
        action: () => setCurrentPage('settings')
      }
    ]

    const q = query.trim().toLowerCase()
    const filtered = q
      ? plugins.filter((p) =>
          `${p.displayName} ${p.name} ${p.description}`.toLowerCase().includes(q)
        )
      : plugins

    const groups: {
      type: 'plugin' | 'page'
      label: string
      icon: React.ReactNode
      run: () => void
    }[] = []
    groups.push(
      ...filtered.map((p) => ({
        type: 'plugin' as const,
        label: p.displayName,
        icon: <AppstoreOutlined />,
        run: () => {
          setActivePluginId(p.id)
          setCurrentPage('pluginView')
        }
      }))
    )
    groups.push(
      ...pageItems
        .filter((item) => !q || item.label.toLowerCase().includes(q))
        .map((item) => ({
          type: 'page' as const,
          label: item.label,
          icon: item.icon,
          run: item.action
        }))
    )
    return groups
  }, [query, plugins, setActivePluginId, setCurrentPage])

  if (!open) return null

  const close = () => setOpen(false)

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setIndex((i) => Math.min(i + 1, results.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setIndex((i) => Math.max(i - 1, 0))
    } else if (e.key === 'Enter' && results[index]) {
      results[index].run()
      close()
    } else if (e.key === 'Escape') {
      close()
    }
  }

  return (
    <div
      onClick={close}
      role="presentation"
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 1000,
        background: 'rgba(0, 0, 0, 0.45)',
        backdropFilter: 'blur(4px)',
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'center',
        paddingTop: '12vh'
      }}
    >
      <div
        className="ob-palette"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="命令面板"
        style={{
          width: 560,
          maxWidth: 'calc(100vw - 48px)',
          background: token.colorBgElevated,
          borderRadius: 12,
          border: `1px solid ${token.colorBorder}`,
          boxShadow: token.boxShadowSecondary,
          overflow: 'hidden',
          animation: 'ob-palette-in 0.15s ease-out'
        }}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value)
            setIndex(0)
          }}
          onKeyDown={onKeyDown}
          placeholder="搜索插件或功能…"
          aria-label="搜索插件或功能"
          aria-controls="ob-command-results"
          aria-activedescendant={results[index] ? `ob-command-option-${index}` : undefined}
          style={{
            width: '100%',
            padding: '14px 18px',
            border: 'none',
            outline: 'none',
            fontSize: 15,
            background: 'transparent',
            color: token.colorText,
            borderBottom: `1px solid ${token.colorBorderSecondary}`
          }}
        />
        <div
          id="ob-command-results"
          role="listbox"
          style={{ maxHeight: 360, overflow: 'auto', padding: 6 }}
        >
          {results.length === 0 ? (
            <div
              style={{
                padding: '24px 18px',
                textAlign: 'center',
                fontSize: 13,
                color: token.colorTextTertiary
              }}
            >
              无匹配结果
            </div>
          ) : (
            results.map((item, i) => (
              <div
                id={`ob-command-option-${i}`}
                key={`${item.type}-${item.label}`}
                role="option"
                aria-selected={i === index}
                onClick={() => {
                  item.run()
                  close()
                }}
                onMouseEnter={() => setIndex(i)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  padding: '9px 12px',
                  borderRadius: 8,
                  cursor: 'pointer',
                  fontSize: 14,
                  color: token.colorText,
                  background: i === index ? token.colorPrimaryBg : 'transparent'
                }}
              >
                <span style={{ color: token.colorPrimary, fontSize: 15 }}>{item.icon}</span>
                <span style={{ flex: 1 }}>{item.label}</span>
                {item.type === 'plugin' ? (
                  <span style={{ fontSize: 11, color: token.colorTextTertiary }}>插件</span>
                ) : (
                  <span style={{ fontSize: 11, color: token.colorTextTertiary }}>页面</span>
                )}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}
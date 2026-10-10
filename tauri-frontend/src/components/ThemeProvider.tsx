import React from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useEffect } from 'react'
import { App as AntdApp, ConfigProvider } from 'antd'
import { themeToCssVars } from '../../../shared/themes/css-vars'
import { antdThemeConfig } from '../theme/antd'
import { useThemeStore } from '../store/theme.store'

interface ThemeProviderProps {
  children: React.ReactNode
}

export function ThemeProvider({ children }: ThemeProviderProps) {
  const toolboxTheme = useThemeStore((s) => s.theme)
  const init = useThemeStore((s) => s.init)

  useEffect(() => {
    void init()
  }, [init])

  useEffect(() => {
    const root = document.documentElement
    const vars = themeToCssVars(toolboxTheme)
    for (const [key, value] of Object.entries(vars)) {
      root.style.setProperty(key, value)
    }
    root.style.colorScheme = toolboxTheme.mode
    root.dataset.obTheme = toolboxTheme.id
    const colorRef = (color: string) => {
      const hex = color.replace(/^#/, '')
      const full = hex.length === 3 ? [...hex].map((c) => c + c).join('') : hex
      return parseInt(full.slice(4, 6) + full.slice(2, 4) + full.slice(0, 2), 16)
    }
    void invoke('window_apply_theme', {
      dark: toolboxTheme.mode === 'dark',
      caption: colorRef(toolboxTheme.tokens.colorBgContainer),
      text: colorRef(toolboxTheme.tokens.colorText)
    }).catch((error) => console.error('标题栏主题同步失败', error))
  }, [toolboxTheme])

  return (
    <ConfigProvider
      theme={antdThemeConfig(toolboxTheme)}
      layout={{ className: 'ob-layout' }}
      button={{ className: 'ob-button' }}
      card={{ className: 'ob-surface-card' }}
      table={{ className: 'ob-data-table' }}
      modal={{ classNames: { content: 'ob-modal-surface' } }}
      input={{ className: 'ob-input' }}
      select={{
        className: 'ob-select',
        classNames: { popup: { root: 'ob-select-content' } }
      }}
      tag={{ className: 'ob-tag' }}
      statistic={{ className: 'ob-statistic-content' }}
      alert={{ className: 'ob-alert' }}
    >
      <AntdApp>{children}</AntdApp>
    </ConfigProvider>
  )
}

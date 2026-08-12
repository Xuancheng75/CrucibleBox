import React from 'react'
import { useEffect } from 'react'
import { App as AntdApp, ConfigProvider } from 'antd'
import { themeToCssVars } from '@shared/themes/css-vars'
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
  }, [toolboxTheme])

  return (
    <ConfigProvider
      theme={antdThemeConfig(toolboxTheme)}
      layout={{ className: 'ob-layout' }}
      button={{ className: 'ob-button' }}
      card={{ className: 'ob-surface-card' }}
      table={{
        className: 'ob-data-table',
        classNames: {
          content: 'ob-data-table-content',
          body: { cell: 'ob-table-cell' },
          header: { cell: 'ob-table-cell' }
        }
      }}
      modal={{ classNames: { container: 'ob-modal-surface' } }}
      input={{ classNames: { root: 'ob-input' } }}
      select={{ classNames: { root: 'ob-select', content: 'ob-select-content' } }}
      tag={{ className: 'ob-tag' }}
      statistic={{ classNames: { content: 'ob-statistic-content' } }}
      alert={{ className: 'ob-alert' }}
    >
      <AntdApp>{children}</AntdApp>
    </ConfigProvider>
  )
}

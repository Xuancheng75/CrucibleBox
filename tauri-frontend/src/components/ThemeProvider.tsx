import React from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useMemo, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { App as AntdApp, ConfigProvider, Alert, Button } from 'antd'
import { restoreDefaultTheme } from '../features/plugins/runtime/NextThemeController'
import { prepareThemeStyle } from '../theme/prepare'
import { themeToCssVars } from '../../../shared/themes/css-vars'
import { antdThemeConfig } from '../theme/antd'
import { useThemeStore } from '../store/theme.store'
import { skipPageHandoff } from '../theme/page-handoff'

interface ThemeProviderProps {
  children: React.ReactNode
}

export function ThemeProvider({ children }: ThemeProviderProps) {
  const toolboxTheme = useThemeStore((s) => s.theme)
  const init = useThemeStore((s) => s.init)
  const style = useThemeStore((s) => s.style)
  const themeConfig = useMemo(() => antdThemeConfig(toolboxTheme), [toolboxTheme])
  const styleElement = useRef<HTMLStyleElement | null>(null)
  const [styleError, setStyleError] = useState<string | null>(null)
  const [reduced, setReduced] = useState(
    () => window.matchMedia('(prefers-reduced-motion: reduce)').matches
  )

  useEffect(() => {
    void init()
  }, [init])

  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const changed = () => setReduced(media.matches)
    media.addEventListener('change', changed)
    const subscription = listen('theme:reset', () => {
      void restoreDefaultTheme().catch((error) => setStyleError(String(error)))
    })
    return () => {
      media.removeEventListener('change', changed)
      void subscription.then((unlisten) => unlisten())
    }
  }, [])

  useEffect(() => {
    const element = document.createElement('style')
    document.head.append(element)
    styleElement.current = element
    return () => element.remove()
  }, [])

  useEffect(() => {
    let active = true
    const root = document.documentElement
    root.dataset.obThemeSurface = 'host'
    setStyleError(null)
    void prepareThemeStyle(toolboxTheme, reduced)
      .then((prepared) => {
        if (!active) return
        skipPageHandoff()
        if (styleElement.current) styleElement.current.textContent = prepared.css
        if (prepared.css) root.dataset.obThemeVersion = '2'
        else delete root.dataset.obThemeVersion
        useThemeStore.setState({ style: prepared })
      })
      .catch((error) => {
        if (active) setStyleError(String(error))
      })
    return () => {
      active = false
    }
  }, [toolboxTheme, reduced])

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
      theme={themeConfig}
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
      <AntdApp>
        {styleError && (
          <Alert
            type="error"
            message="主题样式未能应用"
            description={styleError}
            action={
              <Button
                onClick={() => {
                  void restoreDefaultTheme().catch((error) => setStyleError(String(error)))
                }}
              >
                恢复默认主题
              </Button>
            }
          />
        )}
        {style.layers.length > 0 && (
          <div className="cbx-theme-decorations" aria-hidden="true">
            {style.layers.map((layer) => (
              <div key={layer.id} data-layer-id={layer.id} data-layer-scope={layer.scope}>
                {layer.text}
              </div>
            ))}
          </div>
        )}
        {children}
      </AntdApp>
    </ConfigProvider>
  )
}

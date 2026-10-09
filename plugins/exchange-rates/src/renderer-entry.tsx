import React from 'react'
import { createRoot } from 'react-dom/client'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import Renderer from './renderer'
import UnitConverter from './UnitConverter'

function ExchangeAndUnits(props: PluginRenderProps) {
  const [page, setPage] = React.useState<'rates' | 'units'>('rates')
  return (
    <div style={{ minHeight: '100%', background: 'var(--ob-color-bg-layout, #f7f9fb)' }}>
      <nav style={{ display: 'flex', gap: 8, padding: '16px 20px 0' }}>
        <button onClick={() => setPage('rates')} aria-pressed={page === 'rates'}>
          实时汇率
        </button>
        <button onClick={() => setPage('units')} aria-pressed={page === 'units'}>
          单位换算
        </button>
      </nav>
      {page === 'rates' ? <Renderer {...props} /> : <UnitConverter />}
    </div>
  )
}

declare global {
  interface Window {
    __OPENBOX_PLUGIN_RUNTIME__: {
      mount(
        adapter: (
          container: HTMLElement,
          initialProps: PluginRenderProps,
          subscribeProps: (listener: (props: PluginRenderProps) => void) => () => void
        ) => () => void
      ): void
    }
  }
}

window.__OPENBOX_PLUGIN_RUNTIME__.mount((container, initialProps, subscribeProps) => {
  const root = createRoot(container)
  const render = (props: PluginRenderProps) =>
    root.render(React.createElement(ExchangeAndUnits, props))
  render(initialProps)
  const unsubscribe = subscribeProps(render)
  return () => {
    unsubscribe()
    root.unmount()
  }
})

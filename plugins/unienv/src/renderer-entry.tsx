import React from 'react'
import { createRoot } from 'react-dom/client'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import UniEnvUI from './renderer'

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
  const render = (props: PluginRenderProps) => root.render(React.createElement(UniEnvUI, props))
  render(initialProps)
  const unsubscribe = subscribeProps(render)
  return () => {
    unsubscribe()
    root.unmount()
  }
})

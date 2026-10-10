import React from 'react'
import { createRoot } from 'react-dom/client'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import App from './renderer'
declare global {
  interface Window {
    __OPENBOX_PLUGIN_RUNTIME__: {
      mount(
        adapter: (
          container: HTMLElement,
          props: PluginRenderProps,
          subscribe: (listener: (props: PluginRenderProps) => void) => () => void
        ) => () => void
      ): void
    }
  }
}
window.__OPENBOX_PLUGIN_RUNTIME__.mount((container, props, subscribe) => {
  const root = createRoot(container)
  const render = (next: PluginRenderProps) => root.render(React.createElement(App, next))
  render(props)
  const unsubscribe = subscribe(render)
  return () => {
    unsubscribe()
    root.unmount()
  }
})

import React from 'react'
import { createRoot } from 'react-dom/client'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import Renderer from './renderer'

declare global {
  interface Window {
    __OPENBOX_PLUGIN_PROPS__?: PluginRenderProps
  }
}

const container = document.getElementById('root')
const props = window.__OPENBOX_PLUGIN_PROPS__
if (!container || !props) throw new Error('OpenBox renderer bootstrap is unavailable')
createRoot(container).render(React.createElement(Renderer, props))

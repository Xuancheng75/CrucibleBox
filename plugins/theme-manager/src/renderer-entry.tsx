import React from 'react'
import { createRoot } from 'react-dom/client'
import { createClient, type RendererContext, type Json } from '@cruciblebox/next-api'
import ThemeManager from './renderer'
import type { Theme } from './next-types'
export async function mount(context: RendererContext) {
  const client = createClient(context)
  const [theme, config] = await Promise.all([client.theme.get(), client.config.get()])
  if (!config || typeof config !== 'object' || Array.isArray(config))
    throw Error('INVALID_RESPONSE')
  let active = true
  const root = createRoot(context.root)
  const themeApi = {
    get: async () => (await client.theme.get()) as unknown as Theme,
    list: async () => (await client.theme.list()) as unknown as Theme[],
    preview: (value: Theme) => client.theme.preview(value as unknown as Json),
    commit: client.theme.commit,
    rollback: client.theme.rollback
  }
  let currentConfig: Record<string, unknown> = config
  const onConfigChange = async (values: Record<string, unknown>) => {
    if (!active) throw Error('SESSION_DENIED')
    const saved = await client.config.patch(values as Record<string, Json>)
    if (!active) return
    if (!saved || typeof saved !== 'object' || Array.isArray(saved)) throw Error('INVALID_RESPONSE')
    currentConfig = saved
    render()
  }
  const render = () =>
    root.render(
      React.createElement(ThemeManager, {
        theme: theme as unknown as Theme,
        config: currentConfig,
        api: { theme: themeApi },
        onConfigChange
      })
    )
  render()
  return () => {
    active = false
    root.unmount()
  }
}

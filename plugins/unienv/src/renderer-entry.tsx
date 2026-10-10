import { taskService } from './next-task-service'
import React from 'react'
import { createRoot } from 'react-dom/client'
import { createClient, type RendererContext, type Json } from '@cruciblebox/next-api'
import Renderer from './renderer'
import type { ServiceRenderProps } from './next-types'
export async function mount(context: RendererContext) {
  const client = createClient(context)
  const initial = await client.config.get()
  if (!initial || typeof initial !== 'object' || Array.isArray(initial))
    throw Error('INVALID_RESPONSE')
  let active = true,
    currentConfig: Record<string, unknown> = initial
  const status = document.createElement('div')
  status.setAttribute('role', 'alert')
  status.style.color = 'var(--ob-color-error)'
  const app = document.createElement('div')
  context.root.replaceChildren(status, app)
  const root = createRoot(app)
  const callTaskService = taskService(client, 'environment', (message) =>
    client.environment.call(message as { type: string; [key: string]: Json })
  )
  const api: ServiceRenderProps['api'] = {
    service: {
      call: async (message: unknown) => {
        if (!active) throw Error('SESSION_DENIED')
        if (
          !message ||
          typeof message !== 'object' ||
          Array.isArray(message) ||
          typeof (message as { type?: unknown }).type !== 'string'
        )
          throw Error('INVALID_REQUEST')
        return callTaskService(message as { type: string; [key: string]: unknown })
      }
    },
    dialog: { open: async (options) => (await client.dialog.open(options)) ?? [] },
    confirm: client.dialog.confirm,
    notify: (title, body) => {
      void client.notify(title, body).catch((error) => {
        if (active) status.textContent = String(error)
      })
    },
    onFilesDropped: context.onFilesDropped
  }
  const onConfigChange = async (values: Record<string, unknown>) => {
    try {
      if (!active) throw Error('SESSION_DENIED')
      const saved = await client.config.patch(values as Record<string, Json>)
      if (!active) return
      if (!saved || typeof saved !== 'object' || Array.isArray(saved))
        throw Error('INVALID_RESPONSE')
      currentConfig = saved
      render()
    } catch (error) {
      if (active) status.textContent = '保存配置失败：' + String(error)
    }
  }
  const render = () =>
    root.render(React.createElement(Renderer, { api, config: currentConfig, onConfigChange }))
  render()
  return () => {
    active = false
    root.unmount()
    context.root.replaceChildren()
  }
}

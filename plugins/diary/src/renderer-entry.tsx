import React from 'react'
import { createRoot } from 'react-dom/client'
import { createClient, type RendererContext } from '@cruciblebox/next-api'
import DiaryApp from './renderer'
import diary from './main'
import { createDiaryStorage } from './next-service'
export function mount(context: RendererContext) {
  const client = createClient(context)
  let active = true
  diary.activate({ storage: createDiaryStorage(client), logger: console })
  const api = {
    execute: async (message: unknown) => {
      if (!active) throw Error('SESSION_DENIED')
      return diary.onMessage(message)
    }
  }
  const root = createRoot(context.root)
  root.render(React.createElement(DiaryApp, { api }))
  return () => {
    active = false
    root.unmount()
    diary.deactivate()
  }
}

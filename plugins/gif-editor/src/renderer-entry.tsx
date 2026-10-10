import React from 'react'
import { createRoot } from 'react-dom/client'
import type { RendererContext } from '@cruciblebox/next-api'
import GifEditorPlugin from './renderer'

type GifEditorApi = {
  notify(title: string, message: string): void
}

export function mount(context: RendererContext): () => void {
  const statusRegion = document.createElement('div')
  statusRegion.className = 'gif-editor-runtime-status'
  statusRegion.setAttribute('role', 'status')
  statusRegion.setAttribute('aria-live', 'polite')
  statusRegion.setAttribute('aria-atomic', 'true')
  statusRegion.textContent = '就绪'
  Object.assign(statusRegion.style, {
    boxSizing: 'border-box',
    margin: '0 0 12px',
    padding: '8px 12px',
    border: '1px solid var(--ob-color-border)',
    borderRadius: 'var(--ob-radius)',
    backgroundColor: 'var(--ob-color-bg-container)',
    color: 'var(--ob-color-text)',
    fontFamily: 'var(--ob-font-family)'
  })

  const appContainer = document.createElement('div')
  context.root.replaceChildren(statusRegion, appContainer)
  const root = createRoot(appContainer)
  const api: GifEditorApi = {
    notify(title, message) {
      statusRegion.textContent = `${title}：${message}`
    }
  }
  root.render(React.createElement(GifEditorPlugin, { api }))

  let disposed = false
  return () => {
    if (disposed) return
    disposed = true
    root.unmount()
    context.root.replaceChildren()
  }
}

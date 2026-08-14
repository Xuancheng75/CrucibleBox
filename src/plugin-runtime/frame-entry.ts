import React from 'react'
import * as jsxRuntime from 'react/jsx-runtime'
import { createRoot } from 'react-dom/client'
import {
  createPluginRendererRpcReady,
  createPluginRendererRpcRequest,
  parsePluginRendererRpcEnvelope,
  PluginRendererRpcPendingRequests,
  validatePluginRendererRpcResponse
} from '../../shared/plugin-renderer-rpc'
import type {
  PluginRendererRpcEvent,
  PluginRendererRpcMethod,
  PluginRendererRpcMethodMap,
  PluginRendererRpcResponse
} from '../../shared/types/plugin-renderer-rpc.types'
import type {
  PluginConfig,
  PluginRenderProps,
  PluginRendererComponent
} from '../../shared/types/plugin.types'
import { themeToCssVars } from '../../shared/themes/css-vars'
import type { ToolboxTheme } from '../../shared/types/theme.types'

type PropsListener = (props: PluginRenderProps) => void
type SubscribeProps = (listener: PropsListener) => () => void
type MountAdapter = (
  container: HTMLElement,
  initialProps: PluginRenderProps,
  subscribeProps: SubscribeProps
) => void | (() => void)

declare global {
  interface Window {
    __OPENBOX_PLUGIN_RUNTIME__?: {
      mount(adapter: MountAdapter): void
    }
  }
}

const CONNECT_MESSAGE_KIND = 'cruciblebox-plugin-connect'
const PORT_MESSAGE_KIND = 'cruciblebox-plugin-port'
const REQUEST_TIMEOUT_MS = 30_000
const rootElementCandidate = document.getElementById('root')

if (!rootElementCandidate) {
  throw new Error('Plugin frame root element is missing')
}
const rootElement: HTMLElement = rootElementCandidate

const sessionToken = rootElement.dataset.sessionToken ?? ''
const rendererApiVersion = rootElement.dataset.apiVersion === '2' ? 2 : 1
const rendererUrl = rootElement.dataset.rendererUrl ?? '/renderer.js'
const propsListeners = new Set<PropsListener>()
const backendListeners = new Set<(message: unknown) => void>()
const themeListeners = new Set<(theme: ToolboxTheme) => void>()
const pendingTracker = new PluginRendererRpcPendingRequests()
const pending = new Map<
  string,
  {
    method: PluginRendererRpcMethod
    resolve(value: unknown): void
    reject(reason: Error): void
    timeout: ReturnType<typeof setTimeout>
  }
>()

let port: MessagePort | null = null
let initialized = false
let disposed = false
let mountAdapter: MountAdapter | null = null
let mountCleanup: (() => void) | null = null
let currentConfig: PluginConfig = {}
let currentTheme: ToolboxTheme | null = null
let requestSequence = 0
let legacyLoadStarted = false

const ambientWindow = window as Window & {
  electronAPI?: unknown
  process?: unknown
  require?: unknown
}
if (
  ambientWindow.electronAPI !== undefined ||
  ambientWindow.process !== undefined ||
  ambientWindow.require !== undefined
) {
  throw new Error('Privileged host globals are exposed inside the plugin frame')
}
if (window.parent !== window) {
  let parentDocumentReachable = true
  try {
    void window.parent.document.documentElement
  } catch {
    parentDocumentReachable = false
  }
  if (parentDocumentReachable) {
    throw new Error('Plugin frame must use a cross-origin document')
  }
}

document.documentElement.style.colorScheme = 'light'
document.body.style.margin = '0'
document.body.style.minHeight = '100%'

function renderFatal(error: unknown): void {
  const message = error instanceof Error ? error.message : String(error)
  rootElement.replaceChildren()
  const alert = document.createElement('pre')
  alert.setAttribute('role', 'alert')
  alert.style.whiteSpace = 'pre-wrap'
  alert.style.padding = '16px'
  alert.style.color = 'var(--ob-color-error, #ff4d4f)'
  alert.textContent = `插件界面加载失败：${message}`
  rootElement.append(alert)
}

function applyTheme(theme: ToolboxTheme): void {
  for (const [name, value] of Object.entries(themeToCssVars(theme))) {
    document.documentElement.style.setProperty(name, value)
  }
  document.documentElement.style.colorScheme = theme.mode
  document.documentElement.dataset.obTheme = theme.id
}

function nextRequestId(): string {
  requestSequence = (requestSequence + 1) % Number.MAX_SAFE_INTEGER
  return `frame:${Date.now().toString(36)}:${requestSequence.toString(36)}`
}

function request<Method extends PluginRendererRpcMethod>(
  method: Method,
  params: PluginRendererRpcMethodMap[Method]['params']
): Promise<PluginRendererRpcMethodMap[Method]['result']> {
  if (!initialized || !port || disposed) {
    return Promise.reject(new Error('Plugin renderer bridge is not ready'))
  }

  const requestId = nextRequestId()
  pendingTracker.add(requestId)
  const envelope = createPluginRendererRpcRequest(sessionToken, requestId, method, params)

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pending.delete(requestId)
      pendingTracker.delete(requestId)
      reject(new Error(`Plugin renderer request timed out: ${method}`))
    }, REQUEST_TIMEOUT_MS)
    pending.set(requestId, {
      method,
      resolve,
      reject,
      timeout
    })
    port?.postMessage(envelope)
  })
}

const pluginApi: PluginRenderProps['api'] = {
  async sendToBackend(message) {
    const result = await request('backend.send', {
      message: message as PluginRendererRpcMethodMap['backend.send']['params']['message']
    })
    return result.value
  },
  notify(title, body) {
    void request('notification.show', {
      title,
      ...(body === undefined ? {} : { body })
    }).catch(() => undefined)
  },
  async confirm(options) {
    const result = await request('dialog.confirm', options)
    return result.confirmed
  },
  onBackendMessage(handler) {
    backendListeners.add(handler)
    return () => backendListeners.delete(handler)
  },
  theme: {
    async get() {
      const result = await request('theme.get', {})
      return result.theme
    },
    async list() {
      const result = await request('theme.list', {})
      return result.themes
    },
    async preview(theme) {
      const result = await request('theme.preview', { theme })
      return result.applied
    },
    async commit() {
      const result = await request('theme.commit', {})
      return result.committed
    },
    async rollback() {
      const result = await request('theme.rollback', {})
      return result.restored
    },
    async set(theme) {
      const result = await request('theme.set', { theme })
      return result.applied
    }
  }
}

function createProps(): PluginRenderProps {
  if (!currentTheme) throw new Error('Plugin renderer theme is not initialized')
  return {
    config: currentConfig,
    onConfigChange(config) {
      void request('config.update', {
        config: config as PluginRendererRpcMethodMap['config.update']['params']['config']
      }).catch(() => undefined)
    },
    theme: currentTheme,
    api: pluginApi
  }
}

function publishProps(): void {
  if (!currentTheme) return
  const props = createProps()
  for (const listener of propsListeners) listener(props)
}

const subscribeProps: SubscribeProps = (listener) => {
  propsListeners.add(listener)
  return () => propsListeners.delete(listener)
}

function mountWhenReady(): void {
  if (disposed || mountCleanup || !initialized || !currentTheme || !mountAdapter) return
  const cleanup = mountAdapter(rootElement, createProps(), subscribeProps)
  mountCleanup = typeof cleanup === 'function' ? cleanup : () => undefined
}

async function loadLegacyRenderer(): Promise<void> {
  if (rendererApiVersion !== 1 || legacyLoadStarted || !initialized || !currentTheme || disposed) {
    return
  }
  legacyLoadStarted = true
  const response = await fetch(rendererUrl, { credentials: 'omit', cache: 'no-store' })
  if (!response.ok) throw new Error(`Legacy renderer returned HTTP ${response.status}`)
  const code = await response.text()
  const module: { exports: unknown } = { exports: {} }
  const legacyThemeModule = {
    getTheme: () => currentTheme,
    subscribe(listener: (theme: ToolboxTheme) => void) {
      themeListeners.add(listener)
      return () => themeListeners.delete(listener)
    }
  }
  const legacyRequire = (id: string): unknown => {
    if (id === 'react') return React
    if (id === 'react/jsx-runtime' || id === 'react/jsx-dev-runtime') return jsxRuntime
    if (id === 'openbox-theme') return legacyThemeModule
    throw new Error(`Legacy plugin module is unavailable: ${id}`)
  }
  const factory = new Function('module', 'exports', 'require', code)
  factory(module, module.exports, legacyRequire)
  const exported = module.exports as { default?: unknown }
  const Component = (exported?.default ?? module.exports) as PluginRendererComponent
  if (typeof Component !== 'function') throw new Error('Legacy renderer did not export a component')

  mountAdapter = (container, initialProps, subscribe) => {
    const root = createRoot(container)
    const render = (props: PluginRenderProps): void => {
      root.render(React.createElement(Component, props))
    }
    render(initialProps)
    const unsubscribe = subscribe(render)
    return () => {
      unsubscribe()
      root.unmount()
    }
  }
  mountWhenReady()
}

function handleEvent(envelope: PluginRendererRpcEvent): void {
  switch (envelope.event) {
    case 'state.initialize':
      currentConfig = envelope.data.config
      currentTheme = envelope.data.theme
      applyTheme(currentTheme)
      initialized = true
      publishProps()
      mountWhenReady()
      void loadLegacyRenderer().catch(renderFatal)
      return
    case 'state.configChanged':
      currentConfig = envelope.data.config
      publishProps()
      return
    case 'theme.changed':
      currentTheme = envelope.data.theme
      applyTheme(currentTheme)
      publishProps()
      for (const listener of themeListeners) listener(currentTheme)
      return
    case 'backend.message':
      for (const listener of backendListeners) listener(envelope.data.message)
      return
    case 'host.dispose':
      dispose()
  }
}

function handleResponse(envelope: PluginRendererRpcResponse): void {
  const requestState = pending.get(envelope.requestId)
  if (!requestState) return
  clearTimeout(requestState.timeout)
  pending.delete(envelope.requestId)
  pendingTracker.delete(envelope.requestId)
  try {
    const response = validatePluginRendererRpcResponse(envelope, requestState.method)
    if (response.ok) requestState.resolve(response.result)
    else requestState.reject(new Error(`${response.error.code}: ${response.error.message}`))
  } catch (error) {
    requestState.reject(error instanceof Error ? error : new Error(String(error)))
  }
}

function handlePortMessage(event: MessageEvent<unknown>): void {
  const parsed = parsePluginRendererRpcEnvelope(event.data)
  if (!parsed.ok || parsed.value.token !== sessionToken) return

  if (!initialized && parsed.value.kind === 'init') {
    port?.postMessage(createPluginRendererRpcReady(sessionToken))
    return
  }
  if (parsed.value.kind === 'response') handleResponse(parsed.value)
  if (parsed.value.kind === 'event') handleEvent(parsed.value)
}

function dispose(): void {
  if (disposed) return
  disposed = true
  mountCleanup?.()
  mountCleanup = null
  for (const state of pending.values()) {
    clearTimeout(state.timeout)
    state.reject(new Error('Plugin renderer host disposed'))
  }
  pending.clear()
  pendingTracker.clear()
  propsListeners.clear()
  backendListeners.clear()
  themeListeners.clear()
  port?.close()
  port = null
}

window.__OPENBOX_PLUGIN_RUNTIME__ = {
  mount(adapter) {
    if (mountAdapter || disposed) throw new Error('Plugin renderer may only mount once')
    mountAdapter = adapter
    mountWhenReady()
  }
}

window.addEventListener('message', (event: MessageEvent<unknown>) => {
  if (port || disposed || event.source !== window.parent || event.ports.length !== 0) return
  if (
    typeof event.data !== 'object' ||
    event.data === null ||
    (event.data as Record<string, unknown>).kind !== CONNECT_MESSAGE_KIND ||
    (event.data as Record<string, unknown>).v !== 1 ||
    (event.data as Record<string, unknown>).token !== sessionToken
  ) {
    return
  }
  const channel = new MessageChannel()
  port = channel.port1
  port.onmessage = handlePortMessage
  port.start()
  window.parent.postMessage({ kind: PORT_MESSAGE_KIND, v: 1, token: sessionToken }, '*', [
    channel.port2
  ])
})

let resizeFrame = 0
const resizeObserver = new ResizeObserver(() => {
  if (resizeFrame || !initialized || disposed) return
  resizeFrame = requestAnimationFrame(() => {
    resizeFrame = 0
    const height = Math.max(
      100,
      Math.min(16384, Math.ceil(Math.max(document.body.scrollHeight, rootElement.scrollHeight)))
    )
    void request('layout.resize', { height }).catch(() => undefined)
  })
})
resizeObserver.observe(rootElement)
window.addEventListener('beforeunload', () => {
  resizeObserver.disconnect()
  if (resizeFrame) cancelAnimationFrame(resizeFrame)
  dispose()
})

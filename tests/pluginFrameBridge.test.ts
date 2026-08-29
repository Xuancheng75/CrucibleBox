import { afterEach, describe, expect, it, vi } from 'vitest'

import { PluginFrameBridge } from '../src/plugin-runtime/PluginFrameBridge'
import {
  createPluginRendererRpcReady,
  createPluginRendererRpcRequest,
  validatePluginRendererRpcResponse
} from '../shared/plugin-renderer-rpc'
import { DEFAULT_THEME } from '../shared/themes/presets'
import { Permission } from '../shared/types/permissions'

const TOKEN = 'a'.repeat(64)

function createHarness(permissions: Permission[] = []) {
  let framePort: MessagePort | null = null
  let connectMessage: unknown
  let connectOrigin = ''
  const messages: unknown[] = []
  const waiters: Array<(value: unknown) => void> = []
  const nextMessage = (): Promise<unknown> => {
    const current = messages.shift()
    if (current !== undefined) return Promise.resolve(current)
    return new Promise((resolve) => waiters.push(resolve))
  }
  const hostListeners = new Set<(event: MessageEvent<unknown>) => void>()
  const messageTarget = {
    addEventListener(_type: 'message', listener: (event: MessageEvent<unknown>) => void) {
      hostListeners.add(listener)
    },
    removeEventListener(_type: 'message', listener: (event: MessageEvent<unknown>) => void) {
      hostListeners.delete(listener)
    }
  }
  const targetWindow = {
    postMessage(message: unknown, origin: string) {
      connectMessage = message
      connectOrigin = origin
      const channel = new MessageChannel()
      framePort = channel.port1
      framePort.onmessage = (event) => {
        const waiter = waiters.shift()
        if (waiter) waiter(event.data)
        else messages.push(event.data)
      }
      framePort.start()
      const transferEvent = {
        source: targetWindow,
        origin: 'cruciblebox-plugin://session.test',
        ports: [channel.port2],
        data: { kind: 'cruciblebox-plugin-port', v: 1, token: TOKEN }
      } as unknown as MessageEvent<unknown>
      for (const listener of hostListeners) listener(transferEvent)
    }
  } as unknown as Window
  const sendToBackend = vi.fn(async () => ({ pong: true }))
  const updateConfig = vi.fn()
  let currentTheme = DEFAULT_THEME
  const setTheme = vi.fn(async (theme: typeof DEFAULT_THEME) => {
    currentTheme = theme
    return true
  })
  const bridge = new PluginFrameBridge({
    token: TOKEN,
    origin: 'cruciblebox-plugin://session.test',
    permissions,
    initialConfig: { value: 1 },
    initialTheme: DEFAULT_THEME,
    sendToBackend,
    updateConfig,
    showNotification: () => true,
    getTheme: () => currentTheme,
    listThemes: async () => [DEFAULT_THEME],
    setTheme,
    confirm: async () => true,
    openDialog: async () => [],
    resize: () => undefined,
    messageTarget
  })
  bridge.connect(targetWindow)
  return {
    bridge,
    nextMessage,
    send(message: unknown) {
      framePort?.postMessage(message)
    },
    get connectMessage() {
      return connectMessage
    },
    get connectOrigin() {
      return connectOrigin
    },
    sendToBackend,
    updateConfig,
    setTheme
  }
}

async function ready(
  harness: ReturnType<typeof createHarness>,
  expectedConfigValue = 1
): Promise<void> {
  expect(await harness.nextMessage()).toMatchObject({ kind: 'init', token: TOKEN })
  harness.send(createPluginRendererRpcReady(TOKEN))
  expect(await harness.nextMessage()).toMatchObject({
    kind: 'event',
    event: 'state.initialize',
    data: { config: { value: expectedConfigValue } }
  })
}

afterEach(() => vi.restoreAllMocks())

describe('PluginFrameBridge', () => {
  it('uses the exact session origin and initializes through a transferred port', async () => {
    const harness = createHarness()
    expect(harness.connectOrigin).toBe('cruciblebox-plugin://session.test')
    expect(harness.connectMessage).toEqual({
      kind: 'cruciblebox-plugin-connect',
      v: 1,
      token: TOKEN
    })
    await ready(harness)
    harness.bridge.dispose()
  })

  it('dispatches backend requests and validates the response shape', async () => {
    const harness = createHarness()
    await ready(harness)
    harness.send(
      createPluginRendererRpcRequest(TOKEN, 'request-1', 'backend.send', {
        message: { ping: true }
      })
    )
    const response = validatePluginRendererRpcResponse(await harness.nextMessage(), 'backend.send')
    expect(response).toMatchObject({ ok: true, result: { value: { pong: true } } })
    expect(harness.sendToBackend).toHaveBeenCalledWith({ ping: true })
    harness.bridge.dispose()
  })

  it('rejects a capability request when its permission is absent', async () => {
    const harness = createHarness()
    await ready(harness)
    harness.send(
      createPluginRendererRpcRequest(TOKEN, 'request-2', 'notification.show', {
        title: 'Notice'
      })
    )
    expect(await harness.nextMessage()).toMatchObject({
      kind: 'response',
      ok: false,
      error: { code: 'NOT_ALLOWED' }
    })
    harness.bridge.dispose()
  })

  it('publishes live config and theme changes only after readiness', async () => {
    const harness = createHarness([Permission.ThemeWrite])
    harness.bridge.updateConfig({ value: 2 })
    await ready(harness, 2)
    harness.bridge.updateConfig({ value: 3 })
    expect(await harness.nextMessage()).toMatchObject({
      kind: 'event',
      event: 'state.configChanged',
      data: { config: { value: 3 } }
    })
    harness.bridge.updateTheme({ ...DEFAULT_THEME, id: 'next', name: 'Next' })
    expect(await harness.nextMessage()).toMatchObject({
      kind: 'event',
      event: 'theme.changed',
      data: { theme: { id: 'next' } }
    })
    harness.bridge.dispose()
  })

  it('forwards real OS drop paths after the frame is ready', async () => {
    const harness = createHarness()
    await ready(harness)
    harness.bridge.sendFilesDropped(['C:/Docs/report.pdf', 'D:/Docs'])
    expect(await harness.nextMessage()).toMatchObject({
      kind: 'event',
      event: 'host.filesDropped',
      data: { paths: ['C:/Docs/report.pdf', 'D:/Docs'] }
    })
    harness.bridge.dispose()
  })

  it('restores an uncommitted theme preview when the frame bridge is disposed', async () => {
    const harness = createHarness([Permission.ThemeWrite])
    await ready(harness)
    const preview = { ...DEFAULT_THEME, id: 'preview', name: 'Preview' }
    harness.send(
      createPluginRendererRpcRequest(TOKEN, 'theme-preview', 'theme.preview', { theme: preview })
    )
    expect(await harness.nextMessage()).toMatchObject({
      kind: 'response',
      ok: true,
      result: { applied: true }
    })
    expect(harness.setTheme).toHaveBeenLastCalledWith(preview)

    harness.bridge.dispose()
    await vi.waitFor(() => expect(harness.setTheme).toHaveBeenLastCalledWith(DEFAULT_THEME))
  })

  it('does not roll back a committed theme preview on disposal', async () => {
    const harness = createHarness([Permission.ThemeWrite])
    await ready(harness)
    const preview = { ...DEFAULT_THEME, id: 'kept', name: 'Kept' }
    harness.send(
      createPluginRendererRpcRequest(TOKEN, 'theme-preview', 'theme.preview', { theme: preview })
    )
    await harness.nextMessage()
    harness.send(createPluginRendererRpcRequest(TOKEN, 'theme-commit', 'theme.commit', {}))
    expect(await harness.nextMessage()).toMatchObject({ result: { committed: true } })

    harness.bridge.dispose()
    await Promise.resolve()
    expect(harness.setTheme).toHaveBeenCalledTimes(1)
  })
})

import { Permission } from '../../shared/types/permissions'
import {
  createPluginRendererRpcEvent,
  createPluginRendererRpcFailureResponse,
  createPluginRendererRpcInit,
  createPluginRendererRpcSuccessResponse,
  parsePluginRendererRpcEnvelope,
  PluginRendererRpcPendingRequests,
  PluginRendererRpcValidationError
} from '../../shared/plugin-renderer-rpc'
import type {
  PluginRendererRpcError,
  PluginRendererRpcEventMap,
  PluginRendererRpcEventName,
  PluginRendererRpcJsonObject,
  PluginRendererRpcJsonValue,
  PluginRendererRpcRequest
} from '../../shared/types/plugin-renderer-rpc.types'
import type { PluginConfig } from '../../shared/types/plugin.types'
import type { ToolboxTheme } from '../../shared/types/theme.types'

const CONNECT_MESSAGE_KIND = 'cruciblebox-plugin-connect'
const PORT_MESSAGE_KIND = 'cruciblebox-plugin-port'

export interface PluginFrameMessageTarget {
  addEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
  removeEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
}

export interface PluginFrameBridgeOptions {
  token: string
  origin: string
  permissions: readonly Permission[]
  initialConfig: PluginConfig
  initialTheme: ToolboxTheme
  sendToBackend(message: PluginRendererRpcJsonValue): Promise<unknown>
  updateConfig(config: PluginConfig): Promise<void> | void
  showNotification(title: string, body?: string): boolean | void
  getTheme(): ToolboxTheme
  listThemes(): Promise<ToolboxTheme[]>
  setTheme(theme: ToolboxTheme): Promise<boolean>
  confirm(options: {
    title: string
    message: string
    confirmLabel?: string
    cancelLabel?: string
  }): Promise<boolean>
  resize(height: number): void
  onReady?(): void
  onProtocolError?(error: Error): void
  messageTarget?: PluginFrameMessageTarget
}

class NotAllowedError extends Error {}

export class PluginFrameBridge {
  private readonly inflight = new PluginRendererRpcPendingRequests()
  private readonly options: PluginFrameBridgeOptions
  private readonly messageTarget: PluginFrameMessageTarget
  private port: MessagePort | null = null
  private targetWindow: Window | null = null
  private config: PluginConfig
  private theme: ToolboxTheme
  private themePreviewOriginal: ToolboxTheme | null = null
  private themeOperation: Promise<void> = Promise.resolve()
  private ready = false
  private disposed = false

  constructor(options: PluginFrameBridgeOptions) {
    this.options = options
    this.messageTarget = options.messageTarget ?? window
    this.config = options.initialConfig
    this.theme = options.initialTheme
  }

  connect(targetWindow: Window): void {
    if (this.disposed) throw new Error('Plugin frame bridge is disposed')
    if (this.port) throw new Error('Plugin frame bridge is already connected')
    this.targetWindow = targetWindow
    this.messageTarget.addEventListener('message', this.handlePortTransfer)
    targetWindow.postMessage(
      { kind: CONNECT_MESSAGE_KIND, v: 1, token: this.options.token },
      this.options.origin
    )
  }

  updateConfig(config: PluginConfig): void {
    this.config = config
    if (this.ready) {
      this.sendEvent('state.configChanged', { config: config as PluginRendererRpcJsonObject })
    }
  }

  updateTheme(theme: ToolboxTheme): void {
    this.theme = theme
    if (this.ready) this.sendEvent('theme.changed', { theme })
  }

  sendBackendMessage(message: PluginRendererRpcJsonValue): void {
    if (this.ready) this.sendEvent('backend.message', { message })
  }

  dispose(): void {
    if (this.disposed) return
    void this.enqueueThemeOperation(() => this.rollbackThemePreview()).catch((error: unknown) => {
      this.options.onProtocolError?.(error instanceof Error ? error : new Error(String(error)))
    })
    if (this.ready) {
      try {
        this.sendEvent('host.dispose', {})
      } catch {
        // The frame may already be gone.
      }
    }
    this.disposed = true
    this.ready = false
    this.inflight.clear()
    this.messageTarget.removeEventListener('message', this.handlePortTransfer)
    this.targetWindow = null
    this.port?.close()
    this.port = null
  }

  private readonly handlePortTransfer = (event: MessageEvent<unknown>): void => {
    if (
      this.disposed ||
      this.port ||
      event.source !== this.targetWindow ||
      event.origin !== this.options.origin ||
      event.ports.length !== 1 ||
      typeof event.data !== 'object' ||
      event.data === null ||
      (event.data as Record<string, unknown>).kind !== PORT_MESSAGE_KIND ||
      (event.data as Record<string, unknown>).v !== 1 ||
      (event.data as Record<string, unknown>).token !== this.options.token
    ) {
      return
    }
    this.messageTarget.removeEventListener('message', this.handlePortTransfer)
    this.port = event.ports[0]
    this.port.onmessage = (portEvent) => this.handleMessage(portEvent)
    this.port.start()
    this.port.postMessage(createPluginRendererRpcInit(this.options.token))
  }

  private handleMessage(event: MessageEvent<unknown>): void {
    const parsed = parsePluginRendererRpcEnvelope(event.data)
    if (!parsed.ok || parsed.value.token !== this.options.token || this.disposed) return
    if (parsed.value.kind === 'ready') {
      if (this.ready) return
      this.ready = true
      this.sendEvent('state.initialize', {
        config: this.config as PluginRendererRpcJsonObject,
        theme: this.theme
      })
      this.options.onReady?.()
      return
    }
    if (parsed.value.kind === 'request') void this.handleRequest(parsed.value)
  }

  private async handleRequest(request: PluginRendererRpcRequest): Promise<void> {
    let tracked = false
    try {
      if (!this.ready) throw new NotAllowedError('Plugin frame is not initialized')
      this.inflight.add(request.requestId)
      tracked = true
      const result = await this.dispatch(request)
      this.port?.postMessage(
        createPluginRendererRpcSuccessResponse(
          this.options.token,
          request.requestId,
          request.method,
          result as never
        )
      )
    } catch (error) {
      this.options.onProtocolError?.(error instanceof Error ? error : new Error(String(error)))
      const rpcError = this.toRpcError(error)
      this.port?.postMessage(
        createPluginRendererRpcFailureResponse(this.options.token, request.requestId, rpcError)
      )
    } finally {
      if (tracked) this.inflight.delete(request.requestId)
    }
  }

  private async dispatch(request: PluginRendererRpcRequest): Promise<unknown> {
    switch (request.method) {
      case 'backend.send':
        return { value: await this.options.sendToBackend(request.params.message) }
      case 'notification.show':
        this.assertPermission(Permission.Notification)
        return {
          shown: this.options.showNotification(request.params.title, request.params.body) !== false
        }
      case 'config.update':
        await this.options.updateConfig(request.params.config)
        return { accepted: true }
      case 'theme.get':
        return { theme: this.options.getTheme() }
      case 'theme.list':
        return { themes: await this.options.listThemes() }
      case 'theme.preview':
        this.assertPermission(Permission.ThemeWrite)
        return {
          applied: await this.enqueueThemeOperation(() => this.previewTheme(request.params.theme))
        }
      case 'theme.commit':
        this.assertPermission(Permission.ThemeWrite)
        return {
          committed: await this.enqueueThemeOperation(async () => {
            const committed = this.themePreviewOriginal !== null
            this.themePreviewOriginal = null
            return committed
          })
        }
      case 'theme.rollback':
        this.assertPermission(Permission.ThemeWrite)
        return { restored: await this.enqueueThemeOperation(() => this.rollbackThemePreview()) }
      case 'theme.set':
        this.assertPermission(Permission.ThemeWrite)
        return {
          applied: await this.enqueueThemeOperation(async () => {
            const applied = await this.options.setTheme(request.params.theme)
            if (applied) this.themePreviewOriginal = null
            return applied
          })
        }
      case 'dialog.confirm':
        return { confirmed: await this.options.confirm(request.params) }
      case 'layout.resize':
        this.options.resize(request.params.height)
        return { applied: true }
    }
  }

  private sendEvent<EventName extends PluginRendererRpcEventName>(
    event: EventName,
    data: PluginRendererRpcEventMap[EventName]
  ): void {
    if (!this.port || this.disposed) return
    this.port.postMessage(createPluginRendererRpcEvent(this.options.token, event, data))
  }

  private assertPermission(permission: Permission): void {
    if (!this.options.permissions.includes(permission)) {
      throw new NotAllowedError(`Plugin renderer is missing permission: ${permission}`)
    }
  }

  private async previewTheme(theme: ToolboxTheme): Promise<boolean> {
    const captured = this.themePreviewOriginal === null
    if (captured) this.themePreviewOriginal = this.options.getTheme()
    const applied = await this.options.setTheme(theme)
    if (!applied && captured) this.themePreviewOriginal = null
    return applied
  }

  private async rollbackThemePreview(): Promise<boolean> {
    const original = this.themePreviewOriginal
    if (original === null) return false
    const restored = await this.options.setTheme(original)
    if (restored) this.themePreviewOriginal = null
    return restored
  }

  private enqueueThemeOperation<Result>(operation: () => Promise<Result>): Promise<Result> {
    const result = this.themeOperation.then(operation, operation)
    this.themeOperation = result.then(
      () => undefined,
      () => undefined
    )
    return result
  }

  private toRpcError(error: unknown): PluginRendererRpcError {
    if (error instanceof NotAllowedError) {
      return { code: 'NOT_ALLOWED', message: error.message, retryable: false }
    }
    if (error instanceof PluginRendererRpcValidationError) {
      return { code: error.issue.code, message: error.issue.message, retryable: false }
    }
    return {
      code: 'INTERNAL_ERROR',
      message:
        error instanceof Error
          ? error.message
          : typeof error === 'string'
            ? error
            : 'Plugin renderer request failed',
      retryable: false
    }
  }
}

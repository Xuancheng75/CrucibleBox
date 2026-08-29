import type { ToolboxTheme } from './theme.types'

export const PLUGIN_RENDERER_RPC_VERSION = 1 as const

export type PluginRendererRpcVersion = typeof PLUGIN_RENDERER_RPC_VERSION

export type PluginRendererRpcJsonPrimitive = string | number | boolean | null

export type PluginRendererRpcJsonValue =
  | PluginRendererRpcJsonPrimitive
  | PluginRendererRpcJsonValue[]
  | { [key: string]: PluginRendererRpcJsonValue }

export type PluginRendererRpcJsonObject = Record<string, PluginRendererRpcJsonValue>

export interface PluginRendererRpcMethodMap {
  'backend.send': {
    params: { message: PluginRendererRpcJsonValue }
    result: { value: PluginRendererRpcJsonValue }
  }
  'notification.show': {
    params: { title: string; body?: string }
    result: { shown: boolean }
  }
  'config.update': {
    params: { config: PluginRendererRpcJsonObject }
    result: { accepted: boolean }
  }
  'theme.get': {
    params: Record<string, never>
    result: { theme: ToolboxTheme }
  }
  'theme.list': {
    params: Record<string, never>
    result: { themes: ToolboxTheme[] }
  }
  'theme.preview': {
    params: { theme: ToolboxTheme }
    result: { applied: boolean }
  }
  'theme.commit': {
    params: Record<string, never>
    result: { committed: boolean }
  }
  'theme.rollback': {
    params: Record<string, never>
    result: { restored: boolean }
  }
  'theme.set': {
    params: { theme: ToolboxTheme }
    result: { applied: boolean }
  }
  'dialog.confirm': {
    params: {
      title: string
      message: string
      confirmLabel?: string
      cancelLabel?: string
    }
    result: { confirmed: boolean }
  }
  'dialog.open': {
    params: {
      type: 'file' | 'folder'
      multiple?: boolean
      extensions?: string[]
    }
    result: { paths: string[] }
  }
  'layout.resize': {
    params: { height: number }
    result: { applied: boolean }
  }
}

export type PluginRendererRpcMethod = keyof PluginRendererRpcMethodMap

export interface PluginRendererRpcEventMap {
  'state.initialize': {
    config: PluginRendererRpcJsonObject
    theme: ToolboxTheme
  }
  'state.configChanged': {
    config: PluginRendererRpcJsonObject
  }
  'theme.changed': {
    theme: ToolboxTheme
  }
  'backend.message': {
    message: PluginRendererRpcJsonValue
  }
  'host.filesDropped': {
    paths: string[]
  }
  'host.dispose': Record<string, never>
}

export type PluginRendererRpcEventName = keyof PluginRendererRpcEventMap

export type PluginRendererRpcErrorCode =
  | 'INVALID_ENVELOPE'
  | 'INVALID_VERSION'
  | 'INVALID_TOKEN'
  | 'INVALID_REQUEST_ID'
  | 'UNKNOWN_METHOD'
  | 'UNKNOWN_EVENT'
  | 'INVALID_PARAMS'
  | 'INVALID_RESULT'
  | 'PAYLOAD_TOO_LARGE'
  | 'PAYLOAD_TOO_DEEP'
  | 'PAYLOAD_TOO_COMPLEX'
  | 'TOO_MANY_PENDING_REQUESTS'
  | 'REQUEST_TIMEOUT'
  | 'NOT_ALLOWED'
  | 'HOST_DISPOSED'
  | 'INTERNAL_ERROR'

export interface PluginRendererRpcError {
  code: PluginRendererRpcErrorCode
  message: string
  retryable: boolean
  details?: PluginRendererRpcJsonObject
}

interface PluginRendererRpcEnvelopeBase {
  v: PluginRendererRpcVersion
  token: string
}

export interface PluginRendererRpcInit extends PluginRendererRpcEnvelopeBase {
  kind: 'init'
}

export interface PluginRendererRpcReady extends PluginRendererRpcEnvelopeBase {
  kind: 'ready'
}

export type PluginRendererRpcRequest<
  Method extends PluginRendererRpcMethod = PluginRendererRpcMethod
> = Method extends PluginRendererRpcMethod
  ? PluginRendererRpcEnvelopeBase & {
      kind: 'request'
      requestId: string
      method: Method
      params: PluginRendererRpcMethodMap[Method]['params']
    }
  : never

export type PluginRendererRpcSuccessResponse<
  Method extends PluginRendererRpcMethod = PluginRendererRpcMethod
> = Method extends PluginRendererRpcMethod
  ? PluginRendererRpcEnvelopeBase & {
      kind: 'response'
      requestId: string
      ok: true
      result: PluginRendererRpcMethodMap[Method]['result']
    }
  : never

export interface PluginRendererRpcFailureResponse extends PluginRendererRpcEnvelopeBase {
  kind: 'response'
  requestId: string
  ok: false
  error: PluginRendererRpcError
}

export type PluginRendererRpcResponse<
  Method extends PluginRendererRpcMethod = PluginRendererRpcMethod
> = PluginRendererRpcSuccessResponse<Method> | PluginRendererRpcFailureResponse

export type PluginRendererRpcEvent<
  EventName extends PluginRendererRpcEventName = PluginRendererRpcEventName
> = EventName extends PluginRendererRpcEventName
  ? PluginRendererRpcEnvelopeBase & {
      kind: 'event'
      event: EventName
      data: PluginRendererRpcEventMap[EventName]
    }
  : never

export type PluginRendererRpcEnvelope =
  | PluginRendererRpcInit
  | PluginRendererRpcReady
  | PluginRendererRpcRequest
  | PluginRendererRpcResponse
  | PluginRendererRpcEvent

export interface PluginRendererRpcBudget {
  maxSerializedBytes: number
  maxDepth: number
  maxNodes: number
  maxArrayLength: number
  maxObjectKeys: number
  maxStringBytes: number
}

export interface PluginRendererRpcPayloadStats {
  serializedBytes: number
  depth: number
  nodes: number
}

export interface PluginRendererRpcValidationIssue {
  code: PluginRendererRpcErrorCode
  message: string
  path: string
}

export type PluginRendererRpcParseResult =
  | { ok: true; value: PluginRendererRpcEnvelope }
  | { ok: false; issue: PluginRendererRpcValidationIssue }

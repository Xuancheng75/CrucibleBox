export const PLUGIN_BACKEND_RPC_VERSION = 2 as const

export type PluginBackendRpcJsonPrimitive = string | number | boolean | null
export type PluginBackendRpcJsonValue =
  | PluginBackendRpcJsonPrimitive
  | PluginBackendRpcJsonValue[]
  | { [key: string]: PluginBackendRpcJsonValue }

export interface PluginBackendHostMethodMap {
  'db.query': {
    params: { sql: string; params?: PluginBackendRpcJsonValue[] }
    result: PluginBackendRpcJsonValue[]
  }
  'db.execute': {
    params: { sql: string; params?: PluginBackendRpcJsonValue[] }
    result: null
  }
  'storage.get': {
    params: { key: string }
    result: PluginBackendRpcJsonValue
  }
  'storage.set': {
    params: { key: string; value: PluginBackendRpcJsonValue }
    result: null
  }
  'storage.delete': {
    params: { key: string }
    result: null
  }
  'storage.list': {
    params: { prefix?: string }
    result: { key: string; value: PluginBackendRpcJsonValue }[]
  }
  'storage.batch': {
    params: {
      mutations: (
        | { type: 'set'; key: string; value: PluginBackendRpcJsonValue }
        | { type: 'delete'; key: string }
      )[]
    }
    result: null
  }
  'log.write': {
    params: {
      level: 'debug' | 'info' | 'warn' | 'error'
      message: string
      args: PluginBackendRpcJsonValue[]
    }
    result: null
  }
  'notification.show': {
    params: { title: string; body?: string }
    result: null
  }
  'dialog.open': {
    params: { type: 'file' | 'folder' }
    result: string | null
  }
  'network.fetch': {
    params: {
      url: string
      options?: {
        method?: string
        headers?: Record<string, string>
        body?: string
      }
    }
    result: {
      ok: boolean
      status: number
      statusText: string
      headers: Record<string, string>
      body: string
    }
  }
  'file.read': {
    params: { path: string }
    result: { base64: string }
  }
  'file.write': {
    params: { path: string; base64: string }
    result: null
  }
  'shortcut.register': {
    params: { keys: string }
    result: null
  }
  'shortcut.unregister': {
    params: { keys: string }
    result: null
  }
  'event.emit': {
    params: { event: string; data?: PluginBackendRpcJsonValue }
    result: null
  }
  'event.subscribe': {
    params: { event: string; subscriptionId: string }
    result: null
  }
  'event.unsubscribe': {
    params: { subscriptionId: string }
    result: null
  }
  'trusted.invoke': {
    params: {
      service: string
      operation: string
      payload?: PluginBackendRpcJsonValue
    }
    result: PluginBackendRpcJsonValue
  }
}

export interface PluginBackendWorkerMethodMap {
  'lifecycle.initialize': {
    params: { pluginId: string; config: Record<string, PluginBackendRpcJsonValue> }
    result: null
  }
  'lifecycle.dispose': {
    params: Record<string, never>
    result: null
  }
  'plugin.message': {
    params: { message?: PluginBackendRpcJsonValue }
    result: PluginBackendRpcJsonValue
  }
  'host.event': {
    params: { event: string; data?: PluginBackendRpcJsonValue }
    result: null
  }
}

export type PluginBackendHostMethod = keyof PluginBackendHostMethodMap
export type PluginBackendWorkerMethod = keyof PluginBackendWorkerMethodMap
export type PluginBackendRpcMethod = PluginBackendHostMethod | PluginBackendWorkerMethod

export interface PluginBackendRpcError {
  code: 'INVALID_MESSAGE' | 'NOT_ALLOWED' | 'TIMEOUT' | 'DISPOSED' | 'INTERNAL_ERROR'
  message: string
}

export type PluginBackendRpcParams<Method extends PluginBackendRpcMethod> =
  Method extends PluginBackendHostMethod
    ? PluginBackendHostMethodMap[Method]['params']
    : Method extends PluginBackendWorkerMethod
      ? PluginBackendWorkerMethodMap[Method]['params']
      : never

export type PluginBackendRpcRequest<
  Method extends PluginBackendRpcMethod = PluginBackendRpcMethod
> = Method extends PluginBackendRpcMethod
  ? {
      v: typeof PLUGIN_BACKEND_RPC_VERSION
      kind: 'request'
      token: string
      requestId: string
      method: Method
      params: PluginBackendRpcParams<Method>
    }
  : never

export interface PluginBackendRpcSuccessResponse {
  v: typeof PLUGIN_BACKEND_RPC_VERSION
  kind: 'response'
  token: string
  requestId: string
  ok: true
  result: PluginBackendRpcJsonValue
}

export interface PluginBackendRpcFailureResponse {
  v: typeof PLUGIN_BACKEND_RPC_VERSION
  kind: 'response'
  token: string
  requestId: string
  ok: false
  error: PluginBackendRpcError
}

export interface PluginBackendRpcFatal {
  v: typeof PLUGIN_BACKEND_RPC_VERSION
  kind: 'fatal'
  token: string
  error: PluginBackendRpcError
}

export type PluginBackendRpcResponse =
  PluginBackendRpcSuccessResponse | PluginBackendRpcFailureResponse

export type PluginBackendRpcEnvelope =
  PluginBackendRpcRequest | PluginBackendRpcResponse | PluginBackendRpcFatal

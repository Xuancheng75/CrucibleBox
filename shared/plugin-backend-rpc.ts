import { inspectPluginRendererRpcPayload } from './plugin-renderer-rpc'
import {
  PLUGIN_BACKEND_RPC_VERSION,
  type PluginBackendHostMethod,
  type PluginBackendRpcEnvelope,
  type PluginBackendRpcError,
  type PluginBackendRpcJsonValue,
  type PluginBackendRpcMethod,
  type PluginBackendRpcRequest,
  type PluginBackendRpcResponse,
  type PluginBackendWorkerMethod
} from './types/plugin-backend-rpc.types'

export const PLUGIN_BACKEND_RPC_MAX_PENDING_REQUESTS = 64
const TOKEN_PATTERN = /^[A-Za-z0-9_-]{32,128}$/
const REQUEST_ID_PATTERN = /^[A-Za-z0-9._:-]{1,64}$/

function containsControlCharacter(value: string): boolean {
  return Array.from(value).some((character) => {
    const code = character.charCodeAt(0)
    return code <= 31 || code === 127
  })
}

const HOST_METHODS = new Set<PluginBackendHostMethod>([
  'db.query',
  'db.execute',
  'storage.get',
  'storage.set',
  'storage.delete',
  'storage.list',
  'storage.batch',
  'log.write',
  'notification.show',
  'dialog.open',
  'network.fetch',
  'file.read',
  'file.write',
  'shortcut.register',
  'shortcut.unregister',
  'event.emit',
  'event.subscribe',
  'event.unsubscribe',
  'trusted.invoke'
])

const WORKER_METHODS = new Set<PluginBackendWorkerMethod>([
  'lifecycle.initialize',
  'lifecycle.dispose',
  'plugin.message',
  'host.event'
])

export class PluginBackendRpcValidationError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'PluginBackendRpcValidationError'
  }
}

function fail(message: string): never {
  throw new PluginBackendRpcValidationError(message)
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

function exactObject(
  value: unknown,
  required: readonly string[],
  optional: readonly string[] = []
): Record<string, unknown> {
  if (!isPlainObject(value)) fail('expected a plain object')
  const keys = Object.keys(value)
  const allowed = new Set([...required, ...optional])
  if (keys.some((key) => !allowed.has(key))) fail('unexpected field')
  if (required.some((key) => !Object.prototype.hasOwnProperty.call(value, key))) {
    fail('missing required field')
  }
  return value
}

function boundedString(value: unknown, max: number): string {
  if (
    typeof value !== 'string' ||
    value.length < 1 ||
    value.length > max ||
    containsControlCharacter(value)
  ) {
    fail('invalid string')
  }
  return value
}

function json(value: unknown): PluginBackendRpcJsonValue {
  inspectPluginRendererRpcPayload(value)
  return value as PluginBackendRpcJsonValue
}

function validateParams(method: PluginBackendRpcMethod, value: unknown): void {
  switch (method) {
    case 'db.query':
    case 'db.execute': {
      const params = exactObject(value, ['sql'], ['params'])
      boundedString(params.sql, 64 * 1024)
      if (params.params !== undefined && !Array.isArray(params.params))
        fail('params must be an array')
      json(params.params ?? [])
      return
    }
    case 'storage.get':
    case 'storage.delete': {
      const params = exactObject(value, ['key'])
      boundedString(params.key, 256)
      return
    }
    case 'storage.set': {
      const params = exactObject(value, ['key', 'value'])
      boundedString(params.key, 256)
      json(params.value)
      return
    }
    case 'storage.list': {
      const params = exactObject(value, [], ['prefix'])
      if (
        params.prefix !== undefined &&
        (typeof params.prefix !== 'string' ||
          params.prefix.length > 256 ||
          containsControlCharacter(params.prefix))
      ) {
        fail('invalid prefix')
      }
      return
    }
    case 'storage.batch': {
      const params = exactObject(value, ['mutations'])
      if (
        !Array.isArray(params.mutations) ||
        params.mutations.length < 1 ||
        params.mutations.length > 64
      ) {
        fail('invalid storage batch')
      }
      for (const mutationValue of params.mutations) {
        const mutation = exactObject(mutationValue, ['type', 'key'], ['value'])
        boundedString(mutation.key, 256)
        if (mutation.type === 'set') {
          if (!Object.prototype.hasOwnProperty.call(mutation, 'value')) fail('missing value')
          json(mutation.value)
        } else if (mutation.type === 'delete') {
          if (Object.prototype.hasOwnProperty.call(mutation, 'value')) fail('unexpected value')
        } else {
          fail('invalid storage mutation')
        }
      }
      return
    }
    case 'log.write': {
      const params = exactObject(value, ['level', 'message', 'args'])
      if (!['debug', 'info', 'warn', 'error'].includes(String(params.level)))
        fail('invalid log level')
      boundedString(params.message, 64 * 1024)
      if (!Array.isArray(params.args)) fail('args must be an array')
      json(params.args)
      return
    }
    case 'notification.show': {
      const params = exactObject(value, ['title'], ['body'])
      boundedString(params.title, 512)
      if (params.body !== undefined) boundedString(params.body, 4096)
      return
    }
    case 'dialog.open': {
      const params = exactObject(value, ['type'])
      if (params.type !== 'file' && params.type !== 'folder') fail('invalid dialog type')
      return
    }
    case 'network.fetch': {
      const params = exactObject(value, ['url'], ['options'])
      boundedString(params.url, 8192)
      if (params.options !== undefined) json(params.options)
      return
    }
    case 'file.read': {
      const params = exactObject(value, ['path'])
      boundedString(params.path, 32768)
      return
    }
    case 'file.write': {
      const params = exactObject(value, ['path', 'base64'])
      boundedString(params.path, 32768)
      boundedString(params.base64, 8 * 1024 * 1024)
      return
    }
    case 'shortcut.register':
    case 'shortcut.unregister': {
      const params = exactObject(value, ['keys'])
      boundedString(params.keys, 256)
      return
    }
    case 'event.emit':
    case 'host.event': {
      const params = exactObject(value, ['event'], ['data'])
      boundedString(params.event, 256)
      if (params.data !== undefined) json(params.data)
      return
    }
    case 'event.subscribe': {
      const params = exactObject(value, ['event', 'subscriptionId'])
      boundedString(params.event, 256)
      boundedString(params.subscriptionId, 128)
      return
    }
    case 'event.unsubscribe': {
      const params = exactObject(value, ['subscriptionId'])
      boundedString(params.subscriptionId, 128)
      return
    }
    case 'trusted.invoke': {
      const params = exactObject(value, ['service', 'operation'], ['payload'])
      boundedString(params.service, 128)
      boundedString(params.operation, 128)
      if (params.payload !== undefined) json(params.payload)
      return
    }
    case 'lifecycle.initialize': {
      const params = exactObject(value, ['pluginId', 'config'])
      boundedString(params.pluginId, 128)
      if (!isPlainObject(params.config)) fail('config must be an object')
      json(params.config)
      return
    }
    case 'lifecycle.dispose':
      exactObject(value, [])
      return
    case 'plugin.message': {
      const params = exactObject(value, [], ['message'])
      if (params.message !== undefined) json(params.message)
      return
    }
  }
}

export function validatePluginBackendRpcEnvelope(value: unknown): PluginBackendRpcEnvelope {
  const envelope = exactObject(
    value,
    ['v', 'kind', 'token'],
    ['requestId', 'method', 'params', 'ok', 'result', 'error']
  )
  if (envelope.v !== PLUGIN_BACKEND_RPC_VERSION) fail('unsupported backend RPC version')
  if (typeof envelope.token !== 'string' || !TOKEN_PATTERN.test(envelope.token)) {
    fail('invalid session token')
  }

  if (envelope.kind === 'request') {
    if (typeof envelope.requestId !== 'string' || !REQUEST_ID_PATTERN.test(envelope.requestId)) {
      fail('invalid request id')
    }
    if (
      typeof envelope.method !== 'string' ||
      (!HOST_METHODS.has(envelope.method as PluginBackendHostMethod) &&
        !WORKER_METHODS.has(envelope.method as PluginBackendWorkerMethod))
    ) {
      fail('unknown method')
    }
    if ('ok' in envelope || 'result' in envelope || 'error' in envelope)
      fail('invalid request fields')
    validateParams(envelope.method as PluginBackendRpcMethod, envelope.params)
  } else if (envelope.kind === 'response') {
    if (typeof envelope.requestId !== 'string' || !REQUEST_ID_PATTERN.test(envelope.requestId)) {
      fail('invalid request id')
    }
    if ('method' in envelope || 'params' in envelope) fail('invalid response fields')
    if (envelope.ok === true) {
      if (!('result' in envelope) || 'error' in envelope) fail('invalid success response')
      json(envelope.result)
    } else if (envelope.ok === false) {
      if (!('error' in envelope) || 'result' in envelope) fail('invalid failure response')
      const error = exactObject(envelope.error, ['code', 'message'])
      if (
        !['INVALID_MESSAGE', 'NOT_ALLOWED', 'TIMEOUT', 'DISPOSED', 'INTERNAL_ERROR'].includes(
          String(error.code)
        )
      ) {
        fail('invalid error code')
      }
      boundedString(error.message, 4096)
    } else fail('invalid response status')
  } else if (envelope.kind === 'fatal') {
    if (
      'requestId' in envelope ||
      'method' in envelope ||
      'params' in envelope ||
      'ok' in envelope ||
      'result' in envelope
    ) {
      fail('invalid fatal fields')
    }
    const error = exactObject(envelope.error, ['code', 'message'])
    if (error.code !== 'INTERNAL_ERROR') fail('invalid fatal error code')
    boundedString(error.message, 4096)
  } else fail('unknown envelope kind')

  inspectPluginRendererRpcPayload(value)
  return value as PluginBackendRpcEnvelope
}

export function createPluginBackendRpcRequest<Method extends PluginBackendRpcMethod>(
  token: string,
  requestId: string,
  method: Method,
  params: unknown
): PluginBackendRpcRequest<Method> {
  return validatePluginBackendRpcEnvelope({
    v: PLUGIN_BACKEND_RPC_VERSION,
    kind: 'request',
    token,
    requestId,
    method,
    params
  }) as PluginBackendRpcRequest<Method>
}

export function createPluginBackendRpcResponse(
  token: string,
  requestId: string,
  result: PluginBackendRpcJsonValue
): PluginBackendRpcResponse {
  return validatePluginBackendRpcEnvelope({
    v: PLUGIN_BACKEND_RPC_VERSION,
    kind: 'response',
    token,
    requestId,
    ok: true,
    result
  }) as PluginBackendRpcResponse
}

export function createPluginBackendRpcErrorResponse(
  token: string,
  requestId: string,
  error: PluginBackendRpcError
): PluginBackendRpcResponse {
  return validatePluginBackendRpcEnvelope({
    v: PLUGIN_BACKEND_RPC_VERSION,
    kind: 'response',
    token,
    requestId,
    ok: false,
    error
  }) as PluginBackendRpcResponse
}

export function isPluginBackendHostMethod(
  method: PluginBackendRpcMethod
): method is PluginBackendHostMethod {
  return HOST_METHODS.has(method as PluginBackendHostMethod)
}

export function isPluginBackendWorkerMethod(
  method: PluginBackendRpcMethod
): method is PluginBackendWorkerMethod {
  return WORKER_METHODS.has(method as PluginBackendWorkerMethod)
}

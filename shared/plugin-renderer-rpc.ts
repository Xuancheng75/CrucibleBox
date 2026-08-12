import type { ToolboxTheme } from './types/theme.types'
import {
  PLUGIN_RENDERER_RPC_VERSION,
  type PluginRendererRpcBudget,
  type PluginRendererRpcEnvelope,
  type PluginRendererRpcError,
  type PluginRendererRpcErrorCode,
  type PluginRendererRpcEvent,
  type PluginRendererRpcEventMap,
  type PluginRendererRpcEventName,
  type PluginRendererRpcFailureResponse,
  type PluginRendererRpcInit,
  type PluginRendererRpcMethod,
  type PluginRendererRpcMethodMap,
  type PluginRendererRpcParseResult,
  type PluginRendererRpcPayloadStats,
  type PluginRendererRpcReady,
  type PluginRendererRpcRequest,
  type PluginRendererRpcResponse,
  type PluginRendererRpcSuccessResponse,
  type PluginRendererRpcValidationIssue
} from './types/plugin-renderer-rpc.types'

export const PLUGIN_RENDERER_RPC_TOKEN_MIN_LENGTH = 16
export const PLUGIN_RENDERER_RPC_TOKEN_MAX_LENGTH = 128
export const PLUGIN_RENDERER_RPC_REQUEST_ID_MAX_LENGTH = 64
export const PLUGIN_RENDERER_RPC_MAX_PENDING_REQUESTS = 64

export const PLUGIN_RENDERER_RPC_BUDGET: Readonly<PluginRendererRpcBudget> = Object.freeze({
  maxSerializedBytes: 256 * 1024,
  maxDepth: 16,
  maxNodes: 4096,
  maxArrayLength: 512,
  maxObjectKeys: 256,
  maxStringBytes: 64 * 1024
})

const METHODS = new Set<PluginRendererRpcMethod>([
  'backend.send',
  'notification.show',
  'config.update',
  'theme.get',
  'theme.list',
  'theme.preview',
  'theme.commit',
  'theme.rollback',
  'theme.set',
  'dialog.confirm',
  'layout.resize'
])

const EVENTS = new Set<PluginRendererRpcEventName>([
  'state.initialize',
  'state.configChanged',
  'theme.changed',
  'backend.message',
  'host.dispose'
])

const ERROR_CODES = new Set<PluginRendererRpcErrorCode>([
  'INVALID_ENVELOPE',
  'INVALID_VERSION',
  'INVALID_TOKEN',
  'INVALID_REQUEST_ID',
  'UNKNOWN_METHOD',
  'UNKNOWN_EVENT',
  'INVALID_PARAMS',
  'INVALID_RESULT',
  'PAYLOAD_TOO_LARGE',
  'PAYLOAD_TOO_DEEP',
  'PAYLOAD_TOO_COMPLEX',
  'TOO_MANY_PENDING_REQUESTS',
  'REQUEST_TIMEOUT',
  'NOT_ALLOWED',
  'HOST_DISPOSED',
  'INTERNAL_ERROR'
])

const THEME_TOKEN_KEYS = [
  'colorBg',
  'colorBgLayout',
  'colorBgContainer',
  'colorBgElevated',
  'colorPrimary',
  'colorPrimaryHover',
  'colorPrimaryBg',
  'colorText',
  'colorTextSecondary',
  'colorTextTertiary',
  'colorBorder',
  'colorBorderSecondary',
  'colorSuccess',
  'colorSuccessBg',
  'colorWarning',
  'colorWarningBg',
  'colorError',
  'colorErrorBg',
  'colorLink',
  'borderRadius',
  'fontFamily'
] as const

export class PluginRendererRpcValidationError extends Error {
  readonly issue: PluginRendererRpcValidationIssue

  constructor(code: PluginRendererRpcErrorCode, message: string, path = '$') {
    super(`${message} at ${path}`)
    this.name = 'PluginRendererRpcValidationError'
    this.issue = { code, message, path }
  }
}

function fail(code: PluginRendererRpcErrorCode, message: string, path: string): never {
  throw new PluginRendererRpcValidationError(code, message, path)
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

function inspectObjectShape(value: object, path: string): PropertyDescriptorMap {
  const keys = Reflect.ownKeys(value)
  if (keys.some((key) => typeof key !== 'string')) {
    fail('INVALID_ENVELOPE', 'symbol properties are not supported', path)
  }
  const descriptors = Object.getOwnPropertyDescriptors(value)
  for (const key of keys as string[]) {
    if (Array.isArray(value) && key === 'length') continue
    const descriptor = descriptors[key]
    if (!descriptor.enumerable || !('value' in descriptor)) {
      fail(
        'INVALID_ENVELOPE',
        'accessors and non-enumerable properties are not supported',
        `${path}.${key}`
      )
    }
  }
  return descriptors
}

function utf8ByteLength(value: string): number {
  let bytes = 0
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code <= 0x7f) bytes += 1
    else if (code <= 0x7ff) bytes += 2
    else if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1)
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4
        index += 1
      } else {
        bytes += 3
      }
    } else bytes += 3
  }
  return bytes
}

function jsonStringByteLength(value: string): number {
  let bytes = 2
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (
      code === 0x22 ||
      code === 0x5c ||
      code === 0x08 ||
      code === 0x09 ||
      code === 0x0a ||
      code === 0x0c ||
      code === 0x0d
    ) {
      bytes += 2
    } else if (code < 0x20 || (code >= 0xd800 && code <= 0xdfff)) {
      const next = value.charCodeAt(index + 1)
      if (code >= 0xd800 && code <= 0xdbff && next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4
        index += 1
      } else {
        bytes += 6
      }
    } else if (code <= 0x7f) bytes += 1
    else if (code <= 0x7ff) bytes += 2
    else bytes += 3
  }
  return bytes
}

export function inspectPluginRendererRpcPayload(
  value: unknown,
  budget: Readonly<PluginRendererRpcBudget> = PLUGIN_RENDERER_RPC_BUDGET
): PluginRendererRpcPayloadStats {
  let serializedBytes = 0
  let nodes = 0
  let maxDepth = 0
  const ancestors = new WeakSet<object>()

  const addBytes = (count: number, path: string): void => {
    serializedBytes += count
    if (serializedBytes > budget.maxSerializedBytes) {
      fail('PAYLOAD_TOO_LARGE', 'payload exceeds serialized byte budget', path)
    }
  }

  const visit = (current: unknown, depth: number, path: string): void => {
    nodes += 1
    if (nodes > budget.maxNodes) fail('PAYLOAD_TOO_COMPLEX', 'payload exceeds node budget', path)
    if (depth > budget.maxDepth) fail('PAYLOAD_TOO_DEEP', 'payload exceeds depth budget', path)
    maxDepth = Math.max(maxDepth, depth)

    if (current === null) {
      addBytes(4, path)
      return
    }
    if (typeof current === 'string') {
      if (utf8ByteLength(current) > budget.maxStringBytes) {
        fail('PAYLOAD_TOO_LARGE', 'string exceeds byte budget', path)
      }
      addBytes(jsonStringByteLength(current), path)
      return
    }
    if (typeof current === 'boolean') {
      addBytes(current ? 4 : 5, path)
      return
    }
    if (typeof current === 'number') {
      if (!Number.isFinite(current)) fail('INVALID_ENVELOPE', 'numbers must be finite', path)
      addBytes(String(Object.is(current, -0) ? 0 : current).length, path)
      return
    }
    if (typeof current !== 'object') {
      fail('INVALID_ENVELOPE', 'payload must contain only JSON-compatible values', path)
    }
    if (ancestors.has(current)) fail('INVALID_ENVELOPE', 'cyclic payloads are not supported', path)
    ancestors.add(current)

    if (Array.isArray(current)) {
      if (current.length > budget.maxArrayLength) {
        fail('PAYLOAD_TOO_COMPLEX', 'array exceeds item budget', path)
      }
      const descriptors = inspectObjectShape(current, path)
      const ownKeys = Reflect.ownKeys(current).filter((key) => key !== 'length')
      if (ownKeys.length !== current.length) {
        fail(
          'INVALID_ENVELOPE',
          'sparse arrays and additional array properties are not supported',
          path
        )
      }
      addBytes(2 + Math.max(0, current.length - 1), path)
      for (let index = 0; index < current.length; index += 1) {
        if (!Object.prototype.hasOwnProperty.call(descriptors, String(index))) {
          fail('INVALID_ENVELOPE', 'sparse arrays are not supported', `${path}[${index}]`)
        }
        visit(descriptors[String(index)].value, depth + 1, `${path}[${index}]`)
      }
    } else {
      if (!isPlainObject(current))
        fail('INVALID_ENVELOPE', 'objects must have a plain prototype', path)
      const descriptors = inspectObjectShape(current, path)
      const keys = Object.keys(descriptors)
      if (keys.length > budget.maxObjectKeys) {
        fail('PAYLOAD_TOO_COMPLEX', 'object exceeds key budget', path)
      }
      addBytes(2 + Math.max(0, keys.length - 1), path)
      for (const key of keys) {
        if (utf8ByteLength(key) > budget.maxStringBytes) {
          fail('PAYLOAD_TOO_LARGE', 'property name exceeds byte budget', `${path}.${key}`)
        }
        addBytes(jsonStringByteLength(key) + 1, path)
        visit(descriptors[key].value, depth + 1, `${path}.${key}`)
      }
    }
    ancestors.delete(current)
  }

  visit(value, 0, '$')
  return { serializedBytes, depth: maxDepth, nodes }
}

function exactObject(
  value: unknown,
  required: readonly string[],
  optional: readonly string[],
  code: PluginRendererRpcErrorCode,
  path: string
): Record<string, unknown> {
  if (!isPlainObject(value)) fail(code, 'expected a plain object', path)
  const descriptors = inspectObjectShape(value, path)
  const allowed = new Set([...required, ...optional])
  for (const key of Object.keys(descriptors)) {
    if (!allowed.has(key)) fail(code, `unexpected field "${key}"`, `${path}.${key}`)
  }
  for (const key of required) {
    if (!Object.prototype.hasOwnProperty.call(descriptors, key)) {
      fail(code, `missing field "${key}"`, `${path}.${key}`)
    }
  }
  return value
}

function boundedString(
  value: unknown,
  minLength: number,
  maxLength: number,
  code: PluginRendererRpcErrorCode,
  path: string
): string {
  if (typeof value !== 'string' || value.length < minLength || value.length > maxLength) {
    fail(code, `expected a string with ${minLength}-${maxLength} characters`, path)
  }
  return value
}

export function validatePluginRendererRpcToken(value: unknown): string {
  const token = boundedString(
    value,
    PLUGIN_RENDERER_RPC_TOKEN_MIN_LENGTH,
    PLUGIN_RENDERER_RPC_TOKEN_MAX_LENGTH,
    'INVALID_TOKEN',
    '$.token'
  )
  if (!/^[A-Za-z0-9_-]+$/.test(token)) {
    fail('INVALID_TOKEN', 'token contains unsupported characters', '$.token')
  }
  return token
}

export function validatePluginRendererRpcRequestId(value: unknown): string {
  const requestId = boundedString(
    value,
    1,
    PLUGIN_RENDERER_RPC_REQUEST_ID_MAX_LENGTH,
    'INVALID_REQUEST_ID',
    '$.requestId'
  )
  if (!/^[A-Za-z0-9._:-]+$/.test(requestId)) {
    fail('INVALID_REQUEST_ID', 'requestId contains unsupported characters', '$.requestId')
  }
  return requestId
}

function validateJsonObject(value: unknown, code: PluginRendererRpcErrorCode, path: string): void {
  if (!isPlainObject(value)) fail(code, 'expected a JSON object', path)
}

function validateTheme(value: unknown, code: PluginRendererRpcErrorCode, path: string): void {
  const theme = exactObject(value, ['id', 'name', 'mode', 'tokens'], [], code, path)
  boundedString(theme.id, 1, 128, code, `${path}.id`)
  boundedString(theme.name, 1, 200, code, `${path}.name`)
  if (theme.mode !== 'light' && theme.mode !== 'dark')
    fail(code, 'invalid theme mode', `${path}.mode`)
  const tokens = exactObject(theme.tokens, THEME_TOKEN_KEYS, [], code, `${path}.tokens`)
  for (const key of THEME_TOKEN_KEYS) {
    if (key === 'borderRadius') {
      if (
        typeof tokens[key] !== 'number' ||
        !Number.isFinite(tokens[key]) ||
        tokens[key] < 0 ||
        tokens[key] > 64
      ) {
        fail(code, 'borderRadius must be between 0 and 64', `${path}.tokens.${key}`)
      }
    } else {
      boundedString(tokens[key], 1, key === 'fontFamily' ? 500 : 256, code, `${path}.tokens.${key}`)
    }
  }
}

export function validatePluginRendererRpcParams<Method extends PluginRendererRpcMethod>(
  method: Method,
  value: unknown
): asserts value is PluginRendererRpcMethodMap[Method]['params'] {
  const code = 'INVALID_PARAMS'
  const path = '$.params'
  switch (method) {
    case 'backend.send': {
      exactObject(value, ['message'], [], code, path)
      return
    }
    case 'notification.show': {
      const params = exactObject(value, ['title'], ['body'], code, path)
      boundedString(params.title, 1, 200, code, `${path}.title`)
      if (params.body !== undefined) boundedString(params.body, 0, 2000, code, `${path}.body`)
      return
    }
    case 'config.update': {
      const params = exactObject(value, ['config'], [], code, path)
      validateJsonObject(params.config, code, `${path}.config`)
      return
    }
    case 'theme.get':
    case 'theme.list':
    case 'theme.commit':
    case 'theme.rollback': {
      exactObject(value, [], [], code, path)
      return
    }
    case 'theme.preview':
    case 'theme.set': {
      const params = exactObject(value, ['theme'], [], code, path)
      validateTheme(params.theme, code, `${path}.theme`)
      return
    }
    case 'dialog.confirm': {
      const params = exactObject(
        value,
        ['title', 'message'],
        ['confirmLabel', 'cancelLabel'],
        code,
        path
      )
      boundedString(params.title, 1, 200, code, `${path}.title`)
      boundedString(params.message, 1, 4000, code, `${path}.message`)
      if (params.confirmLabel !== undefined) {
        boundedString(params.confirmLabel, 1, 80, code, `${path}.confirmLabel`)
      }
      if (params.cancelLabel !== undefined) {
        boundedString(params.cancelLabel, 1, 80, code, `${path}.cancelLabel`)
      }
      return
    }
    case 'layout.resize': {
      const params = exactObject(value, ['height'], [], code, path)
      if (
        !Number.isInteger(params.height) ||
        (params.height as number) < 100 ||
        (params.height as number) > 16384
      ) {
        fail(code, 'height must be an integer between 100 and 16384', `${path}.height`)
      }
      return
    }
  }
}

export function validatePluginRendererRpcResult<Method extends PluginRendererRpcMethod>(
  method: Method,
  value: unknown
): asserts value is PluginRendererRpcMethodMap[Method]['result'] {
  const code = 'INVALID_RESULT'
  const path = '$.result'
  switch (method) {
    case 'backend.send':
      exactObject(value, ['value'], [], code, path)
      return
    case 'notification.show': {
      const result = exactObject(value, ['shown'], [], code, path)
      if (typeof result.shown !== 'boolean') fail(code, 'shown must be boolean', `${path}.shown`)
      return
    }
    case 'config.update': {
      const result = exactObject(value, ['accepted'], [], code, path)
      if (typeof result.accepted !== 'boolean')
        fail(code, 'accepted must be boolean', `${path}.accepted`)
      return
    }
    case 'theme.get': {
      const result = exactObject(value, ['theme'], [], code, path)
      validateTheme(result.theme, code, `${path}.theme`)
      return
    }
    case 'theme.list': {
      const result = exactObject(value, ['themes'], [], code, path)
      if (!Array.isArray(result.themes)) fail(code, 'themes must be an array', `${path}.themes`)
      result.themes.forEach((theme, index) =>
        validateTheme(theme, code, `${path}.themes[${index}]`)
      )
      return
    }
    case 'theme.preview':
    case 'theme.set':
    case 'layout.resize': {
      const result = exactObject(value, ['applied'], [], code, path)
      if (typeof result.applied !== 'boolean')
        fail(code, 'applied must be boolean', `${path}.applied`)
      return
    }
    case 'theme.commit': {
      const result = exactObject(value, ['committed'], [], code, path)
      if (typeof result.committed !== 'boolean')
        fail(code, 'committed must be boolean', `${path}.committed`)
      return
    }
    case 'theme.rollback': {
      const result = exactObject(value, ['restored'], [], code, path)
      if (typeof result.restored !== 'boolean')
        fail(code, 'restored must be boolean', `${path}.restored`)
      return
    }
    case 'dialog.confirm': {
      const result = exactObject(value, ['confirmed'], [], code, path)
      if (typeof result.confirmed !== 'boolean')
        fail(code, 'confirmed must be boolean', `${path}.confirmed`)
      return
    }
  }
}

export function validatePluginRendererRpcEventData<EventName extends PluginRendererRpcEventName>(
  event: EventName,
  value: unknown
): asserts value is PluginRendererRpcEventMap[EventName] {
  const code = 'INVALID_PARAMS'
  const path = '$.data'
  switch (event) {
    case 'state.initialize': {
      const data = exactObject(value, ['config', 'theme'], [], code, path)
      validateJsonObject(data.config, code, `${path}.config`)
      validateTheme(data.theme, code, `${path}.theme`)
      return
    }
    case 'state.configChanged': {
      const data = exactObject(value, ['config'], [], code, path)
      validateJsonObject(data.config, code, `${path}.config`)
      return
    }
    case 'theme.changed': {
      const data = exactObject(value, ['theme'], [], code, path)
      validateTheme(data.theme, code, `${path}.theme`)
      return
    }
    case 'backend.message':
      exactObject(value, ['message'], [], code, path)
      return
    case 'host.dispose':
      exactObject(value, [], [], code, path)
      return
  }
}

function validateError(value: unknown): asserts value is PluginRendererRpcError {
  const error = exactObject(
    value,
    ['code', 'message', 'retryable'],
    ['details'],
    'INVALID_RESULT',
    '$.error'
  )
  if (
    typeof error.code !== 'string' ||
    !ERROR_CODES.has(error.code as PluginRendererRpcErrorCode)
  ) {
    fail('INVALID_RESULT', 'unknown error code', '$.error.code')
  }
  boundedString(error.message, 1, 1000, 'INVALID_RESULT', '$.error.message')
  if (typeof error.retryable !== 'boolean') {
    fail('INVALID_RESULT', 'retryable must be boolean', '$.error.retryable')
  }
  if (error.details !== undefined)
    validateJsonObject(error.details, 'INVALID_RESULT', '$.error.details')
}

function validateBase(envelope: Record<string, unknown>): void {
  if (envelope.v !== PLUGIN_RENDERER_RPC_VERSION) {
    fail('INVALID_VERSION', 'unsupported renderer RPC version', '$.v')
  }
  validatePluginRendererRpcToken(envelope.token)
}

export function validatePluginRendererRpcEnvelope(value: unknown): PluginRendererRpcEnvelope {
  inspectPluginRendererRpcPayload(value)
  if (!isPlainObject(value)) fail('INVALID_ENVELOPE', 'expected a plain envelope object', '$')
  if (typeof value.kind !== 'string') fail('INVALID_ENVELOPE', 'missing envelope kind', '$.kind')

  switch (value.kind) {
    case 'init': {
      const envelope = exactObject(value, ['v', 'kind', 'token'], [], 'INVALID_ENVELOPE', '$')
      validateBase(envelope)
      return value as unknown as PluginRendererRpcInit
    }
    case 'ready': {
      const envelope = exactObject(value, ['v', 'kind', 'token'], [], 'INVALID_ENVELOPE', '$')
      validateBase(envelope)
      return value as unknown as PluginRendererRpcReady
    }
    case 'request': {
      const envelope = exactObject(
        value,
        ['v', 'kind', 'token', 'requestId', 'method', 'params'],
        [],
        'INVALID_ENVELOPE',
        '$'
      )
      validateBase(envelope)
      validatePluginRendererRpcRequestId(envelope.requestId)
      if (
        typeof envelope.method !== 'string' ||
        !METHODS.has(envelope.method as PluginRendererRpcMethod)
      ) {
        fail('UNKNOWN_METHOD', 'unknown renderer RPC method', '$.method')
      }
      const method = envelope.method as PluginRendererRpcMethod
      validatePluginRendererRpcParams(method, envelope.params)
      return value as unknown as PluginRendererRpcRequest
    }
    case 'response': {
      const response = value as Record<string, unknown>
      const ok = response.ok
      const required =
        ok === true
          ? ['v', 'kind', 'token', 'requestId', 'ok', 'result']
          : ['v', 'kind', 'token', 'requestId', 'ok', 'error']
      const envelope = exactObject(value, required, [], 'INVALID_ENVELOPE', '$')
      validateBase(envelope)
      validatePluginRendererRpcRequestId(envelope.requestId)
      if (typeof ok !== 'boolean') fail('INVALID_RESULT', 'ok must be boolean', '$.ok')
      if (ok) {
        return value as unknown as PluginRendererRpcSuccessResponse
      }
      validateError(envelope.error)
      return value as unknown as PluginRendererRpcFailureResponse
    }
    case 'event': {
      const envelope = exactObject(
        value,
        ['v', 'kind', 'token', 'event', 'data'],
        [],
        'INVALID_ENVELOPE',
        '$'
      )
      validateBase(envelope)
      if (
        typeof envelope.event !== 'string' ||
        !EVENTS.has(envelope.event as PluginRendererRpcEventName)
      ) {
        fail('UNKNOWN_EVENT', 'unknown renderer RPC event', '$.event')
      }
      const event = envelope.event as PluginRendererRpcEventName
      validatePluginRendererRpcEventData(event, envelope.data)
      return value as unknown as PluginRendererRpcEvent
    }
    default:
      fail('INVALID_ENVELOPE', 'unknown envelope kind', '$.kind')
  }
}

export function parsePluginRendererRpcEnvelope(value: unknown): PluginRendererRpcParseResult {
  try {
    return { ok: true, value: validatePluginRendererRpcEnvelope(value) }
  } catch (error) {
    if (error instanceof PluginRendererRpcValidationError) return { ok: false, issue: error.issue }
    return {
      ok: false,
      issue: { code: 'INVALID_ENVELOPE', message: 'envelope validation failed', path: '$' }
    }
  }
}

export function validatePluginRendererRpcResponse<Method extends PluginRendererRpcMethod>(
  value: unknown,
  method: Method
): PluginRendererRpcResponse<Method> {
  const envelope = validatePluginRendererRpcEnvelope(value)
  if (envelope.kind !== 'response') {
    fail('INVALID_RESULT', 'expected a response envelope', '$.kind')
  }
  if (envelope.ok) validatePluginRendererRpcResult(method, envelope.result)
  return envelope as PluginRendererRpcResponse<Method>
}

export function createPluginRendererRpcInit(token: string): PluginRendererRpcInit {
  validatePluginRendererRpcToken(token)
  return { v: PLUGIN_RENDERER_RPC_VERSION, kind: 'init', token }
}

export function createPluginRendererRpcReady(token: string): PluginRendererRpcReady {
  validatePluginRendererRpcToken(token)
  return { v: PLUGIN_RENDERER_RPC_VERSION, kind: 'ready', token }
}

export function createPluginRendererRpcRequest<Method extends PluginRendererRpcMethod>(
  token: string,
  requestId: string,
  method: Method,
  params: PluginRendererRpcMethodMap[Method]['params']
): PluginRendererRpcRequest<Method> {
  const value = {
    v: PLUGIN_RENDERER_RPC_VERSION,
    kind: 'request',
    token,
    requestId,
    method,
    params
  } as const
  return validatePluginRendererRpcEnvelope(value) as PluginRendererRpcRequest<Method>
}

export function createPluginRendererRpcSuccessResponse<Method extends PluginRendererRpcMethod>(
  token: string,
  requestId: string,
  method: Method,
  result: PluginRendererRpcMethodMap[Method]['result']
): PluginRendererRpcSuccessResponse<Method> {
  const value = {
    v: PLUGIN_RENDERER_RPC_VERSION,
    kind: 'response',
    token,
    requestId,
    ok: true,
    result
  } as const
  return validatePluginRendererRpcResponse(
    value,
    method
  ) as PluginRendererRpcSuccessResponse<Method>
}

export function createPluginRendererRpcFailureResponse(
  token: string,
  requestId: string,
  error: PluginRendererRpcError
): PluginRendererRpcFailureResponse {
  const value = {
    v: PLUGIN_RENDERER_RPC_VERSION,
    kind: 'response',
    token,
    requestId,
    ok: false,
    error
  } as const
  return validatePluginRendererRpcEnvelope(value) as PluginRendererRpcFailureResponse
}

export function createPluginRendererRpcEvent<EventName extends PluginRendererRpcEventName>(
  token: string,
  event: EventName,
  data: PluginRendererRpcEventMap[EventName]
): PluginRendererRpcEvent<EventName> {
  const value = { v: PLUGIN_RENDERER_RPC_VERSION, kind: 'event', token, event, data } as const
  return validatePluginRendererRpcEnvelope(value) as PluginRendererRpcEvent<EventName>
}

export class PluginRendererRpcPendingRequests {
  private readonly requestIds = new Set<string>()

  constructor(readonly limit = PLUGIN_RENDERER_RPC_MAX_PENDING_REQUESTS) {
    if (!Number.isInteger(limit) || limit < 1 || limit > PLUGIN_RENDERER_RPC_MAX_PENDING_REQUESTS) {
      throw new RangeError(
        `pending request limit must be between 1 and ${PLUGIN_RENDERER_RPC_MAX_PENDING_REQUESTS}`
      )
    }
  }

  get size(): number {
    return this.requestIds.size
  }

  add(requestId: string): void {
    validatePluginRendererRpcRequestId(requestId)
    if (this.requestIds.has(requestId)) {
      fail('INVALID_REQUEST_ID', 'requestId is already pending', '$.requestId')
    }
    if (this.requestIds.size >= this.limit) {
      fail('TOO_MANY_PENDING_REQUESTS', 'pending request limit reached', '$.requestId')
    }
    this.requestIds.add(requestId)
  }

  delete(requestId: string): boolean {
    return this.requestIds.delete(requestId)
  }

  has(requestId: string): boolean {
    return this.requestIds.has(requestId)
  }

  clear(): void {
    this.requestIds.clear()
  }
}

export type {
  PluginRendererRpcEnvelope,
  PluginRendererRpcEvent,
  PluginRendererRpcFailureResponse,
  PluginRendererRpcInit,
  PluginRendererRpcReady,
  PluginRendererRpcRequest,
  PluginRendererRpcResponse,
  PluginRendererRpcSuccessResponse,
  ToolboxTheme
}

// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { randomUUID } from 'crypto'
import { createRequire } from 'module'
import { isAbsolute, relative, resolve } from 'path'
import {
  createPluginBackendRpcErrorResponse,
  createPluginBackendRpcRequest,
  createPluginBackendRpcResponse,
  isPluginBackendWorkerMethod,
  validatePluginBackendRpcEnvelope
} from '@shared/plugin-backend-rpc'
import type {
  PluginBackendHostMethod,
  PluginBackendHostMethodMap,
  PluginBackendRpcJsonValue,
  PluginBackendRpcRequest
} from '@shared/types/plugin-backend-rpc.types'
import type {
  PluginContext,
  PluginDatabaseAPI,
  PluginHostAPI,
  PluginLogger,
  PluginMain,
  PluginStorageAPI
} from '@shared/types/plugin.types'

interface PendingRpc {
  resolve: (value: PluginBackendRpcJsonValue) => void
  reject: (error: Error) => void
  timer: NodeJS.Timeout
}

interface FetchSerialized {
  ok: boolean
  status: number
  statusText: string
  headers: Record<string, string>
  body: string
}

interface PluginRuntimeHostAPI extends PluginHostAPI {
  invokeTrustedService(service: string, operation: string, payload?: unknown): Promise<unknown>
}

const RPC_TIMEOUT = 30_000
const MAX_PENDING_RPCS = 64
const nodeRequire = createRequire(__filename)
const parentPort = process.parentPort
if (!parentPort) throw new Error('Plugin backend must run inside an Electron utility process')
const pendingRpcs = new Map<string, PendingRpc>()
const shortcutHandlers = new Map<string, () => void>()
const eventSubscriptions = new Map<
  string,
  { subscriptionId: string; handlers: Set<(data: unknown) => void> }
>()

let pluginModule: PluginMain | null = null
let sessionToken: string | null = null
let disposed = false
let workerRequestQueue = Promise.resolve()

function sendToParent(message: unknown): void {
  if (!disposed) parentPort.postMessage(message)
}

function normalizeRpcValue(value: unknown): PluginBackendRpcJsonValue {
  return value === undefined ? null : (value as PluginBackendRpcJsonValue)
}

function rpc<Method extends PluginBackendHostMethod>(
  method: Method,
  params: PluginBackendHostMethodMap[Method]['params'],
  timeoutMs = RPC_TIMEOUT
): Promise<PluginBackendRpcJsonValue> {
  if (!sessionToken || disposed) return Promise.reject(new Error('Plugin backend is disposed'))
  if (pendingRpcs.size >= MAX_PENDING_RPCS) {
    return Promise.reject(new Error('Plugin backend has too many pending host requests'))
  }
  return new Promise((resolvePromise, rejectPromise) => {
    const requestId = randomUUID()
    const timer = setTimeout(() => {
      pendingRpcs.delete(requestId)
      rejectPromise(new Error(`Host request timed out: ${method}`))
    }, timeoutMs)
    pendingRpcs.set(requestId, { resolve: resolvePromise, reject: rejectPromise, timer })
    sendToParent(createPluginBackendRpcRequest(sessionToken!, requestId, method, params))
  })
}

function rpcFire<Method extends PluginBackendHostMethod>(
  method: Method,
  params: PluginBackendHostMethodMap[Method]['params']
): void {
  void rpc(method, params).catch(() => undefined)
}

function resolvePluginMain(loaded: unknown): PluginMain | null {
  let candidate = loaded
  const visited = new Set<unknown>()
  while (
    candidate !== null &&
    (typeof candidate === 'object' || typeof candidate === 'function') &&
    !visited.has(candidate)
  ) {
    visited.add(candidate)
    const plugin = candidate as Partial<PluginMain> & { default?: unknown }
    if (typeof plugin.activate === 'function') return plugin as PluginMain
    candidate = plugin.default
  }
  return null
}

function serializeFetchOptions(options?: RequestInit):
  | {
      method?: string
      headers?: Record<string, string>
      body?: string
    }
  | undefined {
  if (!options) return undefined
  if (options.signal) throw new Error('AbortSignal cannot cross the backend RPC boundary')
  if (options.body !== undefined && options.body !== null && typeof options.body !== 'string') {
    throw new Error('Only string request bodies are supported')
  }
  const headers: Record<string, string> = {}
  if (options.headers) new Headers(options.headers).forEach((value, key) => (headers[key] = value))
  return {
    ...(options.method ? { method: options.method } : {}),
    ...(Object.keys(headers).length > 0 ? { headers } : {}),
    ...(typeof options.body === 'string' ? { body: options.body } : {})
  }
}

function toJsonArgs(args: unknown[]): PluginBackendRpcJsonValue[] {
  return args.map((arg) => {
    if (arg === undefined) return null
    try {
      return JSON.parse(JSON.stringify(arg)) as PluginBackendRpcJsonValue
    } catch {
      return String(arg)
    }
  })
}

function buildContext(init: {
  pluginId: string
  config: Record<string, PluginBackendRpcJsonValue>
}): PluginContext {
  const logger: PluginLogger = {
    info: (message, ...args) =>
      rpcFire('log.write', { level: 'info', message, args: toJsonArgs(args) }),
    warn: (message, ...args) =>
      rpcFire('log.write', { level: 'warn', message, args: toJsonArgs(args) }),
    error: (message, ...args) =>
      rpcFire('log.write', { level: 'error', message, args: toJsonArgs(args) }),
    debug: (message, ...args) =>
      rpcFire('log.write', { level: 'debug', message, args: toJsonArgs(args) })
  }

  const database: PluginDatabaseAPI = {
    query: async (sql, params) =>
      (await rpc('db.query', {
        sql,
        ...(params ? { params: params as PluginBackendRpcJsonValue[] } : {})
      })) as unknown[],
    execute: async (sql, params) => {
      await rpc('db.execute', {
        sql,
        ...(params ? { params: params as PluginBackendRpcJsonValue[] } : {})
      })
    }
  }

  const storage: PluginStorageAPI = {
    async get<T = unknown>(key: string): Promise<T | null> {
      return (await rpc('storage.get', { key })) as T | null
    },
    set: async (key, value) => {
      await rpc('storage.set', { key, value: value as PluginBackendRpcJsonValue })
    },
    delete: async (key) => {
      await rpc('storage.delete', { key })
    },
    async list<T = unknown>(prefix?: string): Promise<Array<{ key: string; value: T }>> {
      return (await rpc(
        'storage.list',
        prefix === undefined ? {} : { prefix }
      )) as unknown as Array<{ key: string; value: T }>
    },
    batch: async (mutations) => {
      await rpc('storage.batch', {
        mutations: mutations as Array<
          | { type: 'set'; key: string; value: PluginBackendRpcJsonValue }
          | { type: 'delete'; key: string }
        >
      })
    }
  }

  const api: PluginRuntimeHostAPI = {
    notify: (title, body) =>
      rpcFire('notification.show', { title, ...(body === undefined ? {} : { body }) }),
    openDialog: async (type) => (await rpc('dialog.open', { type })) as string | null,
    fetch: async (url, options) => {
      const response = (await rpc('network.fetch', {
        url,
        ...(options ? { options: serializeFetchOptions(options) } : {})
      })) as unknown as FetchSerialized
      return new Response(response.body, {
        status: response.status,
        statusText: response.statusText,
        headers: response.headers
      })
    },
    readFile: async (path) => {
      const result = (await rpc('file.read', { path })) as unknown as { base64: string }
      return Buffer.from(result.base64, 'base64')
    },
    writeFile: async (path, data) => {
      await rpc('file.write', { path, base64: Buffer.from(data).toString('base64') })
    },
    registerShortcut: (keys, handler) => {
      shortcutHandlers.set(keys, handler)
      rpcFire('shortcut.register', { keys })
      return () => {
        shortcutHandlers.delete(keys)
        rpcFire('shortcut.unregister', { keys })
      }
    },
    emitEvent: (event, data) =>
      rpcFire('event.emit', {
        event,
        ...(data === undefined ? {} : { data: data as PluginBackendRpcJsonValue })
      }),
    onEvent: (event, handler) => {
      let subscription = eventSubscriptions.get(event)
      if (!subscription) {
        subscription = { subscriptionId: randomUUID(), handlers: new Set() }
        eventSubscriptions.set(event, subscription)
        rpcFire('event.subscribe', { event, subscriptionId: subscription.subscriptionId })
      }
      subscription.handlers.add(handler)
      return () => {
        const current = eventSubscriptions.get(event)
        if (!current) return
        current.handlers.delete(handler)
        if (current.handlers.size === 0) {
          eventSubscriptions.delete(event)
          rpcFire('event.unsubscribe', { subscriptionId: current.subscriptionId })
        }
      }
    },
    invokeTrustedService: (service, operation, payload) =>
      rpc('trusted.invoke', {
        service,
        operation,
        ...(payload === undefined ? {} : { payload: payload as PluginBackendRpcJsonValue })
      }),
    clipboard: {
      read: async () => (await rpc('clipboard.read', {})) as { text: string },
      write: async (text: string) => (await rpc('clipboard.write', { text })) as { ok: boolean }
    },
    getSystemInfo: async () =>
      (await rpc('system.info', {})) as {
        os: { name: string; version: string; hostname: string }
        cpu: { brand: string; cores: number; physicalCores: number; usage: number }
        memory: { total: number; available: number; usage: number }
        disks: Array<{ name: string; total: number; available: number }>
        network: Array<{ name: string; ip: string; mac: string }>
      }
  }

  return { id: init.pluginId, config: init.config, logger, database, storage, api }
}

function pluginEntryPath(): string {
  const pluginDirectory = resolve(process.argv[2] ?? '')
  const mainEntry = process.argv[3] ?? ''
  const backendApiVersion = Number(process.argv[4] ?? '1')
  if (backendApiVersion !== 1 && backendApiVersion !== 2) {
    throw new Error('Unsupported backend API version')
  }
  const entryPath = resolve(pluginDirectory, mainEntry)
  const relation = relative(pluginDirectory, entryPath)
  if (!relation || relation.startsWith('..') || isAbsolute(relation)) {
    throw new Error('Plugin main entry escaped its directory')
  }
  return entryPath
}

async function handleWorkerRequest(
  message: PluginBackendRpcRequest
): Promise<PluginBackendRpcJsonValue> {
  if (!isPluginBackendWorkerMethod(message.method)) {
    throw new Error(`Host cannot invoke ${message.method}`)
  }
  switch (message.method) {
    case 'lifecycle.initialize': {
      if (pluginModule || sessionToken === null) throw new Error('Plugin is already initialized')
      const context = buildContext(message.params)
      pluginModule = resolvePluginMain(nodeRequire(pluginEntryPath()))
      if (!pluginModule) throw new Error('Plugin main module must export activate()')
      await pluginModule.activate(context)
      return null
    }
    case 'lifecycle.dispose':
      if (pluginModule?.deactivate) await pluginModule.deactivate()
      return null
    case 'plugin.message':
      return normalizeRpcValue(await pluginModule?.onMessage?.(message.params.message))
    case 'host.event':
      if (message.params.event === 'openbox:shortcut' && typeof message.params.data === 'string') {
        shortcutHandlers.get(message.params.data)?.()
      } else {
        const subscription = eventSubscriptions.get(message.params.event)
        if (subscription) {
          for (const handler of subscription.handlers) handler(message.params.data)
        }
      }
      return null
  }
}

function fatal(error: unknown): void {
  if (!sessionToken || disposed) return
  sendToParent({
    v: 2,
    kind: 'fatal',
    token: sessionToken,
    error: {
      code: 'INTERNAL_ERROR',
      message: error instanceof Error ? error.message : String(error)
    }
  })
}

function queueWorkerRequest(message: PluginBackendRpcRequest): void {
  const operation = workerRequestQueue.then(async () => {
    try {
      const result = await handleWorkerRequest(message)
      sendToParent(createPluginBackendRpcResponse(sessionToken!, message.requestId, result))
      if (message.method === 'lifecycle.dispose') {
        disposed = true
        setImmediate(() => process.exit(0))
      }
    } catch (error) {
      sendToParent(
        createPluginBackendRpcErrorResponse(sessionToken!, message.requestId, {
          code: 'INTERNAL_ERROR',
          message: error instanceof Error ? error.message : String(error)
        })
      )
    }
  })
  workerRequestQueue = operation.catch(fatal)
}

parentPort.on('message', (event) => {
  try {
    const message = validatePluginBackendRpcEnvelope(event.data)
    if (!sessionToken) {
      if (message.kind !== 'request' || message.method !== 'lifecycle.initialize') {
        throw new Error('The first backend message must initialize the lifecycle')
      }
      sessionToken = message.token
    } else if (message.token !== sessionToken) {
      throw new Error('Backend session token mismatch')
    }

    if (message.kind === 'response') {
      const pending = pendingRpcs.get(message.requestId)
      if (!pending) return
      clearTimeout(pending.timer)
      pendingRpcs.delete(message.requestId)
      if (message.ok) pending.resolve(message.result)
      else pending.reject(new Error(message.error.message))
      return
    }
    if (message.kind !== 'request') throw new Error('Unexpected host message')
    queueWorkerRequest(message)
  } catch (error) {
    fatal(error)
  }
})

process.on('uncaughtException', (error) => {
  fatal(error)
  process.exit(1)
})

process.on('unhandledRejection', (error) => {
  fatal(error)
  process.exit(1)
})

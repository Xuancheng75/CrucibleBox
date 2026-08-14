// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { randomBytes, randomUUID } from 'crypto'
import { EventEmitter } from 'events'
import { join } from 'path'
import { utilityProcess } from 'electron'
import {
  createPluginBackendRpcErrorResponse,
  createPluginBackendRpcRequest,
  createPluginBackendRpcResponse,
  isPluginBackendHostMethod,
  validatePluginBackendRpcEnvelope
} from '@shared/plugin-backend-rpc'
import { inspectPluginRendererRpcPayload } from '@shared/plugin-renderer-rpc'
import type {
  PluginBackendRpcJsonValue,
  PluginBackendRpcRequest,
  PluginBackendWorkerMethod
} from '@shared/types/plugin-backend-rpc.types'
import type { PluginContext, PluginMessage } from '@shared/types/plugin.types'

interface PendingRequest {
  resolve: (value: PluginBackendRpcJsonValue) => void
  reject: (error: Error) => void
  timer: NodeJS.Timeout
}

export interface SandboxOptions {
  pluginId: string
  mainEntry: string
  pluginDir: string
  backendApiVersion: 1 | 2
  context: PluginContext
  handler: (method: string, params: unknown) => unknown | Promise<unknown>
}

export interface SandboxExitDetails {
  signal: NodeJS.Signals | null
  expected: boolean
}

export interface PluginSandboxRuntime extends EventEmitter {
  readonly useProcessMode: boolean
  readonly runtimeKind: 'utility-process'
  readonly isRunning: boolean
  start(): Promise<void>
  stop(): Promise<void>
  pushEvent(event: string, data: unknown): void
  sendMessage(message: PluginMessage): Promise<unknown>
}

export type PluginSandboxFactory = (options: SandboxOptions) => PluginSandboxRuntime

export interface PluginWorkerProcess extends EventEmitter {
  readonly pid: number | undefined
  readonly stdout: NodeJS.ReadableStream | null
  readonly stderr: NodeJS.ReadableStream | null
  postMessage(message: unknown): void
  kill(): boolean
}

export interface SpawnPluginWorkerOptions {
  cwd: string
  env: Record<string, string>
  stdio: 'pipe'
  serviceName: string
}

export type SpawnPluginWorker = (
  modulePath: string,
  args: string[],
  options: SpawnPluginWorkerOptions
) => PluginWorkerProcess

export interface PluginSandboxDependencies {
  spawnWorker?: SpawnPluginWorker
  requestTimeoutMs?: number
  startTimeoutMs?: number
  stopGraceMs?: number
}

const REQUEST_TIMEOUT = 30_000
const START_TIMEOUT = 30_000
const STOP_GRACE_TIMEOUT = 3_000

type SandboxState = 'idle' | 'starting' | 'running' | 'stopping' | 'stopped' | 'failed'

function defaultSpawnWorker(
  modulePath: string,
  args: string[],
  options: SpawnPluginWorkerOptions
): PluginWorkerProcess {
  return utilityProcess.fork(modulePath, args, options) as PluginWorkerProcess
}

export function createPluginWorkerEnvironment(
  source: NodeJS.ProcessEnv = process.env
): Record<string, string> {
  const allowedKeys = [
    'SystemRoot',
    'WINDIR',
    'TEMP',
    'TMP',
    'TMPDIR',
    'LANG',
    'LC_ALL',
    'TZ'
  ] as const
  const environment: Record<string, string> = {}
  for (const key of allowedKeys) {
    const value = source[key]
    if (value) environment[key] = value
  }
  environment.OPENBOX_BACKEND_RUNTIME = 'utility-process-v2'
  return environment
}

function normalizeRpcValue(value: unknown): PluginBackendRpcJsonValue {
  if (value === undefined) return null
  inspectPluginRendererRpcPayload(value)
  return value as PluginBackendRpcJsonValue
}

export class PluginSandbox extends EventEmitter implements PluginSandboxRuntime {
  readonly runtimeKind = 'utility-process' as const

  private readonly backendApiVersion: 1 | 2
  private readonly context: PluginContext
  private readonly handler: (method: string, params: unknown) => unknown | Promise<unknown>
  private readonly mainEntry: string
  private readonly pluginDir: string
  private readonly pluginId: string
  private readonly requestTimeoutMs: number
  private readonly sessionToken = randomBytes(32).toString('base64url')
  private readonly spawnWorker: SpawnPluginWorker
  private readonly startTimeoutMs: number
  private readonly stopGraceMs: number

  private child: PluginWorkerProcess | null = null
  private childExitHandled = false
  private childExitPromise: Promise<void> | null = null
  private childExitResolve: (() => void) | null = null
  private expectedExit = false
  private pendingRequests = new Map<string, PendingRequest>()
  private startPromise: Promise<void> | null = null
  private state: SandboxState = 'idle'
  private stopPromise: Promise<void> | null = null
  private terminationPromise: Promise<void> | null = null

  constructor(options: SandboxOptions, dependencies: PluginSandboxDependencies = {}) {
    super()
    this.pluginId = options.pluginId
    this.mainEntry = options.mainEntry
    this.pluginDir = options.pluginDir
    this.backendApiVersion = options.backendApiVersion
    this.context = options.context
    this.handler = options.handler
    this.spawnWorker = dependencies.spawnWorker ?? defaultSpawnWorker
    this.requestTimeoutMs = dependencies.requestTimeoutMs ?? REQUEST_TIMEOUT
    this.startTimeoutMs = dependencies.startTimeoutMs ?? START_TIMEOUT
    this.stopGraceMs = dependencies.stopGraceMs ?? STOP_GRACE_TIMEOUT
  }

  get useProcessMode(): boolean {
    return true
  }

  get isRunning(): boolean {
    return this.state === 'running'
  }

  private isState(...states: SandboxState[]): boolean {
    return states.includes(this.state)
  }

  start(): Promise<void> {
    if (this.state === 'running') return Promise.resolve()
    if (this.startPromise) return this.startPromise
    if (this.state === 'stopping') return Promise.reject(new Error('Plugin is stopping'))

    const operation = (async () => {
      this.state = 'starting'
      try {
        await this.startWorker()
        if (this.state === 'starting') this.state = 'running'
      } catch (error) {
        if (!this.isState('stopping', 'stopped')) this.state = 'failed'
        throw error
      }
    })()
    this.startPromise = operation
    operation.then(
      () => {
        if (this.startPromise === operation) this.startPromise = null
      },
      () => {
        if (this.startPromise === operation) this.startPromise = null
      }
    )
    return operation
  }

  private sendChild(message: unknown, child: PluginWorkerProcess | null = this.child): void {
    try {
      child?.postMessage(message)
    } catch {
      // Exit handling rejects the corresponding request.
    }
  }

  private handleChildMessage(rawMessage: unknown): void {
    let message
    try {
      message = validatePluginBackendRpcEnvelope(rawMessage)
      if (message.token !== this.sessionToken) throw new Error('Backend session token mismatch')
    } catch (error) {
      const validationError =
        error instanceof Error ? error : new Error('Invalid backend RPC message')
      if (this.state === 'starting') this.rejectPending(validationError)
      this.emit('error', validationError)
      return
    }

    if (message.kind === 'response') {
      const pending = this.pendingRequests.get(message.requestId)
      if (!pending) return
      clearTimeout(pending.timer)
      this.pendingRequests.delete(message.requestId)
      if (message.ok) pending.resolve(message.result)
      else pending.reject(new Error(message.error.message))
      return
    }

    if (message.kind === 'fatal') {
      const error = new Error(message.error.message)
      this.rejectPending(error)
      this.emit('error', error)
      return
    }

    if (!isPluginBackendHostMethod(message.method)) {
      this.sendChild(
        createPluginBackendRpcErrorResponse(this.sessionToken, message.requestId, {
          code: 'NOT_ALLOWED',
          message: `Worker cannot invoke ${message.method}`
        })
      )
      return
    }

    Promise.resolve()
      .then(() => this.handler(message.method, message.params))
      .then((result) => {
        this.sendChild(
          createPluginBackendRpcResponse(
            this.sessionToken,
            message.requestId,
            normalizeRpcValue(result)
          )
        )
      })
      .catch((error) => {
        this.sendChild(
          createPluginBackendRpcErrorResponse(this.sessionToken, message.requestId, {
            code: 'INTERNAL_ERROR',
            message: error instanceof Error ? error.message : String(error)
          })
        )
      })
  }

  private async startWorker(): Promise<void> {
    const processEntry = join(__dirname, 'plugin-process.js')
    const child = this.spawnWorker(
      processEntry,
      [this.pluginDir, this.mainEntry, String(this.backendApiVersion)],
      {
        cwd: this.pluginDir,
        env: createPluginWorkerEnvironment(),
        stdio: 'pipe',
        serviceName: `openbox-plugin-${this.pluginId.slice(0, 48)}`
      }
    )
    this.attachChild(child)
    try {
      await this.requestWorker(
        'lifecycle.initialize',
        {
          pluginId: this.pluginId,
          config: this.context.config as Record<string, PluginBackendRpcJsonValue>
        },
        this.startTimeoutMs
      )
      if (this.child !== child || this.childExitHandled) {
        throw new Error('Plugin worker exited during startup')
      }
    } catch (error) {
      await this.terminateChild(child, false)
      throw error
    }
  }

  private attachChild(child: PluginWorkerProcess): void {
    this.child = child
    this.childExitHandled = false
    this.expectedExit = false
    this.childExitPromise = new Promise<void>((resolve) => {
      this.childExitResolve = resolve
    })
    child.on('message', (message) => this.handleChildMessage(message))
    child.once('exit', (code) => this.handleChildExit(child, code))
    child.stdout?.on('data', (chunk: Buffer | string) => {
      if (process.env.OPENBOX_SMOKE_TEST === '1') {
        console.log(`[smoke:plugin-worker:stdout] ${String(chunk).trimEnd()}`)
      }
    })
    child.stderr?.on('data', (chunk: Buffer | string) => {
      if (process.env.OPENBOX_SMOKE_TEST === '1') {
        console.error(`[smoke:plugin-worker:stderr] ${String(chunk).trimEnd()}`)
      }
    })
    child.on('error', (...details: unknown[]) => {
      const error = new Error(`Plugin utility process failure: ${details.map(String).join(' ')}`)
      this.rejectPending(error)
      this.emit('error', error)
    })
  }

  private handleChildExit(child: PluginWorkerProcess, code: number): void {
    if (this.childExitHandled) return
    this.childExitHandled = true
    const expected = this.expectedExit || this.state === 'stopping'
    if (this.child === child) this.child = null
    if (this.state === 'starting' || (!expected && this.state !== 'stopping')) this.state = 'failed'
    this.rejectPending(new Error(expected ? 'Plugin stopped' : 'Plugin utility process exited'))
    this.childExitResolve?.()
    this.childExitResolve = null
    this.emit('exit', code, { signal: null, expected } satisfies SandboxExitDetails)
  }

  private requestWorker<Method extends PluginBackendWorkerMethod>(
    method: Method,
    params: PluginBackendRpcRequest<Method>['params'],
    timeoutMs = this.requestTimeoutMs
  ): Promise<PluginBackendRpcJsonValue> {
    const child = this.child
    if (!child) return Promise.reject(new Error('Plugin utility process is not available'))
    if (this.pendingRequests.size >= 64) {
      return Promise.reject(new Error('Plugin backend has too many pending requests'))
    }
    return new Promise((resolve, reject) => {
      const requestId = randomUUID()
      const timer = setTimeout(() => {
        this.pendingRequests.delete(requestId)
        const error = new Error(`Plugin backend request timed out: ${method}`)
        reject(error)
        if (this.child === child && !this.isState('stopping', 'stopped')) {
          this.state = 'failed'
          this.emit('error', error)
          void this.terminateChild(child, false, false).catch((terminationError) => {
            this.emit('error', terminationError as Error)
          })
        }
      }, timeoutMs)
      this.pendingRequests.set(requestId, { resolve, reject, timer })
      this.sendChild(
        createPluginBackendRpcRequest(this.sessionToken, requestId, method, params),
        child
      )
    })
  }

  private async terminateChild(
    child: PluginWorkerProcess,
    graceful: boolean,
    expectedExit = true
  ): Promise<void> {
    if (this.terminationPromise) return this.terminationPromise
    const exitPromise = this.childExitPromise ?? Promise.resolve()
    const operation = (async () => {
      this.expectedExit = expectedExit
      if (graceful && this.child === child) {
        await this.requestWorker('lifecycle.dispose', {}, this.stopGraceMs).catch(() => undefined)
        if (await this.waitForExit(exitPromise, this.stopGraceMs)) return
      }
      if (this.child === child) child.kill()
      if (!(await this.waitForExit(exitPromise, Math.max(this.stopGraceMs, 5_000)))) {
        throw new Error('Plugin utility process did not exit after termination')
      }
    })()
    this.terminationPromise = operation
    try {
      await operation
    } finally {
      if (this.terminationPromise === operation) this.terminationPromise = null
    }
  }

  private waitForExit(exitPromise: Promise<void>, timeoutMs: number): Promise<boolean> {
    return new Promise((resolve) => {
      let settled = false
      const finish = (exited: boolean): void => {
        if (settled) return
        settled = true
        clearTimeout(timer)
        resolve(exited)
      }
      const timer = setTimeout(() => finish(false), timeoutMs)
      void exitPromise.then(() => finish(true))
    })
  }

  stop(): Promise<void> {
    if (this.stopPromise) return this.stopPromise
    if (this.state === 'stopped' || (this.state === 'idle' && !this.child)) {
      return Promise.resolve()
    }

    const operation = (async () => {
      this.state = 'stopping'
      const child = this.child
      if (child) await this.terminateChild(child, true)
      this.rejectPending(new Error('Plugin stopped'))
      this.state = 'stopped'
    })()
    this.stopPromise = operation
    operation.then(
      () => {
        if (this.stopPromise === operation) this.stopPromise = null
      },
      () => {
        if (this.stopPromise === operation) this.stopPromise = null
      }
    )
    return operation
  }

  private rejectPending(error: Error): void {
    for (const pending of this.pendingRequests.values()) {
      clearTimeout(pending.timer)
      pending.reject(error)
    }
    this.pendingRequests.clear()
  }

  pushEvent(event: string, data: unknown): void {
    if (this.child && this.state === 'running') {
      void this.requestWorker('host.event', {
        event,
        ...(data === undefined ? {} : { data: data as PluginBackendRpcJsonValue })
      }).catch((error) => this.emit('error', error))
    }
  }

  async sendMessage(message: PluginMessage): Promise<unknown> {
    if (!this.child || this.state !== 'running') return null
    return await this.requestWorker('plugin.message', {
      ...(message.payload === undefined
        ? {}
        : { message: message.payload as PluginBackendRpcJsonValue })
    })
  }
}

// The test tsconfig narrows its include set, so pull in the host's local sql.js declaration.
// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="../shared/types/sql.js.d.ts" />
/// <reference types="react" />

import { EventEmitter } from 'node:events'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PluginContext, PluginMessage, PluginMeta } from '../shared/types/plugin.types'
import { PluginLifecycleStatus } from '../shared/types/plugin.types'
import { Permission } from '../shared/types/permissions'
import type {
  PluginSandboxRuntime,
  PluginWorkerProcess,
  SandboxExitDetails,
  SandboxOptions
} from '../plugin-system/PluginSandbox'

const repositoryMocks = vi.hoisted(() => ({
  create: vi.fn(),
  delete: vi.fn(),
  findAll: vi.fn(),
  findById: vi.fn(),
  findByName: vi.fn(),
  getConfig: vi.fn(),
  getEnabledPlugins: vi.fn(),
  updateConfig: vi.fn(),
  updateEnabled: vi.fn(),
  updatePluginVersion: vi.fn()
}))

const databaseMocks = vi.hoisted(() => ({
  execute: vi.fn(),
  getDatabase: vi.fn(() => ({ namespace: 'database' })),
  queryAll: vi.fn(() => [])
}))

const storageMocks = vi.hoisted(() => ({
  delete: vi.fn(),
  ensureMigrated: vi.fn(),
  get: vi.fn((): unknown => null),
  list: vi.fn((): unknown[] => []),
  set: vi.fn()
}))

const electronMocks = vi.hoisted(() => ({
  registerShortcut: vi.fn(() => true),
  unregisterShortcut: vi.fn(),
  protocolHandle: vi.fn()
}))

vi.mock('@database/repositories/plugin.repository', () => ({
  PluginRepository: repositoryMocks
}))

vi.mock('@database/index', () => ({
  execute: databaseMocks.execute,
  getDatabase: databaseMocks.getDatabase,
  queryAll: databaseMocks.queryAll
}))

vi.mock('@database/pluginStorage', () => ({
  deletePluginStorageValue: storageMocks.delete,
  ensureLegacyPluginStorageMigrated: storageMocks.ensureMigrated,
  getPluginStorageValue: storageMocks.get,
  listPluginStorageValues: storageMocks.list,
  setPluginStorageValue: storageMocks.set
}))

vi.mock('electron', () => ({
  app: { getPath: vi.fn(() => '.') },
  BrowserWindow: {
    getAllWindows: vi.fn(() => []),
    getFocusedWindow: vi.fn(() => null)
  },
  dialog: {
    showOpenDialog: vi.fn(async () => ({ canceled: true, filePaths: [] }))
  },
  globalShortcut: {
    register: electronMocks.registerShortcut,
    unregister: electronMocks.unregisterShortcut
  },
  protocol: { handle: electronMocks.protocolHandle }
}))

import { PluginManager, type PluginManagerOptions } from '../plugin-system/PluginManager'
import { PluginSandbox } from '../plugin-system/PluginSandbox'

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')

function createContext(): PluginContext {
  return {
    id: 'plugin-a',
    config: {},
    logger: {
      debug: () => undefined,
      error: () => undefined,
      info: () => undefined,
      warn: () => undefined
    },
    database: {
      execute: async () => undefined,
      query: async () => []
    },
    storage: {
      batch: async () => undefined,
      delete: async () => undefined,
      get: async () => null,
      list: async () => [],
      set: async () => undefined
    },
    api: {
      emitEvent: () => undefined,
      fetch: async () => new Response(),
      notify: () => undefined,
      onEvent: () => () => undefined,
      openDialog: async () => null,
      readFile: async () => Buffer.alloc(0),
      registerShortcut: () => () => undefined,
      writeFile: async () => undefined,
      clipboard: {
        read: async () => ({ text: '' }),
        write: async () => ({ ok: true })
      },
      getSystemInfo: async () => ({
        os: { name: '', version: '', hostname: '' },
        cpu: { brand: '', cores: 0, physicalCores: 0, usage: 0 },
        memory: { total: 0, available: 0, usage: 0 },
        disks: [],
        network: []
      })
    }
  }
}

class FakeChild extends EventEmitter {
  readonly pid = 1234
  readonly stdout = null
  readonly stderr = null
  readonly sent: unknown[] = []
  readonly kill = vi.fn(() => true)
  readonly postMessage = vi.fn((message: unknown) => {
    this.sent.push(message)
  })

  asWorkerProcess(): PluginWorkerProcess {
    return this as unknown as PluginWorkerProcess
  }

  emitExit(code: number): void {
    this.emit('exit', code)
  }

  respond(ok: boolean, error = 'startup failed'): void {
    const request = this.sent.at(-1) as { token: string; requestId: string }
    this.emit('message', {
      v: 2,
      kind: 'response',
      token: request.token,
      requestId: request.requestId,
      ok,
      ...(ok ? { result: null } : { error: { code: 'INTERNAL_ERROR', message: error } })
    })
  }
}

function createProcessSandbox(
  child: FakeChild,
  timeouts: { requestTimeoutMs?: number; startTimeoutMs?: number; stopGraceMs?: number } = {}
): PluginSandbox {
  return new PluginSandbox(
    {
      pluginId: 'plugin-a',
      mainEntry: 'dist/main.js',
      pluginDir: repositoryRoot,
      backendApiVersion: 2,
      context: createContext(),
      handler: async () => null
    },
    {
      spawnWorker: () => child.asWorkerProcess(),
      requestTimeoutMs: timeouts.requestTimeoutMs,
      startTimeoutMs: timeouts.startTimeoutMs,
      stopGraceMs: timeouts.stopGraceMs
    }
  )
}

describe('PluginSandbox process lifecycle', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('kills a timed-out child and does not settle start until the child exits', async () => {
    vi.useFakeTimers()
    const child = new FakeChild()
    const sandbox = createProcessSandbox(child, { startTimeoutMs: 25 })
    sandbox.on('error', () => undefined)
    const starting = sandbox.start()
    let settled = false
    void starting.then(
      () => {
        settled = true
      },
      () => {
        settled = true
      }
    )

    await vi.advanceTimersByTimeAsync(25)
    expect(child.kill).toHaveBeenCalledWith()
    expect(settled).toBe(false)

    child.emitExit(1)
    await expect(starting).rejects.toBeInstanceOf(Error)
    expect(settled).toBe(true)
  })

  it('kills a child that reports startup failure and waits for exit', async () => {
    const child = new FakeChild()
    const sandbox = createProcessSandbox(child)
    const starting = sandbox.start()
    const observed = starting.catch((error: unknown) => error)

    child.respond(false)
    await Promise.resolve()
    await Promise.resolve()
    expect(child.kill).toHaveBeenCalledWith()

    child.emitExit(1)
    const error = await observed
    expect(error).toBeInstanceOf(Error)
    expect((error as Error).message).toBe('startup failed')
  })

  it('clears the startup timeout after the child acknowledges startup', async () => {
    vi.useFakeTimers()
    const child = new FakeChild()
    const sandbox = createProcessSandbox(child, { startTimeoutMs: 25 })
    const starting = sandbox.start()

    child.respond(true)
    await starting
    await vi.advanceTimersByTimeAsync(100)

    expect(sandbox.isRunning).toBe(true)
    expect(child.kill).not.toHaveBeenCalled()
  })

  it('marks a graceful deactivate exit as expected and waits for it', async () => {
    const child = new FakeChild()
    const sandbox = createProcessSandbox(child, { stopGraceMs: 25 })
    const exits: Array<{ code: number | null; details: SandboxExitDetails }> = []
    sandbox.on('exit', (code: number | null, details: SandboxExitDetails) => {
      exits.push({ code, details })
    })
    const starting = sandbox.start()
    child.respond(true)
    await starting

    const stopping = sandbox.stop()
    await Promise.resolve()
    expect(child.sent).toEqual(
      expect.arrayContaining([expect.objectContaining({ method: 'lifecycle.dispose' })])
    )
    child.respond(true)
    child.emitExit(0)
    await stopping

    expect(child.kill).not.toHaveBeenCalled()
    expect(exits).toEqual([{ code: 0, details: { expected: true, signal: null } }])
    expect(sandbox.isRunning).toBe(false)
  })

  it('kills an unresponsive running backend and reports its exit as unexpected', async () => {
    vi.useFakeTimers()
    const child = new FakeChild()
    const sandbox = createProcessSandbox(child, { requestTimeoutMs: 25 })
    const exits: SandboxExitDetails[] = []
    sandbox.on('error', () => undefined)
    sandbox.on('exit', (_code, details: SandboxExitDetails) => exits.push(details))
    const starting = sandbox.start()
    child.respond(true)
    await starting

    const request = sandbox.sendMessage({ type: 'message', payload: 'hang' })
    const observed = request.catch((error: unknown) => error)
    await vi.advanceTimersByTimeAsync(25)

    const error = await observed
    expect(error).toBeInstanceOf(Error)
    expect((error as Error).message).toContain('timed out')
    expect(child.kill).toHaveBeenCalledWith()
    child.emitExit(1)
    expect(exits).toEqual([{ expected: false, signal: null }])
  })
})

interface Deferred<T> {
  promise: Promise<T>
  reject: (error: Error) => void
  resolve: (value: T | PromiseLike<T>) => void
}

function deferred<T>(): Deferred<T> {
  let resolvePromise!: (value: T | PromiseLike<T>) => void
  let rejectPromise!: (error: Error) => void
  const promise = new Promise<T>((resolveValue, rejectValue) => {
    resolvePromise = resolveValue
    rejectPromise = rejectValue
  })
  return { promise, reject: rejectPromise, resolve: resolvePromise }
}

class FakeSandbox extends EventEmitter implements PluginSandboxRuntime {
  readonly useProcessMode = true
  readonly runtimeKind = 'utility-process' as const
  readonly options: SandboxOptions
  startCalls = 0
  stopCalls = 0
  private running = false
  private startGate = deferred<void>()
  private stopOperation: Promise<void> | null = null

  constructor(options: SandboxOptions) {
    super()
    this.options = options
  }

  get isRunning(): boolean {
    return this.running
  }

  async start(): Promise<void> {
    this.startCalls += 1
    await this.startGate.promise
    this.running = true
  }

  resolveStart(): void {
    this.startGate.resolve(undefined)
  }

  rejectStart(error: Error): void {
    this.startGate.reject(error)
  }

  stop(): Promise<void> {
    if (this.stopOperation) return this.stopOperation
    const operation = (async () => {
      this.stopCalls += 1
      this.startGate.reject(new Error('activation cancelled'))
      this.running = false
      this.emit('exit', 0, { expected: true, signal: null } satisfies SandboxExitDetails)
    })()
    this.stopOperation = operation
    return operation
  }

  crash(code: number | null = 1): void {
    this.running = false
    this.emit('exit', code, { expected: false, signal: null } satisfies SandboxExitDetails)
  }

  invoke(op: string, payload: unknown): Promise<unknown> {
    return Promise.resolve(this.options.handler(op, payload))
  }

  pushEvent(_event: string, _data: unknown): void {}

  async sendMessage(message: PluginMessage): Promise<unknown> {
    return message.payload
  }
}

function createPlugin(id: string, enabled = true): PluginMeta {
  return {
    id,
    name: 'plugin-system',
    version: '1.0.0',
    displayName: id,
    description: '',
    author: 'test',
    entryMain: 'dist/main.js',
    entryRenderer: 'dist/renderer.js',
    permissions: [Permission.Shortcut, Permission.StorageRead, Permission.StorageWrite],
    configSchema: {},
    configData: {},
    enabled,
    installedAt: '',
    updatedAt: ''
  }
}

describe('PluginManager lifecycle coordination', () => {
  let plugins: Map<string, PluginMeta>
  let instances: FakeSandbox[]
  let manager: PluginManager

  const createManager = (options: Partial<PluginManagerOptions> = {}): PluginManager =>
    new PluginManager({
      pluginsDir: repositoryRoot,
      registerProtocol: false,
      sandboxFactory: (sandboxOptions) => {
        const sandbox = new FakeSandbox(sandboxOptions)
        instances.push(sandbox)
        return sandbox
      },
      manifestReader: () => ({
        name: 'plugin-system',
        version: '1.0.0',
        displayName: 'test',
        description: '',
        author: 'test',
        main: 'dist/main.js',
        renderer: 'dist/renderer.js',
        backendApiVersion: 2,
        rendererApiVersion: 2,
        permissions: [Permission.Shortcut, Permission.StorageRead, Permission.StorageWrite],
        config: {}
      }),
      ...options
    })

  beforeEach(() => {
    vi.clearAllMocks()
    plugins = new Map([['plugin-a', createPlugin('plugin-a')]])
    repositoryMocks.findById.mockImplementation((id: string) => plugins.get(id) ?? null)
    repositoryMocks.findByName.mockImplementation(
      (name: string) => Array.from(plugins.values()).find((plugin) => plugin.name === name) ?? null
    )
    repositoryMocks.findAll.mockImplementation(() => Array.from(plugins.values()))
    repositoryMocks.getConfig.mockReturnValue({})
    repositoryMocks.getEnabledPlugins.mockImplementation(() =>
      Array.from(plugins.values()).filter((plugin) => plugin.enabled)
    )
    repositoryMocks.updateEnabled.mockImplementation((id: string, enabled: boolean) => {
      const plugin = plugins.get(id)
      if (plugin) plugin.enabled = enabled
    })
    instances = []
    manager = createManager()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('coalesces concurrent activation into one sandbox start', async () => {
    let reentrant: Promise<void> | undefined
    manager.onEvent('plugin:status', (data) => {
      if ((data as { status: PluginLifecycleStatus }).status === PluginLifecycleStatus.Activating) {
        reentrant = manager.activatePlugin('plugin-a')
      }
    })
    const first = manager.activatePlugin('plugin-a')
    const second = manager.activatePlugin('plugin-a')

    expect(first).toBe(second)
    expect(reentrant).toBe(first)
    expect(instances).toHaveLength(1)
    expect(instances[0].startCalls).toBe(1)

    instances[0].resolveStart()
    await Promise.all([first, second])

    expect(repositoryMocks.updateEnabled).toHaveBeenCalledTimes(1)
    expect(manager.getActivePlugins()).toEqual(['plugin-a'])
  })

  it('activates and stops renderer-only plugins without creating a backend sandbox', async () => {
    const statuses: PluginLifecycleStatus[] = []
    manager = createManager({
      manifestReader: () => ({
        name: 'plugin-system',
        version: '1.0.0',
        displayName: 'test',
        description: '',
        author: 'test',
        main: 'dist/main.js',
        renderer: 'dist/renderer.js',
        backend: false,
        manifestVersion: 2,
        rendererApiVersion: 2,
        permissions: [],
        config: {}
      })
    })
    manager.onEvent('plugin:status', (data) => {
      statuses.push((data as { status: PluginLifecycleStatus }).status)
    })

    await manager.activatePlugin('plugin-a')

    expect(instances).toHaveLength(0)
    expect(manager.getActivePlugins()).toEqual(['plugin-a'])
    expect(statuses).toEqual([PluginLifecycleStatus.Activating, PluginLifecycleStatus.Active])
    await expect(manager.sendMessage('plugin-a', { type: 'ping' })).rejects.toThrow('renderer-only')

    await manager.deactivatePlugin('plugin-a')

    expect(manager.getActivePlugins()).toEqual([])
    expect(statuses.slice(-2)).toEqual([
      PluginLifecycleStatus.Deactivating,
      PluginLifecycleStatus.Inactive
    ])
    expect(repositoryMocks.updateEnabled).toHaveBeenLastCalledWith('plugin-a', false)
  })

  it('does not leave a renderer-only runtime active when stop races with activation', async () => {
    manager = createManager({
      manifestReader: () => ({
        name: 'plugin-system',
        version: '1.0.0',
        displayName: 'test',
        description: '',
        author: 'test',
        main: 'dist/main.js',
        renderer: 'dist/renderer.js',
        backend: false,
        manifestVersion: 2,
        rendererApiVersion: 2,
        permissions: [],
        config: {}
      })
    })
    let deactivation: Promise<void> | undefined
    manager.onEvent('plugin:status', (data) => {
      if ((data as { status: PluginLifecycleStatus }).status === PluginLifecycleStatus.Activating) {
        deactivation = manager.deactivatePlugin('plugin-a')
      }
    })

    await manager.activatePlugin('plugin-a')
    await deactivation

    expect(instances).toHaveLength(0)
    expect(manager.getActivePlugins()).toEqual([])
    expect(repositoryMocks.updateEnabled).toHaveBeenLastCalledWith('plugin-a', false)
  })

  it('removes a crashed sandbox, releases resources, and permits explicit restart', async () => {
    const statuses: PluginLifecycleStatus[] = []
    manager.onEvent('plugin:status', (data) => {
      statuses.push((data as { status: PluginLifecycleStatus }).status)
    })
    const firstActivation = manager.activatePlugin('plugin-a')
    instances[0].resolveStart()
    await firstActivation
    await instances[0].invoke('shortcut.register', { keys: 'Ctrl+Shift+P' })

    instances[0].crash(0)

    expect(manager.getActivePlugins()).toEqual([])
    expect(electronMocks.unregisterShortcut).toHaveBeenCalledWith('Ctrl+Shift+P')
    expect(statuses.at(-1)).toBe(PluginLifecycleStatus.Error)

    const restart = manager.activatePlugin('plugin-a')
    expect(instances).toHaveLength(2)
    instances[1].resolveStart()
    await restart
    expect(manager.getActivePlugins()).toEqual(['plugin-a'])
  })

  it('releases the global shortcut when the plugin explicitly unregisters it', async () => {
    const activation = manager.activatePlugin('plugin-a')
    instances[0].resolveStart()
    await activation

    await instances[0].invoke('shortcut.register', { keys: 'Ctrl+Shift+P' })
    expect(electronMocks.registerShortcut).toHaveBeenCalledWith(
      'Ctrl+Shift+P',
      expect.any(Function)
    )

    await instances[0].invoke('shortcut.unregister', { keys: 'Ctrl+Shift+P' })
    expect(electronMocks.unregisterShortcut).toHaveBeenCalledWith('Ctrl+Shift+P')

    // 注销后 stop 不应再次触发清理（cleanup 已消费）
    const stops = electronMocks.unregisterShortcut.mock.calls.length
    await manager.deactivatePlugin('plugin-a')
    expect(electronMocks.unregisterShortcut.mock.calls.length).toBe(stops)
  })

  it('unsubscribes event handlers when the plugin explicitly unsubscribes', async () => {
    const activation = manager.activatePlugin('plugin-a')
    instances[0].resolveStart()
    await activation

    const pushEvent = vi.spyOn(instances[0], 'pushEvent').mockImplementation(() => undefined)
    const runtimeApi = instances[0].options.context.api

    await instances[0].invoke('event.subscribe', { subscriptionId: '1', event: 'foo' })
    runtimeApi.emitEvent('foo', { n: 1 })
    await Promise.resolve()
    expect(pushEvent).toHaveBeenCalledWith('foo', { n: 1 })

    pushEvent.mockClear()
    await instances[0].invoke('event.unsubscribe', { subscriptionId: '1' })
    runtimeApi.emitEvent('foo', { n: 2 })
    await Promise.resolve()
    expect(pushEvent).not.toHaveBeenCalled()
  })

  it('namespaces storage operations by the active plugin id', async () => {
    storageMocks.get.mockReturnValue({ title: 'entry' })
    storageMocks.list.mockReturnValue([{ key: 'entry:1', value: { title: 'entry' } }])
    const activation = manager.activatePlugin('plugin-a')
    instances[0].resolveStart()
    await activation

    await expect(instances[0].invoke('storage.get', { key: 'entry:1' })).resolves.toEqual({
      title: 'entry'
    })
    await instances[0].invoke('storage.set', { key: 'entry:1', value: { title: 'updated' } })
    await instances[0].invoke('storage.delete', { key: 'entry:1' })
    await expect(instances[0].invoke('storage.list', { prefix: 'entry:' })).resolves.toEqual([
      { key: 'entry:1', value: { title: 'entry' } }
    ])

    expect(storageMocks.get).toHaveBeenCalledWith({ namespace: 'database' }, 'plugin-a', 'entry:1')
    expect(storageMocks.set).toHaveBeenCalledWith(
      { namespace: 'database' },
      'plugin-a',
      'entry:1',
      {
        title: 'updated'
      }
    )
    expect(storageMocks.delete).toHaveBeenCalledWith(
      { namespace: 'database' },
      'plugin-a',
      'entry:1'
    )
    expect(storageMocks.list).toHaveBeenCalledWith({ namespace: 'database' }, 'plugin-a', 'entry:')
  })

  it('emits deactivating then inactive without treating normal stop as a crash', async () => {
    const statuses: PluginLifecycleStatus[] = []
    manager.onEvent('plugin:status', (data) => {
      statuses.push((data as { status: PluginLifecycleStatus }).status)
    })
    const activation = manager.activatePlugin('plugin-a')
    instances[0].resolveStart()
    await activation

    await manager.deactivatePlugin('plugin-a')

    expect(statuses.slice(-2)).toEqual([
      PluginLifecycleStatus.Deactivating,
      PluginLifecycleStatus.Inactive
    ])
    expect(statuses).not.toContain(PluginLifecycleStatus.Error)
    expect(instances[0].stopCalls).toBe(1)
    expect(repositoryMocks.updateEnabled).toHaveBeenLastCalledWith('plugin-a', false)
  })

  it('orders an explicit restart after an in-flight deactivate completes', async () => {
    const statuses: PluginLifecycleStatus[] = []
    manager.onEvent('plugin:status', (data) => {
      statuses.push((data as { status: PluginLifecycleStatus }).status)
    })
    const activation = manager.activatePlugin('plugin-a')
    instances[0].resolveStart()
    await activation

    const deactivation = manager.deactivatePlugin('plugin-a')
    const restart = manager.activatePlugin('plugin-a')
    await deactivation

    expect(instances).toHaveLength(2)
    instances[1].resolveStart()
    await restart
    expect(statuses.slice(-4)).toEqual([
      PluginLifecycleStatus.Deactivating,
      PluginLifecycleStatus.Inactive,
      PluginLifecycleStatus.Activating,
      PluginLifecycleStatus.Active
    ])
    expect(repositoryMocks.updateEnabled).toHaveBeenLastCalledWith('plugin-a', true)
  })

  it('stops and removes a sandbox whose activation fails', async () => {
    const statuses: PluginLifecycleStatus[] = []
    manager.onEvent('plugin:status', (data) => {
      statuses.push((data as { status: PluginLifecycleStatus }).status)
    })
    const activation = manager.activatePlugin('plugin-a')

    instances[0].rejectStart(new Error('boom'))
    await expect(activation).rejects.toThrow('boom')

    expect(instances[0].stopCalls).toBe(1)
    expect(manager.getActivePlugins()).toEqual([])
    expect(statuses.at(-1)).toBe(PluginLifecycleStatus.Error)
  })

  it('restores enabled plugins with bounded startup concurrency', async () => {
    plugins.set('plugin-b', createPlugin('plugin-b'))
    plugins.set('plugin-c', createPlugin('plugin-c'))

    const activation = manager.activateAllEnabled()

    expect(instances).toHaveLength(2)
    expect(instances.map((sandbox) => sandbox.startCalls)).toEqual([1, 1])
    instances[0].resolveStart()
    await vi.waitFor(() => expect(instances).toHaveLength(3))
    instances.slice(1).forEach((sandbox) => sandbox.resolveStart())
    await activation

    expect(manager.getActivePlugins().sort()).toEqual(['plugin-a', 'plugin-b', 'plugin-c'])
  })

  it('backs off crash recovery and quarantines a repeated crash loop', async () => {
    vi.useFakeTimers()
    manager = createManager({
      crashPolicy: {
        baseDelayMs: 10,
        backoffFactor: 2,
        quarantineThreshold: 3,
        windowMs: 1_000
      }
    })
    const firstActivation = manager.activatePlugin('plugin-a')
    instances[0].resolveStart()
    await firstActivation

    instances[0].crash()
    await vi.advanceTimersByTimeAsync(9)
    expect(instances).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(instances).toHaveLength(2)
    instances[1].resolveStart()
    await vi.advanceTimersByTimeAsync(0)
    expect(manager.getActivePlugins()).toEqual(['plugin-a'])

    instances[1].crash()
    await vi.advanceTimersByTimeAsync(19)
    expect(instances).toHaveLength(2)
    await vi.advanceTimersByTimeAsync(1)
    expect(instances).toHaveLength(3)
    instances[2].resolveStart()
    await vi.advanceTimersByTimeAsync(0)

    instances[2].crash()
    await vi.advanceTimersByTimeAsync(100)

    expect(instances).toHaveLength(3)
    expect(repositoryMocks.updateEnabled).toHaveBeenLastCalledWith('plugin-a', false)
    expect(plugins.get('plugin-a')?.enabled).toBe(false)
  })
})

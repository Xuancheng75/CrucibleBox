import { existsSync, mkdirSync } from 'fs'
import { join } from 'path'
import { app, BrowserWindow, dialog, globalShortcut } from 'electron'

import { execute as dbExecute, getDatabase, queryAll as dbQueryAll } from '@database/index'
import {
  applyPluginStorageBatch,
  deletePluginStorageValue,
  ensureLegacyPluginStorageMigrated,
  getPluginStorageValue,
  listPluginStorageValues,
  setPluginStorageValue
} from '@database/pluginStorage'
import { PluginRepository } from '@database/repositories/plugin.repository'
import type {
  PluginConfig,
  PluginContext,
  PluginDatabaseAPI,
  PluginHostAPI,
  PluginLogEntry,
  PluginLogFilter,
  PluginLogger,
  PluginManifest,
  PluginMessage,
  PluginMeta,
  PluginStorageAPI
} from '@shared/types/plugin.types'
import { PluginLifecycleStatus } from '@shared/types/plugin.types'
import type { ToolboxTheme } from '@shared/types/theme.types'

import { EventBus } from './EventBus'
import { PermissionGuard } from './PermissionGuard'
import { PluginCrashPolicy, type PluginCrashPolicyOptions } from './PluginCrashPolicy'
import { PluginInstallationService } from './PluginInstallationService'
import type { PluginInstallPreview, PluginInstallSource } from './PluginInstallPreparation'
import { readPluginManifest } from './PluginManifestPolicy'
import { PluginProtocol } from './PluginProtocol'
import {
  PluginSandbox,
  type PluginSandboxFactory,
  type PluginSandboxRuntime,
  type SandboxExitDetails
} from './PluginSandbox'
import { Permission } from '@shared/types/permissions'
import { TrustedServiceRuntime } from './TrustedServiceRuntime'
import {
  createPluginRuntimeRecord,
  hasPluginRuntime,
  type PluginRuntimeRecord
} from './runtime/PluginRuntimeRecord'
import { PluginLogService } from './runtime/PluginLogService'

export type { PluginInstallPreview } from './PluginInstallPreparation'

export interface PluginManagerOptions {
  allowLegacyPluginInstall?: boolean
  crashPolicy?: PluginCrashPolicyOptions
  pluginsDir?: string
  registerProtocol?: boolean
  sandboxFactory?: PluginSandboxFactory
  startupActivationConcurrency?: number
  manifestReader?: (pluginDirectory: string) => PluginManifest
}

interface PluginRuntimeHostAPI extends PluginHostAPI {
  invokeTrustedService(service: string, operation: string, payload?: unknown): Promise<unknown>
}

/** 子进程请求分发表上下文：把 handler 依赖的实例状态收敛到单个入参 */
interface ChildRequestContext {
  pluginId: string
  runtimeDb: PluginDatabaseAPI
  runtimeStorage: PluginStorageAPI
  runtimeApi: PluginRuntimeHostAPI
  sandbox: PluginSandboxRuntime
  cleanups: Map<string, () => void>
  log(level: string, message: string, args: unknown[]): void
}

type ChildRequestHandler = (
  ctx: ChildRequestContext,
  payload: Record<string, unknown>
) => Promise<unknown>

/** capability 分发表：与原 switch 逐分支行为一致 */
const childRequestHandlers = new Map<string, ChildRequestHandler>([
  ['db.query', async (ctx, p) => ctx.runtimeDb.query(String(p.sql), p.params as unknown[])],
  [
    'db.execute',
    async (ctx, p) => {
      await ctx.runtimeDb.execute(String(p.sql), p.params as unknown[])
      return null
    }
  ],
  ['storage.get', async (ctx, p) => ctx.runtimeStorage.get(String(p.key))],
  [
    'storage.set',
    async (ctx, p) => {
      await ctx.runtimeStorage.set(String(p.key), p.value)
      return null
    }
  ],
  [
    'storage.delete',
    async (ctx, p) => {
      await ctx.runtimeStorage.delete(String(p.key))
      return null
    }
  ],
  [
    'storage.list',
    async (ctx, p) => ctx.runtimeStorage.list(p.prefix === undefined ? undefined : String(p.prefix))
  ],
  [
    'storage.batch',
    async (ctx, p) => {
      await ctx.runtimeStorage.batch(p.mutations as Parameters<PluginStorageAPI['batch']>[0])
      return null
    }
  ],
  [
    'log.write',
    async (ctx, p) => {
      ctx.log(String(p.level), String(p.message ?? ''), p.args as unknown[])
      return null
    }
  ],
  [
    'notification.show',
    async (ctx, p) => {
      ctx.runtimeApi.notify(String(p.title ?? ''), p.body == null ? undefined : String(p.body))
      return null
    }
  ],
  [
    'dialog.open',
    async (ctx, p) => ctx.runtimeApi.openDialog(p.type === 'folder' ? 'folder' : 'file')
  ],
  [
    'network.fetch',
    async (ctx, p) => {
      const res = await ctx.runtimeApi.fetch(String(p.url), p.options as RequestInit)
      const headers: Record<string, string> = {}
      res.headers.forEach((value, key) => {
        headers[key] = value
      })
      const body = await res.text()
      return { ok: res.ok, status: res.status, statusText: res.statusText, headers, body }
    }
  ],
  [
    'file.read',
    async (ctx, p) => {
      const data = await ctx.runtimeApi.readFile(String(p.path))
      return { base64: Buffer.from(data).toString('base64') }
    }
  ],
  [
    'file.write',
    async (ctx, p) => {
      await ctx.runtimeApi.writeFile(String(p.path), Buffer.from(String(p.base64), 'base64'))
      return null
    }
  ],
  [
    'shortcut.register',
    async (ctx, p) => {
      const keys = String(p.keys)
      const cleanup = ctx.runtimeApi.registerShortcut(keys, () => {
        ctx.sandbox.pushEvent('openbox:shortcut', keys)
      })
      ctx.cleanups.set(`shortcut:${keys}`, cleanup)
      return null
    }
  ],
  [
    'shortcut.unregister',
    async (ctx, p) => {
      const key = `shortcut:${String(p.keys)}`
      const cleanup = ctx.cleanups.get(key)
      if (cleanup) {
        ctx.cleanups.delete(key)
        try {
          cleanup()
        } catch {
          // ignore
        }
      }
      return null
    }
  ],
  [
    'event.emit',
    async (ctx, p) => {
      ctx.runtimeApi.emitEvent(String(p.event), p.data)
      return null
    }
  ],
  [
    'event.subscribe',
    async (ctx, p) => {
      const subId = String(p.subscriptionId ?? '')
      const event = String(p.event)
      const cleanup = ctx.runtimeApi.onEvent(event, (data) => {
        ctx.sandbox.pushEvent(event, data)
      })
      ctx.cleanups.set(`sub:${subId}`, cleanup)
      return null
    }
  ],
  [
    'event.unsubscribe',
    async (ctx, p) => {
      const key = `sub:${String(p.subscriptionId ?? '')}`
      const cleanup = ctx.cleanups.get(key)
      if (cleanup) {
        ctx.cleanups.delete(key)
        try {
          cleanup()
        } catch {
          // ignore
        }
      }
      return null
    }
  ],
  [
    'trusted.invoke',
    async (ctx, p) =>
      ctx.runtimeApi.invokeTrustedService(String(p.service), String(p.operation), p.payload)
  ]
])

export class PluginManager {
  /** 每插件单一运行时记录（替代原 9 张 Map/Set/WeakSet） */
  private runtimes = new Map<string, PluginRuntimeRecord>()
  private eventBus: EventBus = new EventBus()
  private crashPolicy: PluginCrashPolicy
  private logService: PluginLogService
  private sandboxFactory: PluginSandboxFactory
  private shuttingDown = false
  private startupActivationConcurrency: number
  private manifestReader: (pluginDirectory: string) => PluginManifest
  private installationService: PluginInstallationService
  private pluginsDir: string

  constructor(options: PluginManagerOptions = {}) {
    this.pluginsDir = options.pluginsDir ?? join(app.getPath('userData'), 'plugins')
    this.sandboxFactory =
      options.sandboxFactory ?? ((sandboxOptions) => new PluginSandbox(sandboxOptions))
    this.crashPolicy = new PluginCrashPolicy(options.crashPolicy)
    const requestedConcurrency = options.startupActivationConcurrency ?? 2
    this.startupActivationConcurrency =
      Number.isSafeInteger(requestedConcurrency) && requestedConcurrency > 0
        ? Math.min(8, requestedConcurrency)
        : 2
    this.manifestReader = options.manifestReader ?? readPluginManifest
    this.logService = new PluginLogService({
      emitLog: (entry) => this.eventBus.emit('plugin:log', entry)
    })
    if (!existsSync(this.pluginsDir)) {
      mkdirSync(this.pluginsDir, { recursive: true })
    }
    this.installationService = new PluginInstallationService({
      pluginsDir: this.pluginsDir,
      allowLegacyFullTrust: options.allowLegacyPluginInstall,
      runtime: {
        hasRuntime: (id) => this.hasRuntime(id),
        stopRuntime: (id) => this.stopPlugin(id),
        activateRuntime: (id) => this.activatePluginRuntime(id),
        getPendingDeactivation: (id) => this.runtime(id).deactivationPromise ?? undefined,
        acquireMaintenance: (id) => this.acquireMaintenance(id)
      }
    })
    if (options.registerProtocol !== false) PluginProtocol.register(this.pluginsDir)
  }

  private runtime(id: string): PluginRuntimeRecord {
    let record = this.runtimes.get(id)
    if (!record) {
      record = createPluginRuntimeRecord(id)
      this.runtimes.set(id, record)
    }
    return record
  }

  get pluginsPath(): string {
    return this.pluginsDir
  }

  async installFromZip(zipPath: string): Promise<PluginMeta> {
    return await this.installationService.installFromZip(zipPath)
  }

  previewInstall(source: PluginInstallSource): PluginInstallPreview {
    return this.installationService.previewInstall(source)
  }

  commitPreparedInstall(installToken: string): Promise<PluginMeta> {
    return this.installationService.commitPreparedInstall(installToken)
  }

  discardPreparedInstall(installToken: string): void {
    this.installationService.discardPreparedInstall(installToken)
  }

  installFromDirectory(dirPath: string): Promise<PluginMeta> {
    return this.installationService.installFromDirectory(dirPath)
  }

  async uninstall(id: string): Promise<void> {
    this.clearCrashRecovery(id, true)
    await this.installationService.uninstall(id)
  }

  activatePlugin(id: string): Promise<void> {
    this.clearCrashRecovery(id, true)
    if (this.runtime(id).maintenance) {
      return Promise.reject(new Error(`Plugin ${id} is currently being upgraded`))
    }
    return this.activatePluginRuntime(id)
  }

  private activatePluginRuntime(id: string): Promise<void> {
    const record = this.runtime(id)
    if (record.quarantine) {
      return Promise.reject(new Error(`Plugin ${id} is quarantined after repeated crashes`))
    }
    if (record.stopPromise) return record.stopPromise.then(() => this.activatePluginRuntime(id))

    if (record.activationPromise) return record.activationPromise
    if (record.rendererOnly) return Promise.resolve()
    if (record.sandbox?.isRunning) return Promise.resolve()

    return this.startTrackedOperation(record, 'activationPromise', () => this.performActivation(id))
  }

  private async performActivation(id: string): Promise<void> {
    const plugin = PluginRepository.findById(id)
    if (!plugin) {
      throw new Error(`插件 ${id} 未找到`)
    }
    this.installationService.assertPluginCanRun(plugin.name)

    const pluginDir = join(this.pluginsDir, plugin.name)
    if (!existsSync(pluginDir)) {
      throw new Error(`插件目录不存在: ${pluginDir}`)
    }
    const manifest = this.manifestReader(pluginDir)
    if (manifest.name !== plugin.name || manifest.main !== plugin.entryMain) {
      throw new Error(`Plugin metadata does not match its installed manifest: ${plugin.name}`)
    }

    const record = this.runtime(id)
    if (manifest.backend === false) {
      this.emitStatus(plugin.id, PluginLifecycleStatus.Activating)
      PluginRepository.updateEnabled(id, true)
      record.rendererOnly = true
      console.log(`Plugin activated: ${plugin.name} (renderer-only)`)
      this.emitStatus(plugin.id, PluginLifecycleStatus.Active)
      return
    }

    const config = PluginRepository.getConfig(id)
    const permissions = PermissionGuard.parsePermissions(plugin.permissions as unknown as string[])
    ensureLegacyPluginStorageMigrated(getDatabase(), plugin.id, plugin.name)

    const logger: PluginLogger = {
      info: (msg, ...args) => this.logService.log(plugin.id, 'info', msg, args),
      warn: (msg, ...args) => this.logService.log(plugin.id, 'warn', msg, args),
      error: (msg, ...args) => this.logService.log(plugin.id, 'error', msg, args),
      debug: (msg, ...args) => this.logService.log(plugin.id, 'debug', msg, args)
    }
    const trustedServiceRuntime = new TrustedServiceRuntime({
      pluginId: plugin.id,
      pluginDirectory: pluginDir,
      manifest,
      config,
      logger
    })

    const runtimeDb: PluginDatabaseAPI = {
      query: async (sql, params) => {
        new PermissionGuard(permissions).assert(Permission.DatabaseRead)
        return dbQueryAll(sql, params)
      },
      execute: async (sql, params) => {
        new PermissionGuard(permissions).assert(Permission.DatabaseWrite)
        dbExecute(sql, params)
      }
    }

    const runtimeStorage: PluginStorageAPI = {
      get: async (key) => {
        new PermissionGuard(permissions).assert(Permission.StorageRead)
        return getPluginStorageValue(getDatabase(), plugin.id, key)
      },
      set: async (key, value) => {
        new PermissionGuard(permissions).assert(Permission.StorageWrite)
        setPluginStorageValue(getDatabase(), plugin.id, key, value)
      },
      delete: async (key) => {
        new PermissionGuard(permissions).assert(Permission.StorageWrite)
        deletePluginStorageValue(getDatabase(), plugin.id, key)
      },
      list: async (prefix) => {
        new PermissionGuard(permissions).assert(Permission.StorageRead)
        return listPluginStorageValues(getDatabase(), plugin.id, prefix)
      },
      batch: async (mutations) => {
        new PermissionGuard(permissions).assert(Permission.StorageWrite)
        applyPluginStorageBatch(getDatabase(), plugin.id, mutations)
      }
    }

    const runtimeApi: PluginRuntimeHostAPI = {
      notify: (title, body) => {
        new PermissionGuard(permissions).assert(Permission.Notification)
        this.eventBus.emit('api:notify', { pluginId: plugin.id, title, body })
      },
      openDialog: async (type) => {
        new PermissionGuard(permissions).assert(Permission.Dialog)
        const parent = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0]
        const result = await dialog.showOpenDialog(parent, {
          properties: type === 'folder' ? ['openDirectory'] : ['openFile']
        })
        return result.canceled ? null : (result.filePaths[0] ?? null)
      },
      fetch: async (url, opts) => {
        new PermissionGuard(permissions).assert(Permission.NetworkFetch)
        const MAX_FETCH_BYTES = 50 * 1024 * 1024 // 50 MB
        const controller = new AbortController()
        const timeout = setTimeout(() => controller.abort(), 30000)
        try {
          const res = await fetch(url, { ...opts, signal: controller.signal })
          const total = Number(res.headers.get('content-length') ?? 0)
          if (total > MAX_FETCH_BYTES) {
            throw new Error('响应体积过大')
          }
          const buffer = await res.arrayBuffer()
          if (buffer.byteLength > MAX_FETCH_BYTES) {
            throw new Error('响应体积过大')
          }
          return new Response(buffer, {
            status: res.status,
            statusText: res.statusText,
            headers: res.headers
          })
        } finally {
          clearTimeout(timeout)
        }
      },
      readFile: async (path) => {
        new PermissionGuard(permissions).assert(Permission.FileRead)
        const { readFile } = await import('fs/promises')
        return readFile(path) as unknown as Buffer
      },
      writeFile: async (path, data) => {
        new PermissionGuard(permissions).assert(Permission.FileWrite)
        const { writeFile } = await import('fs/promises')
        await writeFile(path, data)
      },
      registerShortcut: (keys, handler) => {
        const guard = new PermissionGuard(permissions)
        guard.assert(Permission.Shortcut)
        globalShortcut.register(keys, handler)
        return () => {
          globalShortcut.unregister(keys)
        }
      },
      emitEvent: (event, data) => this.eventBus.emit(`plugin:${plugin.id}:${event}`, data),
      onEvent: (event, handler) => this.eventBus.on(`plugin:${plugin.id}:${event}`, handler),
      invokeTrustedService: (service, operation, payload) =>
        trustedServiceRuntime.invoke(service, operation, payload)
    }

    const context: PluginContext = {
      id: plugin.id,
      config,
      logger,
      database: runtimeDb,
      storage: runtimeStorage,
      api: runtimeApi
    }

    const sandbox: PluginSandboxRuntime = this.sandboxFactory({
      pluginId: plugin.id,
      mainEntry: plugin.entryMain,
      pluginDir,
      backendApiVersion: manifest.backendApiVersion ?? 1,
      context,
      handler: (op, payload) =>
        this.handleChildRequest(
          {
            pluginId: plugin.id,
            runtimeDb,
            runtimeStorage,
            runtimeApi,
            sandbox,
            cleanups: record.cleanups,
            log: (level, message, args) => this.logService.log(plugin.id, level, message, args)
          },
          op,
          payload
        )
    })
    record.cleanups.set('trusted-services', () => {
      void trustedServiceRuntime.dispose().catch((error) => {
        logger.error(`Trusted service cleanup failed: ${(error as Error).message}`)
      })
    })

    sandbox.on('message', (msg: PluginMessage) => {
      this.eventBus.emit(`plugin:message:${plugin.id}`, msg)
      this.eventBus.emit('plugin:message', { pluginId: plugin.id, message: msg })
    })

    sandbox.on('error', (err: Error) => {
      if (!record.expectedStopSandboxes.has(sandbox)) {
        this.reportSandboxError(plugin.id, logger, sandbox, err)
      }
    })

    sandbox.on('exit', (code: number | null, details?: SandboxExitDetails) => {
      const expected = details?.expected ?? record.expectedStopSandboxes.has(sandbox)
      if (record.sandbox === sandbox) record.sandbox = null
      this.runPluginCleanups(record)
      if (!expected) {
        this.reportSandboxError(
          plugin.id,
          logger,
          sandbox,
          new Error(`插件进程意外退出 (code=${code}, signal=${details?.signal ?? 'none'})`)
        )
        this.scheduleCrashRecovery(plugin, logger)
      }
    })

    record.sandbox = sandbox
    this.emitStatus(plugin.id, PluginLifecycleStatus.Activating)
    try {
      await sandbox.start()
      if (record.sandbox !== sandbox || !sandbox.isRunning) {
        throw new Error('插件启动完成前已停止')
      }
    } catch (error) {
      if (record.sandbox === sandbox) record.sandbox = null
      this.runPluginCleanups(record)
      await sandbox.stop().catch(() => undefined)
      if (!record.expectedStopSandboxes.has(sandbox)) {
        this.reportSandboxError(plugin.id, logger, sandbox, error as Error)
      }
      throw error
    }

    PluginRepository.updateEnabled(id, true)
    console.log(`插件已激活: ${plugin.name} (${sandbox.runtimeKind})`)
    this.emitStatus(plugin.id, PluginLifecycleStatus.Active)
  }

  private reportSandboxError(
    pluginId: string,
    logger: PluginLogger,
    sandbox: PluginSandboxRuntime,
    error: Error
  ): void {
    const record = this.runtime(pluginId)
    if (record.reportedErrorSandboxes.has(sandbox)) return
    record.reportedErrorSandboxes.add(sandbox)
    logger.error(`插件运行错误: ${error.message}`)
    this.emitStatus(pluginId, PluginLifecycleStatus.Error)
    this.eventBus.emit('plugin:error', { pluginId, error: error.message })
  }

  private clearCrashRecovery(pluginId: string, resetPolicy: boolean): void {
    const record = this.runtime(pluginId)
    if (record.restartTimer) clearTimeout(record.restartTimer)
    record.restartTimer = null
    if (resetPolicy) {
      this.crashPolicy.reset(pluginId)
      record.quarantine = false
    }
  }

  private scheduleCrashRecovery(plugin: PluginMeta, logger: PluginLogger): void {
    const record = this.runtime(plugin.id)
    if (this.shuttingDown || record.restartTimer) return
    const decision = this.crashPolicy.record(plugin.id)
    if (decision.action === 'quarantine') {
      record.quarantine = true
      try {
        PluginRepository.updateEnabled(plugin.id, false)
      } catch (error) {
        logger.error(`插件隔离状态持久化失败: ${(error as Error).message}`)
      }
      logger.error(`插件在恢复窗口内崩溃 ${decision.crashesInWindow} 次，已隔离；请手动重新启用`)
      this.eventBus.emit('plugin:quarantined', {
        pluginId: plugin.id,
        crashes: decision.crashesInWindow
      })
      return
    }

    logger.warn(
      `插件异常退出，将在 ${decision.delayMs}ms 后尝试第 ${decision.crashesInWindow} 次恢复`
    )
    const timer = setTimeout(() => {
      record.restartTimer = null
      const current = PluginRepository.findById(plugin.id)
      if (
        this.shuttingDown ||
        !current?.enabled ||
        record.maintenance ||
        record.deactivationPromise
      ) {
        return
      }
      void this.activatePluginRuntime(plugin.id).catch((error) => {
        console.error(`Failed to recover plugin ${plugin.name}:`, error)
      })
    }, decision.delayMs)
    timer.unref?.()
    record.restartTimer = timer
  }

  private async handleChildRequest(
    ctx: ChildRequestContext,
    op: string,
    payload: unknown
  ): Promise<unknown> {
    const handler = childRequestHandlers.get(op)
    if (!handler) throw new Error(`未知子进程请求: ${op}`)
    return handler(ctx, (payload ?? {}) as Record<string, unknown>)
  }

  private runPluginCleanups(record: PluginRuntimeRecord): void {
    for (const [suffix, cleanup] of record.cleanups) {
      record.cleanups.delete(suffix)
      try {
        cleanup()
      } catch {
        // ignore
      }
    }
  }

  private withRollbackErrors(primary: unknown, rollbackErrors: unknown[]): Error {
    const primaryError = primary instanceof Error ? primary : new Error(String(primary))
    if (rollbackErrors.length === 0) return primaryError
    return new AggregateError(
      [primaryError, ...rollbackErrors],
      `${primaryError.message}; rollback encountered ${rollbackErrors.length} additional error(s)`
    )
  }

  private acquireMaintenance(id: string): () => void {
    const record = this.runtime(id)
    if (record.maintenance) {
      throw new Error(`Plugin ${id} already has a maintenance operation in progress`)
    }
    record.maintenance = true
    let released = false
    return () => {
      if (released) return
      released = true
      record.maintenance = false
    }
  }

  private hasRuntime(id: string): boolean {
    const record = this.runtimes.get(id)
    return record !== undefined && hasPluginRuntime(record)
  }

  private startTrackedOperation(
    record: PluginRuntimeRecord,
    field: 'activationPromise' | 'stopPromise' | 'deactivationPromise',
    task: () => Promise<void>
  ): Promise<void> {
    let resolveOperation!: () => void
    let rejectOperation!: (error: unknown) => void
    const operation = new Promise<void>((resolve, reject) => {
      resolveOperation = resolve
      rejectOperation = reject
    })
    record[field] = operation
    const clear = (): void => {
      if (record[field] === operation) record[field] = null
    }
    operation.then(clear, clear)
    void task().then(resolveOperation, rejectOperation)
    return operation
  }

  private stopPlugin(id: string): Promise<void> {
    const record = this.runtime(id)
    if (record.stopPromise) return record.stopPromise

    return this.startTrackedOperation(record, 'stopPromise', () => this.performStop(id))
  }

  private async performStop(id: string): Promise<void> {
    const record = this.runtime(id)
    record.rendererOnly = false
    const sandbox = record.sandbox
    if (sandbox) {
      record.expectedStopSandboxes.add(sandbox)
      if (record.sandbox === sandbox) record.sandbox = null
      try {
        await sandbox.stop()
      } finally {
        this.runPluginCleanups(record)
      }
    }

    // If the stop raced with activation, wait for its cancellation/failure so
    // callers never observe a runtime reappearing after stopPlugin resolves.
    if (record.activationPromise) {
      await record.activationPromise.catch(() => undefined)
    }
    record.rendererOnly = false

    const lateSandbox = record.sandbox
    if (lateSandbox && lateSandbox !== sandbox) {
      record.expectedStopSandboxes.add(lateSandbox)
      record.sandbox = null
      try {
        await lateSandbox.stop()
      } finally {
        this.runPluginCleanups(record)
      }
    }
  }

  deactivatePlugin(id: string): Promise<void> {
    this.clearCrashRecovery(id, true)
    if (this.runtime(id).maintenance) {
      return Promise.reject(new Error(`Plugin ${id} is currently being upgraded`))
    }
    const record = this.runtime(id)
    if (record.deactivationPromise) return record.deactivationPromise
    const hadRuntime = this.hasRuntime(id)
    const completion = this.startTrackedOperation(record, 'deactivationPromise', async () => {
      try {
        await this.stopPlugin(id)
        PluginRepository.updateEnabled(id, false)
        this.emitStatus(id, PluginLifecycleStatus.Inactive)
      } catch (error) {
        const message = (error as Error).message
        this.emitStatus(id, PluginLifecycleStatus.Error)
        this.eventBus.emit('plugin:error', { pluginId: id, error: message })
        throw error
      }
    })
    if (hadRuntime) this.emitStatus(id, PluginLifecycleStatus.Deactivating)
    return completion
  }

  async activateAllEnabled(): Promise<void> {
    const plugins = PluginRepository.getEnabledPlugins()
    let nextIndex = 0
    const worker = async (): Promise<void> => {
      while (nextIndex < plugins.length) {
        const plugin = plugins[nextIndex++]
        try {
          await this.activatePluginRuntime(plugin.id)
        } catch (err) {
          console.error(`Failed to activate plugin ${plugin.name}:`, err)
        }
      }
    }
    await Promise.all(
      Array.from({ length: Math.min(this.startupActivationConcurrency, plugins.length) }, worker)
    )
  }

  async deactivateAll(): Promise<void> {
    this.shuttingDown = true
    for (const record of this.runtimes.values()) {
      this.clearCrashRecovery(record.pluginId, false)
    }
    // Application shutdown only stops plugin runtimes. It must not turn the
    // user's enabled plugins into disabled plugins in persistent storage.
    const activeIds = new Set<string>()
    for (const record of this.runtimes.values()) {
      if (record.rendererOnly || record.sandbox || record.activationPromise) {
        activeIds.add(record.pluginId)
      }
    }
    await Promise.all(Array.from(activeIds, (id) => this.stopPlugin(id)))
  }

  async sendMessage(id: string, message: unknown): Promise<unknown> {
    const record = this.runtime(id)
    if (record.stopPromise) {
      throw new Error(`Plugin ${id} is stopping`)
    }
    if (record.activationPromise) await record.activationPromise
    let sandbox = record.sandbox
    if (!sandbox?.isRunning) {
      const plugin = PluginRepository.findById(id)
      if (plugin?.enabled) {
        await this.activatePlugin(id)
        sandbox = record.sandbox
      }
    }
    if (record.rendererOnly) {
      throw new Error(`Plugin ${id} is renderer-only and does not expose backend messages`)
    }
    if (!sandbox?.isRunning) {
      throw new Error(`插件 ${id} 未激活`)
    }
    return await sandbox.sendMessage({ type: 'message', payload: message })
  }

  async updateConfig(id: string, config: PluginConfig): Promise<void> {
    const record = this.runtime(id)
    if (record.maintenance) {
      throw new Error(`Plugin ${id} is currently being upgraded`)
    }
    const plugin = PluginRepository.findById(id)
    if (!plugin) throw new Error(`插件 ${id} 未找到`)
    this.installationService.assertPluginCanRun(plugin.name)
    const releaseMaintenance = this.acquireMaintenance(id)
    try {
      if (record.deactivationPromise) await record.deactivationPromise
      const currentPlugin = PluginRepository.findById(id)
      if (!currentPlugin) throw new Error(`Plugin metadata disappeared during config update: ${id}`)
      this.installationService.assertPluginCanRun(currentPlugin.name)
      const previousConfig = PluginRepository.getConfig(id)
      const hadRuntime = this.hasRuntime(id)
      try {
        if (hadRuntime) await this.stopPlugin(id)
        PluginRepository.updateConfig(id, config)
        if (hadRuntime) await this.activatePluginRuntime(id)
      } catch (error) {
        const rollbackErrors: unknown[] = []
        if (this.hasRuntime(id)) {
          try {
            await this.stopPlugin(id)
          } catch (rollbackError) {
            rollbackErrors.push(rollbackError)
          }
        }
        try {
          PluginRepository.updateConfig(id, previousConfig)
        } catch (rollbackError) {
          rollbackErrors.push(rollbackError)
        }
        if (hadRuntime) {
          try {
            await this.activatePluginRuntime(id)
          } catch (rollbackError) {
            rollbackErrors.push(rollbackError)
          }
        }
        throw this.withRollbackErrors(error, rollbackErrors)
      }
    } finally {
      releaseMaintenance()
    }
  }

  getInstalledPlugins(): PluginMeta[] {
    return PluginRepository.findAll()
  }

  /**
   * Re-orders installed plugins. `orderedIds` must be a complete permutation of
   * every installed plugin id; the repository validates the input and commits
   * the new order transactionally. Enabled plugins keep the same relative
   * activation order as the list because activation follows sort_order.
   */
  reorderPlugins(orderedIds: string[]): PluginMeta[] {
    return PluginRepository.reorder(orderedIds)
  }

  getPlugin(id: string): PluginMeta | null {
    return PluginRepository.findById(id)
  }

  getActivePlugins(): string[] {
    const activeIds = new Set<string>()
    for (const record of this.runtimes.values()) {
      if (record.rendererOnly || record.sandbox?.isRunning) activeIds.add(record.pluginId)
    }
    return Array.from(activeIds)
  }

  getLogs(filter: PluginLogFilter = {}): PluginLogEntry[] {
    return this.logService.getLogs(filter)
  }

  clearLogs(pluginId?: string): void {
    this.logService.clearLogs(pluginId)
  }

  onEvent(event: string, handler: (data: unknown) => void): () => void {
    return this.eventBus.on(event, handler)
  }

  notifyThemeChanged(theme: ToolboxTheme): void {
    this.eventBus.emit('openbox:theme-changed', theme)
  }

  private emitStatus(pluginId: string, status: PluginLifecycleStatus): void {
    this.eventBus.emit('plugin:status', { pluginId, status })
  }
}

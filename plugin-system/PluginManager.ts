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

export class PluginManager {
  private sandboxes: Map<string, PluginSandboxRuntime> = new Map()
  private rendererOnlyActivePluginIds = new Set<string>()
  private childCleanups: Map<string, () => void> = new Map()
  private eventBus: EventBus = new EventBus()
  private activationPromises = new Map<string, Promise<void>>()
  private stopPromises = new Map<string, Promise<void>>()
  private deactivationPromises = new Map<string, Promise<void>>()
  private maintenancePluginIds = new Set<string>()
  private expectedStops = new WeakSet<PluginSandboxRuntime>()
  private reportedErrors = new WeakSet<PluginSandboxRuntime>()
  private crashPolicy: PluginCrashPolicy
  private quarantinedPluginIds = new Set<string>()
  private restartTimers = new Map<string, NodeJS.Timeout>()
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
        getPendingDeactivation: (id) => this.deactivationPromises.get(id),
        acquireMaintenance: (id) => this.acquireMaintenance(id)
      }
    })
    if (options.registerProtocol !== false) PluginProtocol.register(this.pluginsDir)
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
    if (this.maintenancePluginIds.has(id)) {
      return Promise.reject(new Error(`Plugin ${id} is currently being upgraded`))
    }
    return this.activatePluginRuntime(id)
  }

  private activatePluginRuntime(id: string): Promise<void> {
    if (this.quarantinedPluginIds.has(id)) {
      return Promise.reject(new Error(`Plugin ${id} is quarantined after repeated crashes`))
    }
    const stopping = this.stopPromises.get(id)
    if (stopping) return stopping.then(() => this.activatePluginRuntime(id))

    const pending = this.activationPromises.get(id)
    if (pending) return pending
    if (this.rendererOnlyActivePluginIds.has(id)) return Promise.resolve()
    if (this.sandboxes.get(id)?.isRunning) return Promise.resolve()

    return this.startTrackedOperation(this.activationPromises, id, () => this.performActivation(id))
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

    if (manifest.backend === false) {
      this.emitStatus(plugin.id, PluginLifecycleStatus.Activating)
      PluginRepository.updateEnabled(id, true)
      this.rendererOnlyActivePluginIds.add(id)
      console.log(`Plugin activated: ${plugin.name} (renderer-only)`)
      this.emitStatus(plugin.id, PluginLifecycleStatus.Active)
      return
    }

    const config = PluginRepository.getConfig(id)
    const permissions = PermissionGuard.parsePermissions(plugin.permissions as unknown as string[])
    ensureLegacyPluginStorageMigrated(getDatabase(), plugin.id, plugin.name)

    const logger: PluginLogger = {
      info: (msg, ...args) => this.log(plugin.id, 'info', msg, args),
      warn: (msg, ...args) => this.log(plugin.id, 'warn', msg, args),
      error: (msg, ...args) => this.log(plugin.id, 'error', msg, args),
      debug: (msg, ...args) => this.log(plugin.id, 'debug', msg, args)
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
          plugin.id,
          runtimeDb,
          runtimeStorage,
          runtimeApi,
          sandbox,
          op,
          payload
        )
    })
    this.childCleanups.set(`${plugin.id}:trusted-services`, () => {
      void trustedServiceRuntime.dispose().catch((error) => {
        logger.error(`Trusted service cleanup failed: ${(error as Error).message}`)
      })
    })

    sandbox.on('message', (msg: PluginMessage) => {
      this.eventBus.emit(`plugin:message:${plugin.id}`, msg)
      this.eventBus.emit('plugin:message', { pluginId: plugin.id, message: msg })
    })

    sandbox.on('error', (err: Error) => {
      if (!this.expectedStops.has(sandbox)) this.reportSandboxError(plugin.id, logger, sandbox, err)
    })

    sandbox.on('exit', (code: number | null, details?: SandboxExitDetails) => {
      const expected = details?.expected ?? this.expectedStops.has(sandbox)
      if (this.sandboxes.get(plugin.id) === sandbox) this.sandboxes.delete(plugin.id)
      this.runPluginCleanups(plugin.id)
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

    this.sandboxes.set(id, sandbox)
    this.emitStatus(plugin.id, PluginLifecycleStatus.Activating)
    try {
      await sandbox.start()
      if (this.sandboxes.get(id) !== sandbox || !sandbox.isRunning) {
        throw new Error('插件启动完成前已停止')
      }
    } catch (error) {
      if (this.sandboxes.get(id) === sandbox) this.sandboxes.delete(id)
      this.runPluginCleanups(plugin.id)
      await sandbox.stop().catch(() => undefined)
      if (!this.expectedStops.has(sandbox)) {
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
    if (this.reportedErrors.has(sandbox)) return
    this.reportedErrors.add(sandbox)
    logger.error(`插件运行错误: ${error.message}`)
    this.emitStatus(pluginId, PluginLifecycleStatus.Error)
    this.eventBus.emit('plugin:error', { pluginId, error: error.message })
  }

  private clearCrashRecovery(pluginId: string, resetPolicy: boolean): void {
    const timer = this.restartTimers.get(pluginId)
    if (timer) clearTimeout(timer)
    this.restartTimers.delete(pluginId)
    if (resetPolicy) {
      this.crashPolicy.reset(pluginId)
      this.quarantinedPluginIds.delete(pluginId)
    }
  }

  private scheduleCrashRecovery(plugin: PluginMeta, logger: PluginLogger): void {
    if (this.shuttingDown || this.restartTimers.has(plugin.id)) return
    const decision = this.crashPolicy.record(plugin.id)
    if (decision.action === 'quarantine') {
      this.quarantinedPluginIds.add(plugin.id)
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
      this.restartTimers.delete(plugin.id)
      const current = PluginRepository.findById(plugin.id)
      if (
        this.shuttingDown ||
        !current?.enabled ||
        this.maintenancePluginIds.has(plugin.id) ||
        this.deactivationPromises.has(plugin.id)
      ) {
        return
      }
      void this.activatePluginRuntime(plugin.id).catch((error) => {
        console.error(`Failed to recover plugin ${plugin.name}:`, error)
      })
    }, decision.delayMs)
    timer.unref?.()
    this.restartTimers.set(plugin.id, timer)
  }

  private async handleChildRequest(
    pluginId: string,
    runtimeDb: PluginDatabaseAPI,
    runtimeStorage: PluginStorageAPI,
    runtimeApi: PluginRuntimeHostAPI,
    sandbox: PluginSandboxRuntime,
    op: string,
    payload: unknown
  ): Promise<unknown> {
    const p = (payload ?? {}) as Record<string, unknown>

    switch (op) {
      case 'db.query':
        return runtimeDb.query(String(p.sql), p.params as unknown[])
      case 'db.execute':
        await runtimeDb.execute(String(p.sql), p.params as unknown[])
        return null
      case 'storage.get':
        return runtimeStorage.get(String(p.key))
      case 'storage.set':
        await runtimeStorage.set(String(p.key), p.value)
        return null
      case 'storage.delete':
        await runtimeStorage.delete(String(p.key))
        return null
      case 'storage.list':
        return runtimeStorage.list(p.prefix === undefined ? undefined : String(p.prefix))
      case 'storage.batch':
        await runtimeStorage.batch(p.mutations as Parameters<PluginStorageAPI['batch']>[0])
        return null
      case 'log.write': {
        const level = String(p.level)
        this.log(pluginId, level, String(p.message ?? ''), p.args as unknown[])
        return null
      }
      case 'notification.show':
        runtimeApi.notify(String(p.title ?? ''), p.body == null ? undefined : String(p.body))
        return null
      case 'dialog.open':
        return runtimeApi.openDialog(p.type === 'folder' ? 'folder' : 'file')
      case 'network.fetch': {
        const res = await runtimeApi.fetch(String(p.url), p.options as RequestInit)
        const headers: Record<string, string> = {}
        res.headers.forEach((value, key) => {
          headers[key] = value
        })
        const body = await res.text()
        return { ok: res.ok, status: res.status, statusText: res.statusText, headers, body }
      }
      case 'file.read': {
        const data = await runtimeApi.readFile(String(p.path))
        return { base64: Buffer.from(data).toString('base64') }
      }
      case 'file.write':
        await runtimeApi.writeFile(String(p.path), Buffer.from(String(p.base64), 'base64'))
        return null
      case 'shortcut.register': {
        const keys = String(p.keys)
        const cleanup = runtimeApi.registerShortcut(keys, () => {
          sandbox.pushEvent('openbox:shortcut', keys)
        })
        this.childCleanups.set(`${pluginId}:shortcut:${keys}`, cleanup)
        return null
      }
      case 'shortcut.unregister': {
        const key = `${pluginId}:shortcut:${String(p.keys)}`
        this.releaseCleanup(key)
        return null
      }
      case 'event.emit':
        runtimeApi.emitEvent(String(p.event), p.data)
        return null
      case 'event.subscribe': {
        const subId = String(p.subscriptionId ?? '')
        const event = String(p.event)
        const key = `${pluginId}:sub:${subId}`
        const cleanup = runtimeApi.onEvent(event, (data) => {
          sandbox.pushEvent(event, data)
        })
        this.childCleanups.set(key, cleanup)
        return null
      }
      case 'event.unsubscribe': {
        const key = `${pluginId}:sub:${String(p.subscriptionId ?? '')}`
        this.releaseCleanup(key)
        return null
      }
      case 'trusted.invoke':
        return runtimeApi.invokeTrustedService(String(p.service), String(p.operation), p.payload)
      default:
        throw new Error(`未知子进程请求: ${op}`)
    }
  }

  private runPluginCleanups(pluginId: string): void {
    for (const [key, cleanup] of this.childCleanups) {
      if (key.startsWith(`${pluginId}:`)) {
        this.childCleanups.delete(key)
        try {
          cleanup()
        } catch {
          // ignore
        }
      }
    }
  }

  private releaseCleanup(key: string): void {
    const cleanup = this.childCleanups.get(key)
    if (cleanup) {
      this.childCleanups.delete(key)
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
    if (this.maintenancePluginIds.has(id)) {
      throw new Error(`Plugin ${id} already has a maintenance operation in progress`)
    }
    this.maintenancePluginIds.add(id)
    let released = false
    return () => {
      if (released) return
      released = true
      this.maintenancePluginIds.delete(id)
    }
  }

  private hasRuntime(id: string): boolean {
    return (
      this.rendererOnlyActivePluginIds.has(id) ||
      this.sandboxes.has(id) ||
      this.activationPromises.has(id) ||
      this.stopPromises.has(id)
    )
  }

  private startTrackedOperation(
    operations: Map<string, Promise<void>>,
    id: string,
    task: () => Promise<void>
  ): Promise<void> {
    let resolveOperation!: () => void
    let rejectOperation!: (error: unknown) => void
    const operation = new Promise<void>((resolve, reject) => {
      resolveOperation = resolve
      rejectOperation = reject
    })
    operations.set(id, operation)
    const clear = (): void => {
      if (operations.get(id) === operation) operations.delete(id)
    }
    operation.then(clear, clear)
    void task().then(resolveOperation, rejectOperation)
    return operation
  }

  private stopPlugin(id: string): Promise<void> {
    const pending = this.stopPromises.get(id)
    if (pending) return pending

    return this.startTrackedOperation(this.stopPromises, id, () => this.performStop(id))
  }

  private async performStop(id: string): Promise<void> {
    this.rendererOnlyActivePluginIds.delete(id)
    const sandbox = this.sandboxes.get(id)
    if (sandbox) {
      this.expectedStops.add(sandbox)
      if (this.sandboxes.get(id) === sandbox) this.sandboxes.delete(id)
      try {
        await sandbox.stop()
      } finally {
        this.runPluginCleanups(id)
      }
    }

    // If the stop raced with activation, wait for its cancellation/failure so
    // callers never observe a runtime reappearing after stopPlugin resolves.
    const activation = this.activationPromises.get(id)
    if (activation) await activation.catch(() => undefined)
    this.rendererOnlyActivePluginIds.delete(id)

    const lateSandbox = this.sandboxes.get(id)
    if (lateSandbox && lateSandbox !== sandbox) {
      this.expectedStops.add(lateSandbox)
      this.sandboxes.delete(id)
      try {
        await lateSandbox.stop()
      } finally {
        this.runPluginCleanups(id)
      }
    }
  }

  deactivatePlugin(id: string): Promise<void> {
    this.clearCrashRecovery(id, true)
    if (this.maintenancePluginIds.has(id)) {
      return Promise.reject(new Error(`Plugin ${id} is currently being upgraded`))
    }
    const pending = this.deactivationPromises.get(id)
    if (pending) return pending
    const hadRuntime = this.hasRuntime(id)
    const completion = this.startTrackedOperation(this.deactivationPromises, id, async () => {
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
    for (const pluginId of this.restartTimers.keys()) this.clearCrashRecovery(pluginId, false)
    // Application shutdown only stops plugin runtimes. It must not turn the
    // user's enabled plugins into disabled plugins in persistent storage.
    const activeIds = new Set([
      ...this.rendererOnlyActivePluginIds,
      ...this.sandboxes.keys(),
      ...this.activationPromises.keys()
    ])
    await Promise.all(Array.from(activeIds, (id) => this.stopPlugin(id)))
  }

  async sendMessage(id: string, message: unknown): Promise<unknown> {
    if (this.stopPromises.has(id)) {
      throw new Error(`Plugin ${id} is stopping`)
    }
    const activation = this.activationPromises.get(id)
    if (activation) await activation
    let sandbox = this.sandboxes.get(id)
    if (!sandbox?.isRunning) {
      const plugin = PluginRepository.findById(id)
      if (plugin?.enabled) {
        await this.activatePlugin(id)
        sandbox = this.sandboxes.get(id)
      }
    }
    if (this.rendererOnlyActivePluginIds.has(id)) {
      throw new Error(`Plugin ${id} is renderer-only and does not expose backend messages`)
    }
    if (!sandbox?.isRunning) {
      throw new Error(`插件 ${id} 未激活`)
    }
    return await sandbox.sendMessage({ type: 'message', payload: message })
  }

  async updateConfig(id: string, config: PluginConfig): Promise<void> {
    if (this.maintenancePluginIds.has(id)) {
      throw new Error(`Plugin ${id} is currently being upgraded`)
    }
    const plugin = PluginRepository.findById(id)
    if (!plugin) throw new Error(`插件 ${id} 未找到`)
    this.installationService.assertPluginCanRun(plugin.name)
    const releaseMaintenance = this.acquireMaintenance(id)
    try {
      const pendingDeactivation = this.deactivationPromises.get(id)
      if (pendingDeactivation) await pendingDeactivation
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
    const activeIds = new Set(this.rendererOnlyActivePluginIds)
    for (const [id, sandbox] of this.sandboxes.entries()) {
      if (sandbox.isRunning) activeIds.add(id)
    }
    return Array.from(activeIds)
  }

  getLogs(filter: PluginLogFilter = {}): PluginLogEntry[] {
    const conditions: string[] = []
    const params: Array<string | number> = []
    if (filter.pluginId) {
      conditions.push('plugin_id = ?')
      params.push(filter.pluginId)
    }
    if (filter.level) {
      conditions.push('level = ?')
      params.push(filter.level)
    }
    const where = conditions.length ? ` WHERE ${conditions.join(' AND ')}` : ''
    const limit = Math.min(Math.max(filter.limit ?? 500, 1), 2000)
    params.push(limit)
    const rows = dbQueryAll<{
      id: number
      pluginId: string
      level: string
      message: string
      timestamp: string
    }>(
      `SELECT id, plugin_id AS pluginId, level, message, timestamp
       FROM plugin_logs${where} ORDER BY timestamp DESC, id DESC LIMIT ?`,
      params
    )
    return rows.map((row) => ({
      id: row.id,
      pluginId: row.pluginId,
      level: row.level as PluginLogEntry['level'],
      message: row.message,
      timestamp: row.timestamp
    }))
  }

  clearLogs(pluginId?: string): void {
    if (pluginId) {
      dbExecute('DELETE FROM plugin_logs WHERE plugin_id = ?', [pluginId])
      return
    }
    dbExecute('DELETE FROM plugin_logs')
  }

  onEvent(event: string, handler: (data: unknown) => void): () => void {
    return this.eventBus.on(event, handler)
  }

  notifyThemeChanged(theme: ToolboxTheme): void {
    this.eventBus.emit('openbox:theme-changed', theme)
  }

  private log(pluginId: string, level: string, message: string, _args: unknown[]): void {
    const MAX_LOG_LENGTH = 4000
    const trimmed =
      message.length > MAX_LOG_LENGTH ? `${message.slice(0, MAX_LOG_LENGTH)}…` : message
    try {
      dbExecute('INSERT INTO plugin_logs (plugin_id, level, message) VALUES (?, ?, ?)', [
        pluginId,
        level,
        trimmed
      ])
      this.trimPluginLogs(pluginId)
    } catch (err) {
      // db 已关闭/未初始化时降级为控制台输出，避免日志写入抛未捕获异常
      console.error('[plugin-log] drop:', (err as Error)?.message ?? err)
    }
    this.eventBus.emit('plugin:log', { pluginId, level, message: trimmed })
  }

  /** Keep at most 2000 log rows per plugin to avoid unbounded growth. */
  private trimPluginLogs(pluginId: string): void {
    try {
      dbExecute(
        `DELETE FROM plugin_logs WHERE plugin_id = ? AND id NOT IN (
           SELECT id FROM plugin_logs WHERE plugin_id = ? ORDER BY id DESC LIMIT 2000
         )`,
        [pluginId, pluginId]
      )
    } catch {
      // ignore
    }
  }

  private emitStatus(pluginId: string, status: PluginLifecycleStatus): void {
    this.eventBus.emit('plugin:status', { pluginId, status })
  }
}

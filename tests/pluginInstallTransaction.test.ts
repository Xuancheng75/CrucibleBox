// The test tsconfig narrows its include set, so pull in the host's local sql.js declaration.
// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="../shared/types/sql.js.d.ts" />
/// <reference types="react" />

import { EventEmitter } from 'node:events'
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import AdmZip from 'adm-zip'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PluginConfig, PluginMessage, PluginMeta } from '../shared/types/plugin.types'
import { Permission } from '../shared/types/permissions'
import type {
  PluginSandboxRuntime,
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
  getDatabase: vi.fn(() => ({})),
  queryAll: vi.fn(() => [])
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
  deletePluginStorageValue: vi.fn(),
  ensureLegacyPluginStorageMigrated: vi.fn(),
  getPluginStorageValue: vi.fn(() => null),
  listPluginStorageValues: vi.fn(() => []),
  setPluginStorageValue: vi.fn()
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
    register: vi.fn(() => true),
    unregister: vi.fn()
  },
  protocol: { handle: vi.fn() }
}))

import { PluginManager } from '../plugin-system/PluginManager'

interface PluginCreateRecord {
  id: string
  name: string
  version: string
  display_name: string
  description: string
  author: string
  icon: string
  entry_main: string
  entry_renderer: string
  permissions: string
  config_schema: string
  config_data: string
  enabled: number
  installed_path: string
}

interface PluginVersionFields {
  version: string
  display_name: string
  description: string
  author: string
  icon: string
  entry_main: string
  entry_renderer: string
  permissions: string
  config_schema: string
  installed_path: string
}

interface Deferred {
  promise: Promise<void>
  resolve: () => void
}

function createDeferred(): Deferred {
  let resolve!: () => void
  const promise = new Promise<void>((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}

function clonePlugin(plugin: PluginMeta): PluginMeta {
  return {
    ...plugin,
    permissions: [...plugin.permissions],
    configSchema: structuredClone(plugin.configSchema),
    configData: structuredClone(plugin.configData)
  }
}

function writePlugin(
  root: string,
  name: string,
  version: string,
  options: {
    backend?: boolean
    manifestVersion?: 1 | 2
    permissions?: Permission[]
  } = {}
): void {
  mkdirSync(join(root, 'dist'), { recursive: true })
  writeFileSync(
    join(root, 'plugin.json'),
    JSON.stringify({
      name,
      version,
      displayName: `Transaction ${version}`,
      description: `candidate ${version}`,
      author: 'OpenBox tests',
      main: 'dist/main.js',
      renderer: 'dist/renderer.js',
      ...((options.manifestVersion ?? 2) === 2
        ? {
            ...(options.backend === false ? { backend: false } : { backendApiVersion: 2 }),
            manifestVersion: 2,
            rendererApiVersion: 2
          }
        : {}),
      permissions: options.permissions ?? [],
      config: {}
    })
  )
  writeFileSync(join(root, 'dist', 'main.js'), `module.exports = '${version}'`)
  writeFileSync(join(root, 'dist', 'renderer.js'), `module.exports = '${version}'`)
  writeFileSync(join(root, 'version.txt'), version)
}

function createInstalledPlugin(
  id: string,
  name: string,
  version: string,
  enabled: boolean
): PluginMeta {
  return {
    id,
    name,
    version,
    displayName: `Transaction ${version}`,
    description: `installed ${version}`,
    author: 'OpenBox tests',
    entryMain: 'dist/main.js',
    entryRenderer: 'dist/renderer.js',
    permissions: [],
    configSchema: {},
    configData: { preserved: true },
    enabled,
    installedAt: '2026-08-09T00:00:00.000Z',
    updatedAt: '2026-08-09T00:00:00.000Z'
  }
}

function writeUniEnvPlugin(
  root: string,
  name: string,
  version: string,
  options: { extras?: boolean } = {}
): void {
  mkdirSync(join(root, 'dist'), { recursive: true })
  writeFileSync(
    join(root, 'plugin.json'),
    JSON.stringify({
      name,
      version,
      displayName: `UniEnv ${version}`,
      description: `trusted candidate ${version}`,
      author: 'OpenBox tests',
      main: 'dist/main.js',
      renderer: 'dist/renderer.js',
      manifestVersion: 2,
      backendApiVersion: 2,
      rendererApiVersion: 2,
      permissions: [Permission.TrustedUniEnv],
      config: {}
    })
  )
  writeFileSync(join(root, 'dist', 'main.js'), `module.exports = 'trusted-main-${version}'`)
  writeFileSync(join(root, 'dist', 'renderer.js'), `module.exports = 'trusted-renderer-${version}'`)
  if (options.extras) {
    writeFileSync(join(root, 'version.txt'), version)
    mkdirSync(join(root, 'src'), { recursive: true })
    writeFileSync(join(root, 'src', 'deep.js'), 'must not be installed')
    writeFileSync(join(root, 'dist', 'extra.js'), 'extra dist file')
  }
}

function writeUniEnvZip(zipPath: string, version: string): void {
  const archive = new AdmZip()
  archive.addFile(
    'plugin.json',
    Buffer.from(
      JSON.stringify({
        name: 'unienv',
        version,
        displayName: `UniEnv ${version}`,
        description: `trusted candidate ${version}`,
        author: 'OpenBox tests',
        main: 'dist/main.js',
        renderer: 'dist/renderer.js',
        manifestVersion: 2,
        backendApiVersion: 2,
        rendererApiVersion: 2,
        permissions: [Permission.TrustedUniEnv],
        config: {}
      })
    )
  )
  archive.addFile('dist/main.js', Buffer.from(`module.exports = 'trusted-main-${version}'`))
  archive.addFile('dist/renderer.js', Buffer.from(`module.exports = 'trusted-renderer-${version}'`))
  archive.writeZip(zipPath)
}

function installedFileSet(root: string): string[] {
  const files: string[] = []
  const visit = (directory: string, prefix: string): void => {
    for (const entry of readdirSync(directory)) {
      const absolute = join(directory, entry)
      const stats = statSync(absolute)
      if (stats.isDirectory()) {
        visit(absolute, prefix === '' ? entry : `${prefix}/${entry}`)
      } else {
        files.push(prefix === '' ? entry : `${prefix}/${entry}`)
      }
    }
  }
  visit(root, '')
  return files.sort()
}

class FakeSandbox extends EventEmitter implements PluginSandboxRuntime {
  readonly useProcessMode = true
  readonly runtimeKind = 'utility-process' as const
  readonly options: SandboxOptions
  readonly startError?: Error
  startCalls = 0
  stopCalls = 0
  private running = false

  constructor(options: SandboxOptions, startError?: Error) {
    super()
    this.options = options
    this.startError = startError
  }

  get isRunning(): boolean {
    return this.running
  }

  async start(): Promise<void> {
    this.startCalls += 1
    if (this.startError) throw this.startError
    this.running = true
  }

  async stop(): Promise<void> {
    this.stopCalls += 1
    this.running = false
    this.emit('exit', 0, { expected: true, signal: null } satisfies SandboxExitDetails)
  }

  pushEvent(_event: string, _data: unknown): void {}

  async sendMessage(message: PluginMessage): Promise<unknown> {
    return message.payload
  }
}

describe('PluginManager atomic install and upgrade', () => {
  let testDir: string
  let pluginsDir: string
  let records: Map<string, PluginMeta>
  let createFailure: Error | null
  let deleteFailure: Error | null
  let sandboxStartResults: Array<Error | undefined>
  let sandboxes: FakeSandbox[]
  let manager: PluginManager

  beforeEach(() => {
    vi.clearAllMocks()
    testDir = mkdtempSync(join(tmpdir(), 'openbox-manager-transaction-'))
    pluginsDir = join(testDir, 'plugins')
    mkdirSync(pluginsDir)
    records = new Map()
    createFailure = null
    deleteFailure = null
    sandboxStartResults = []
    sandboxes = []

    repositoryMocks.findAll.mockImplementation(() => Array.from(records.values(), clonePlugin))
    repositoryMocks.findById.mockImplementation((id: string) => {
      const plugin = records.get(id)
      return plugin ? clonePlugin(plugin) : null
    })
    repositoryMocks.findByName.mockImplementation((name: string) => {
      const plugin = Array.from(records.values()).find((candidate) => candidate.name === name)
      return plugin ? clonePlugin(plugin) : null
    })
    repositoryMocks.getConfig.mockImplementation((id: string): PluginConfig => {
      const plugin = records.get(id)
      return plugin ? structuredClone(plugin.configData) : {}
    })
    repositoryMocks.getEnabledPlugins.mockImplementation(() =>
      Array.from(records.values())
        .filter((plugin) => plugin.enabled)
        .map(clonePlugin)
    )
    repositoryMocks.create.mockImplementation((record: PluginCreateRecord) => {
      if (createFailure) throw createFailure
      records.set(record.id, {
        id: record.id,
        name: record.name,
        version: record.version,
        displayName: record.display_name,
        description: record.description,
        author: record.author,
        icon: record.icon || undefined,
        entryMain: record.entry_main,
        entryRenderer: record.entry_renderer,
        permissions: JSON.parse(record.permissions) as PluginMeta['permissions'],
        configSchema: JSON.parse(record.config_schema) as PluginMeta['configSchema'],
        configData: JSON.parse(record.config_data) as PluginConfig,
        enabled: record.enabled === 1,
        installedAt: '2026-08-09T00:00:00.000Z',
        updatedAt: '2026-08-09T00:00:00.000Z'
      })
    })
    repositoryMocks.delete.mockImplementation((id: string) => {
      if (deleteFailure) throw deleteFailure
      records.delete(id)
    })
    repositoryMocks.updateEnabled.mockImplementation((id: string, enabled: boolean) => {
      const plugin = records.get(id)
      if (plugin) records.set(id, { ...plugin, enabled })
    })
    repositoryMocks.updateConfig.mockImplementation((id: string, config: PluginConfig) => {
      const plugin = records.get(id)
      if (plugin) records.set(id, { ...plugin, configData: structuredClone(config) })
    })
    repositoryMocks.updatePluginVersion.mockImplementation(
      (id: string, fields: PluginVersionFields) => {
        const plugin = records.get(id)
        if (!plugin) return
        records.set(id, {
          ...plugin,
          version: fields.version,
          displayName: fields.display_name,
          description: fields.description,
          author: fields.author,
          icon: fields.icon || undefined,
          entryMain: fields.entry_main,
          entryRenderer: fields.entry_renderer,
          permissions: JSON.parse(fields.permissions) as PluginMeta['permissions'],
          configSchema: JSON.parse(fields.config_schema) as PluginMeta['configSchema'],
          updatedAt: '2026-08-09T00:00:01.000Z'
        })
      }
    )

    manager = new PluginManager({
      pluginsDir,
      registerProtocol: false,
      sandboxFactory: (options) => {
        const sandbox = new FakeSandbox(options, sandboxStartResults.shift())
        sandboxes.push(sandbox)
        return sandbox
      }
    })
  })

  afterEach(() => {
    const resolved = resolve(testDir)
    const tempRoot = resolve(tmpdir())
    if (resolved.startsWith(`${tempRoot}\\`) || resolved.startsWith(`${tempRoot}/`)) {
      rmSync(resolved, { force: true, recursive: true })
    }
  })

  it('installs a new plugin and commits both its directory and metadata', async () => {
    const sourceDir = join(testDir, 'new-candidate')
    mkdirSync(sourceDir)
    writePlugin(sourceDir, 'transaction-plugin', '1.0.0')

    const installed = await manager.installFromDirectory(sourceDir)

    expect(installed.name).toBe('transaction-plugin')
    expect(installed.version).toBe('1.0.0')
    expect(installed.enabled).toBe(false)
    expect(readFileSync(join(pluginsDir, 'transaction-plugin', 'version.txt'), 'utf8')).toBe(
      '1.0.0'
    )
    expect(repositoryMocks.create).toHaveBeenCalledTimes(1)
    expect(repositoryMocks.delete).not.toHaveBeenCalled()
    expect(readdirSync(pluginsDir)).toEqual(['transaction-plugin'])
  })

  it('rejects new legacy Full Trust packages without deleting their source', async () => {
    const sourceDir = join(testDir, 'legacy-candidate')
    mkdirSync(sourceDir)
    writePlugin(sourceDir, 'legacy-plugin', '1.0.0', { manifestVersion: 1 })

    const preview = manager.previewInstall({ type: 'directory', path: sourceDir })
    expect(preview).toEqual({
      error: expect.stringContaining('legacy v1 packages can no longer be installed')
    })
    expect(() => manager.installFromDirectory(sourceDir)).toThrow(
      'legacy v1 packages can no longer be installed'
    )
    expect(existsSync(sourceDir)).toBe(true)
    expect(records.size).toBe(0)
  })

  it('commits the immutable snapshot the user previewed even if the source changes', async () => {
    const sourceDir = join(testDir, 'preview-candidate')
    mkdirSync(sourceDir)
    writePlugin(sourceDir, 'transaction-plugin', '1.0.0')
    const preview = manager.previewInstall({ type: 'directory', path: sourceDir })
    if ('error' in preview) throw new Error(preview.error)

    writePlugin(sourceDir, 'transaction-plugin', '9.0.0')
    const installed = await manager.commitPreparedInstall(preview.installToken)

    expect(installed.version).toBe('1.0.0')
    expect(readFileSync(join(pluginsDir, 'transaction-plugin', 'version.txt'), 'utf8')).toBe(
      '1.0.0'
    )
    expect(readdirSync(pluginsDir)).toEqual(['transaction-plugin'])
    await expect(manager.commitPreparedInstall(preview.installToken)).rejects.toThrow(
      'missing or expired'
    )
  })

  it('previews manifest mode and exact permission changes before an upgrade', () => {
    const pluginId = 'permission-diff-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    const sourceDir = join(testDir, 'permission-diff-upgrade')
    mkdirSync(targetDir)
    mkdirSync(sourceDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    writePlugin(sourceDir, 'transaction-plugin', '2.0.0', {
      manifestVersion: 2,
      permissions: [Permission.Notification, Permission.StorageWrite]
    })
    const existing = createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', false)
    existing.permissions = [Permission.Notification, Permission.StorageRead]
    records.set(pluginId, existing)

    const preview = manager.previewInstall({ type: 'directory', path: sourceDir })
    if ('error' in preview) throw new Error(preview.error)

    expect(preview).toMatchObject({
      backend: true,
      backendApiVersion: 2,
      isUpgrade: true,
      legacyFullTrust: false,
      manifestVersion: 2,
      previousVersion: '1.0.0',
      rendererApiVersion: 2
    })
    expect(preview.addedPermissions).toEqual([Permission.StorageWrite])
    expect(preview.removedPermissions).toEqual([Permission.StorageRead])
    manager.discardPreparedInstall(preview.installToken)
  })

  it('identifies renderer-only packages without inventing a backend API version', () => {
    const sourceDir = join(testDir, 'renderer-only-candidate')
    mkdirSync(sourceDir)
    writePlugin(sourceDir, 'transaction-plugin', '1.0.0', {
      backend: false,
      manifestVersion: 2
    })

    const preview = manager.previewInstall({ type: 'directory', path: sourceDir })
    if ('error' in preview) throw new Error(preview.error)

    expect(preview).toMatchObject({
      backend: false,
      backendApiVersion: null,
      legacyFullTrust: false,
      rendererApiVersion: 2
    })
    manager.discardPreparedInstall(preview.installToken)
  })

  it('rolls back a new directory when metadata creation fails', async () => {
    const sourceDir = join(testDir, 'failing-candidate')
    mkdirSync(sourceDir)
    writePlugin(sourceDir, 'transaction-plugin', '1.0.0')
    createFailure = new Error('database create failed')

    await expect(manager.installFromDirectory(sourceDir)).rejects.toThrow('database create failed')

    expect(records.size).toBe(0)
    expect(existsSync(join(pluginsDir, 'transaction-plugin'))).toBe(false)
    expect(readdirSync(pluginsDir)).toEqual([])
  })

  it('refuses to overwrite an orphan directory without plugin metadata', async () => {
    const sourceDir = join(testDir, 'orphan-candidate')
    const orphanDir = join(pluginsDir, 'transaction-plugin')
    mkdirSync(sourceDir)
    mkdirSync(orphanDir)
    writePlugin(sourceDir, 'transaction-plugin', '1.0.0')
    writeFileSync(join(orphanDir, 'keep.txt'), 'orphan data')

    await expect(manager.installFromDirectory(sourceDir)).rejects.toThrow(
      'already exists without metadata'
    )

    expect(readFileSync(join(orphanDir, 'keep.txt'), 'utf8')).toBe('orphan data')
    expect(repositoryMocks.create).not.toHaveBeenCalled()
    expect(readdirSync(pluginsDir)).toEqual(['transaction-plugin'])
  })

  it('upgrades a disabled plugin without starting a runtime', async () => {
    const pluginId = 'disabled-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    const sourceDir = join(testDir, 'disabled-upgrade')
    mkdirSync(targetDir)
    mkdirSync(sourceDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    writePlugin(sourceDir, 'transaction-plugin', '2.0.0')
    records.set(pluginId, createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', false))

    const upgraded = await manager.installFromDirectory(sourceDir)

    expect(upgraded.id).toBe(pluginId)
    expect(upgraded.version).toBe('2.0.0')
    expect(upgraded.enabled).toBe(false)
    expect(readFileSync(join(targetDir, 'version.txt'), 'utf8')).toBe('2.0.0')
    expect(sandboxes).toHaveLength(0)
    expect(repositoryMocks.updateEnabled).not.toHaveBeenCalled()
    expect(readdirSync(pluginsDir)).toEqual(['transaction-plugin'])
  })

  it('restores the old directory, metadata, and runtime when the upgraded runtime fails', async () => {
    const pluginId = 'enabled-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    const sourceDir = join(testDir, 'enabled-upgrade')
    mkdirSync(targetDir)
    mkdirSync(sourceDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    writePlugin(sourceDir, 'transaction-plugin', '2.0.0')
    const original = createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', true)
    records.set(pluginId, original)
    sandboxStartResults.push(undefined, new Error('new runtime failed'), undefined)

    await manager.activatePlugin(pluginId)
    expect(manager.getActivePlugins()).toEqual([pluginId])

    await expect(manager.installFromDirectory(sourceDir)).rejects.toThrow('new runtime failed')

    expect(readFileSync(join(targetDir, 'version.txt'), 'utf8')).toBe('1.0.0')
    expect(records.get(pluginId)).toMatchObject({
      id: pluginId,
      version: '1.0.0',
      displayName: original.displayName,
      description: original.description,
      enabled: true,
      configData: { preserved: true }
    })
    expect(manager.getActivePlugins()).toEqual([pluginId])
    expect(sandboxes).toHaveLength(3)
    expect(sandboxes[0].stopCalls).toBe(1)
    expect(sandboxes[1].startCalls).toBe(1)
    expect(sandboxes[1].stopCalls).toBe(1)
    expect(sandboxes[2].isRunning).toBe(true)
    expect(repositoryMocks.updatePluginVersion).toHaveBeenCalledTimes(2)
    expect(readdirSync(pluginsDir)).toEqual(['transaction-plugin'])
  })

  it('honors a deactivation that was already in progress when upgrade begins', async () => {
    const pluginId = 'deactivating-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    const sourceDir = join(testDir, 'deactivation-upgrade')
    mkdirSync(targetDir)
    mkdirSync(sourceDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    writePlugin(sourceDir, 'transaction-plugin', '2.0.0')
    records.set(pluginId, createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', true))
    await manager.activatePlugin(pluginId)

    const deactivation = manager.deactivatePlugin(pluginId)
    const upgrade = manager.installFromDirectory(sourceDir)
    await deactivation
    const upgraded = await upgrade

    expect(upgraded.enabled).toBe(false)
    expect(records.get(pluginId)?.enabled).toBe(false)
    expect(readFileSync(join(targetDir, 'version.txt'), 'utf8')).toBe('2.0.0')
    expect(manager.getActivePlugins()).toEqual([])
    expect(sandboxes).toHaveLength(1)
    expect(sandboxes[0].stopCalls).toBe(1)
  })

  it('updates config without restarting when deactivation is already in progress', async () => {
    const pluginId = 'deactivating-config-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    mkdirSync(targetDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    records.set(pluginId, createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', true))
    await manager.activatePlugin(pluginId)

    const stopStarted = createDeferred()
    const allowStop = createDeferred()
    const sandbox = sandboxes[0]
    const originalStop = sandbox.stop.bind(sandbox)
    vi.spyOn(sandbox, 'stop').mockImplementation(async () => {
      stopStarted.resolve()
      await allowStop.promise
      await originalStop()
    })

    const deactivation = manager.deactivatePlugin(pluginId)
    await stopStarted.promise
    const configUpdate = manager.updateConfig(pluginId, { preserved: false, source: 'race' })
    allowStop.resolve()
    await Promise.all([deactivation, configUpdate])

    expect(records.get(pluginId)).toMatchObject({
      enabled: false,
      configData: { preserved: false, source: 'race' }
    })
    expect(manager.getActivePlugins()).toEqual([])
    expect(sandboxes).toHaveLength(1)
    expect(sandbox.stopCalls).toBe(1)
    expect(repositoryMocks.updateConfig).toHaveBeenCalledTimes(1)
  })

  it('rejects a concurrent install of the same plugin name', async () => {
    const firstSource = join(testDir, 'first-candidate')
    const secondSource = join(testDir, 'second-candidate')
    mkdirSync(firstSource)
    mkdirSync(secondSource)
    writePlugin(firstSource, 'transaction-plugin', '1.0.0')
    writePlugin(secondSource, 'transaction-plugin', '2.0.0')

    const first = manager.installFromDirectory(firstSource)
    const second = manager.installFromDirectory(secondSource)

    await expect(second).rejects.toThrow('already being installed')
    await expect(first).resolves.toMatchObject({ name: 'transaction-plugin', version: '1.0.0' })
    expect(repositoryMocks.create).toHaveBeenCalledTimes(1)
    expect(readFileSync(join(pluginsDir, 'transaction-plugin', 'version.txt'), 'utf8')).toBe(
      '1.0.0'
    )
  })

  it('restores an enabled plugin directory and runtime when metadata deletion fails', async () => {
    const pluginId = 'uninstall-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    mkdirSync(targetDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    records.set(pluginId, createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', true))
    await manager.activatePlugin(pluginId)
    deleteFailure = new Error('database delete failed')

    await expect(manager.uninstall(pluginId)).rejects.toThrow('database delete failed')

    expect(records.has(pluginId)).toBe(true)
    expect(readFileSync(join(targetDir, 'version.txt'), 'utf8')).toBe('1.0.0')
    expect(manager.getActivePlugins()).toEqual([pluginId])
    expect(sandboxes).toHaveLength(2)
    expect(sandboxes[0].stopCalls).toBe(1)
    expect(sandboxes[1].isRunning).toBe(true)
    expect(readdirSync(pluginsDir)).toEqual(['transaction-plugin'])
  })

  it('restores the directory without restarting when uninstall races with deactivation', async () => {
    const pluginId = 'deactivating-uninstall-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    mkdirSync(targetDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    records.set(pluginId, createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', true))
    await manager.activatePlugin(pluginId)
    deleteFailure = new Error('database delete failed')

    const stopStarted = createDeferred()
    const allowStop = createDeferred()
    const sandbox = sandboxes[0]
    const originalStop = sandbox.stop.bind(sandbox)
    vi.spyOn(sandbox, 'stop').mockImplementation(async () => {
      stopStarted.resolve()
      await allowStop.promise
      await originalStop()
    })

    const deactivation = manager.deactivatePlugin(pluginId)
    await stopStarted.promise
    const uninstall = manager.uninstall(pluginId)
    allowStop.resolve()
    await deactivation
    await expect(uninstall).rejects.toThrow('database delete failed')

    expect(records.get(pluginId)?.enabled).toBe(false)
    expect(readFileSync(join(targetDir, 'version.txt'), 'utf8')).toBe('1.0.0')
    expect(manager.getActivePlugins()).toEqual([])
    expect(sandboxes).toHaveLength(1)
    expect(sandbox.stopCalls).toBe(1)
    expect(readdirSync(pluginsDir)).toEqual(['transaction-plugin'])
  })

  it('restores the previous config and runtime when config restart fails', async () => {
    const pluginId = 'config-plugin-id'
    const targetDir = join(pluginsDir, 'transaction-plugin')
    mkdirSync(targetDir)
    writePlugin(targetDir, 'transaction-plugin', '1.0.0')
    records.set(pluginId, createInstalledPlugin(pluginId, 'transaction-plugin', '1.0.0', true))
    sandboxStartResults.push(undefined, new Error('config runtime failed'), undefined)
    await manager.activatePlugin(pluginId)

    await expect(manager.updateConfig(pluginId, { preserved: false })).rejects.toThrow(
      'config runtime failed'
    )

    expect(records.get(pluginId)?.configData).toEqual({ preserved: true })
    expect(manager.getActivePlugins()).toEqual([pluginId])
    expect(sandboxes).toHaveLength(3)
    expect(sandboxes[0].stopCalls).toBe(1)
    expect(sandboxes[1].stopCalls).toBe(1)
    expect(sandboxes[2].isRunning).toBe(true)
  })

  it('installs a trusted UniEnv directory staging only the pinned runtime files', async () => {
    const sourceDir = join(testDir, 'unienv-candidate')
    mkdirSync(sourceDir)
    writeUniEnvPlugin(sourceDir, 'unienv', '1.0.0', { extras: true })

    const installed = await manager.installFromDirectory(sourceDir)

    expect(installed.name).toBe('unienv')
    expect(installed.version).toBe('1.0.0')
    expect(installed.permissions).toEqual([Permission.TrustedUniEnv])
    expect(installedFileSet(join(pluginsDir, 'unienv'))).toEqual([
      'dist/main.js',
      'dist/renderer.js',
      'plugin.json'
    ])
    expect(repositoryMocks.create).toHaveBeenCalledTimes(1)
    expect(readdirSync(pluginsDir)).toEqual(['unienv'])
  })

  it('upgrades a trusted UniEnv plugin keeping only the pinned runtime files', async () => {
    const pluginId = 'unienv-installed-id'
    const targetDir = join(pluginsDir, 'unienv')
    const sourceDir = join(testDir, 'unienv-upgrade')
    mkdirSync(targetDir)
    mkdirSync(sourceDir)
    writeUniEnvPlugin(targetDir, 'unienv', '1.0.0', { extras: true })
    writeUniEnvPlugin(sourceDir, 'unienv', '2.0.0', { extras: true })
    records.set(pluginId, createInstalledPlugin(pluginId, 'unienv', '1.0.0', false))

    const upgraded = await manager.installFromDirectory(sourceDir)

    expect(upgraded.id).toBe(pluginId)
    expect(upgraded.version).toBe('2.0.0')
    expect(installedFileSet(targetDir)).toEqual(['dist/main.js', 'dist/renderer.js', 'plugin.json'])
    expect(readFileSync(join(targetDir, 'dist', 'main.js'), 'utf8')).toBe(
      "module.exports = 'trusted-main-2.0.0'"
    )
    expect(repositoryMocks.updatePluginVersion).toHaveBeenCalledTimes(1)
  })

  it('rejects a trusted UniEnv directory that is missing a required runtime file', async () => {
    const sourceDir = join(testDir, 'unienv-incomplete')
    mkdirSync(sourceDir)
    writeUniEnvPlugin(sourceDir, 'unienv', '1.0.0')
    rmSync(join(sourceDir, 'dist', 'renderer.js'))

    expect(() => manager.installFromDirectory(sourceDir)).toThrow()

    expect(records.size).toBe(0)
    expect(existsSync(join(pluginsDir, 'unienv'))).toBe(false)
    expect(readdirSync(pluginsDir)).toEqual([])
  })

  it('installs the official trusted UniEnv ZIP without regressing', async () => {
    const zipPath = join(testDir, 'unienv-official.zip')
    writeUniEnvZip(zipPath, '1.0.0')

    const installed = await manager.installFromZip(zipPath)

    expect(installed.name).toBe('unienv')
    expect(installed.version).toBe('1.0.0')
    expect(installedFileSet(join(pluginsDir, 'unienv'))).toEqual([
      'dist/main.js',
      'dist/renderer.js',
      'plugin.json'
    ])
    expect(repositoryMocks.create).toHaveBeenCalledTimes(1)
  })

  it('still copies extra files when installing an ordinary plugin from a ZIP', async () => {
    const zipPath = join(testDir, 'ordinary.zip')
    const archive = new AdmZip()
    archive.addFile(
      'plugin.json',
      Buffer.from(
        JSON.stringify({
          name: 'ordinary-zip',
          version: '1.0.0',
          displayName: 'Ordinary ZIP',
          description: 'candidate',
          author: 'OpenBox tests',
          main: 'dist/main.js',
          renderer: 'dist/renderer.js',
          manifestVersion: 2,
          backendApiVersion: 2,
          rendererApiVersion: 2,
          permissions: [],
          config: {}
        })
      )
    )
    archive.addFile('dist/main.js', Buffer.from("module.exports = 'main'"))
    archive.addFile('dist/renderer.js', Buffer.from("module.exports = 'renderer'"))
    archive.addFile('version.txt', Buffer.from('extra zip file'))
    archive.writeZip(zipPath)

    const installed = await manager.installFromZip(zipPath)

    expect(installed.name).toBe('ordinary-zip')
    expect(installedFileSet(join(pluginsDir, 'ordinary-zip'))).toEqual([
      'dist/main.js',
      'dist/renderer.js',
      'plugin.json',
      'version.txt'
    ])
  })
})

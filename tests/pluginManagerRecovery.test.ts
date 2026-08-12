// The test tsconfig narrows its include set, so pull in the host's local sql.js declaration.
// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="../shared/types/sql.js.d.ts" />
/// <reference types="react" />

import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { isAbsolute, join, relative, resolve } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PluginConfig, PluginMeta } from '../shared/types/plugin.types'
import {
  PLUGIN_TRANSACTION_JOURNAL_FILENAME,
  PLUGIN_TRANSACTION_JOURNAL_VERSION,
  writePluginTransactionJournal,
  type PluginTransactionJournal
} from '../plugin-system/PluginTransactionRecovery'

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

function clonePlugin(plugin: PluginMeta): PluginMeta {
  return {
    ...plugin,
    permissions: [...plugin.permissions],
    configSchema: structuredClone(plugin.configSchema),
    configData: structuredClone(plugin.configData)
  }
}

function createPluginMeta(id: string, name: string, version: string, enabled: boolean): PluginMeta {
  return {
    id,
    name,
    version,
    displayName: `Recovery ${version}`,
    description: `metadata ${version}`,
    author: 'OpenBox recovery tests',
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

function writePlugin(root: string, name: string, version: string): void {
  mkdirSync(join(root, 'dist'), { recursive: true })
  writeFileSync(
    join(root, 'plugin.json'),
    JSON.stringify({
      name,
      version,
      displayName: `Recovery ${version}`,
      description: `candidate ${version}`,
      author: 'OpenBox recovery tests',
      main: 'dist/main.js',
      renderer: 'dist/renderer.js',
      permissions: [],
      config: {}
    })
  )
  writeFileSync(join(root, 'dist', 'main.js'), `module.exports = '${version}'`)
  writeFileSync(join(root, 'dist', 'renderer.js'), `module.exports = '${version}'`)
  writeFileSync(join(root, 'version.txt'), version)
}

function toRecoveryMetadata(plugin: PluginMeta): object {
  return {
    id: plugin.id,
    enabled: plugin.enabled,
    manifest: {
      name: plugin.name,
      version: plugin.version,
      displayName: plugin.displayName,
      description: plugin.description,
      author: plugin.author,
      main: plugin.entryMain,
      renderer: plugin.entryRenderer,
      permissions: [...plugin.permissions],
      config: structuredClone(plugin.configSchema)
    }
  }
}

describe('PluginManager startup transaction recovery', () => {
  let testDir: string
  let pluginsDir: string
  let records: Map<string, PluginMeta>
  let consoleError: ReturnType<typeof vi.spyOn>
  let consoleInfo: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    vi.clearAllMocks()
    testDir = mkdtempSync(join(tmpdir(), 'openbox-manager-recovery-'))
    pluginsDir = join(testDir, 'plugins')
    mkdirSync(pluginsDir)
    records = new Map()

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
    repositoryMocks.updateEnabled.mockImplementation((id: string, enabled: boolean) => {
      const plugin = records.get(id)
      if (plugin) records.set(id, { ...plugin, enabled })
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

    consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    consoleInfo = vi.spyOn(console, 'info').mockImplementation(() => undefined)
  })

  afterEach(() => {
    consoleError.mockRestore()
    consoleInfo.mockRestore()
    const resolvedTestDir = resolve(testDir)
    const resolvedTempDir = resolve(tmpdir())
    const pathFromTemp = relative(resolvedTempDir, resolvedTestDir)
    if (
      resolvedTestDir !== resolvedTempDir &&
      pathFromTemp !== '' &&
      !pathFromTemp.startsWith('..') &&
      !isAbsolute(pathFromTemp) &&
      resolve(resolvedTestDir).split(/[\\/]/).at(-1)?.startsWith('openbox-manager-recovery-')
    ) {
      rmSync(resolvedTestDir, { force: true, recursive: true })
    }
  })

  it('rolls back an applied upgrade before exposing the manager', () => {
    const pluginId = 'recovery-plugin-id'
    const pluginName = 'recovery-plugin'
    const transactionId = 'manager-upgrade-1'
    const targetDir = join(pluginsDir, pluginName)
    const backupDir = join(pluginsDir, `.${pluginName}.backup-${transactionId}`)
    const previousPlugin = createPluginMeta(pluginId, pluginName, '1.0.0', true)
    const currentPlugin = createPluginMeta(pluginId, pluginName, '2.0.0', true)
    records.set(pluginId, currentPlugin)
    writePlugin(targetDir, pluginName, '2.0.0')
    writePlugin(backupDir, pluginName, '1.0.0')
    const journal: PluginTransactionJournal = {
      version: PLUGIN_TRANSACTION_JOURNAL_VERSION,
      operation: 'upgrade',
      phase: 'applied',
      pluginName,
      transactionId,
      previousMetadata: toRecoveryMetadata(previousPlugin),
      createdAt: '2026-08-09T00:00:00.000Z'
    }
    writePluginTransactionJournal({ pluginsDir, transactionRoot: targetDir, journal })

    new PluginManager({ pluginsDir, registerProtocol: false })

    expect(readFileSync(join(targetDir, 'version.txt'), 'utf8')).toBe('1.0.0')
    expect(existsSync(backupDir)).toBe(false)
    expect(existsSync(join(targetDir, PLUGIN_TRANSACTION_JOURNAL_FILENAME))).toBe(false)
    expect(records.get(pluginId)).toMatchObject({
      id: pluginId,
      name: pluginName,
      version: '1.0.0',
      displayName: previousPlugin.displayName,
      description: previousPlugin.description,
      author: previousPlugin.author,
      enabled: true,
      configData: { preserved: true }
    })
    expect(repositoryMocks.updatePluginVersion).toHaveBeenCalledTimes(1)
    expect(repositoryMocks.updateEnabled).toHaveBeenCalledWith(pluginId, true)
  })

  it('blocks activation when startup finds an invalid journal', async () => {
    const pluginId = 'blocked-plugin-id'
    const pluginName = 'blocked-plugin'
    const targetDir = join(pluginsDir, pluginName)
    const sandboxFactory = vi.fn(() => {
      throw new Error('sandbox must not be created for a recovery-blocked plugin')
    })
    writePlugin(targetDir, pluginName, '1.0.0')
    writeFileSync(join(targetDir, PLUGIN_TRANSACTION_JOURNAL_FILENAME), '{not-json')
    records.set(pluginId, createPluginMeta(pluginId, pluginName, '1.0.0', true))

    const manager = new PluginManager({ pluginsDir, registerProtocol: false, sandboxFactory })

    await expect(manager.activatePlugin(pluginId)).rejects.toThrow(
      'blocked because an interrupted transaction requires manual recovery'
    )
    expect(sandboxFactory).not.toHaveBeenCalled()
    expect(existsSync(join(targetDir, PLUGIN_TRANSACTION_JOURNAL_FILENAME))).toBe(true)
    expect(consoleError).toHaveBeenCalledWith(
      expect.stringContaining('[plugin-recovery] invalid-journal')
    )
  })

  it('rejects a candidate carrying a reserved host transaction marker', () => {
    const candidateDir = join(testDir, 'candidate')
    writePlugin(candidateDir, 'candidate-plugin', '1.0.0')
    writeFileSync(join(candidateDir, PLUGIN_TRANSACTION_JOURNAL_FILENAME), '{}')
    const manager = new PluginManager({ pluginsDir, registerProtocol: false })

    const preview = manager.previewInstall({ type: 'directory', path: candidateDir })

    expect(preview).toEqual({
      error: expect.stringContaining('Plugin candidate contains reserved host marker')
    })
    expect(() => manager.installFromDirectory(candidateDir)).toThrow(
      'Plugin candidate contains reserved host marker'
    )
    expect(repositoryMocks.create).not.toHaveBeenCalled()
    expect(records.size).toBe(0)
  })
})

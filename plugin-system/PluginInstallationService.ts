import { randomUUID } from 'crypto'
import { existsSync } from 'fs'
import { join } from 'path'

import { PluginRepository } from '@database/repositories/plugin.repository'
import type {
  ConfigField,
  PluginConfig,
  PluginManifest,
  PluginMeta
} from '@shared/types/plugin.types'

import {
  PluginDirectoryRemovalTransaction,
  PluginDirectoryTransaction
} from './PluginDirectoryTransaction'
import {
  PluginInstallPreparation,
  stagePluginCandidate,
  type PluginInstallPreview,
  type PluginInstallSource
} from './PluginInstallPreparation'
import {
  assertPluginManifestInstallable,
  parsePluginManifest,
  readPluginManifest,
  validatePluginEntrypoints
} from './PluginManifestPolicy'
import {
  PLUGIN_TRANSACTION_JOURNAL_VERSION,
  assertPluginCandidateHasNoTransactionMarker,
  clearPluginTransactionJournal,
  recoverPluginTransactions,
  writePluginTransactionJournal,
  type PluginTransactionJournal,
  type PluginTransactionOperation
} from './PluginTransactionRecovery'
import { compareVersions } from './semver'
import { trustedUniEnvPolicyFiles } from './TrustedServiceRuntime'

interface PluginRecoveryMetadata {
  id: string
  enabled: boolean
  manifest: PluginManifest
}

export interface PluginInstallationRuntimePort {
  hasRuntime: (id: string) => boolean
  stopRuntime: (id: string) => Promise<void>
  activateRuntime: (id: string) => Promise<void>
  getPendingDeactivation: (id: string) => Promise<void> | undefined
  acquireMaintenance: (id: string) => () => void
}

export interface PluginInstallationServiceOptions {
  allowLegacyFullTrust?: boolean
  pluginsDir: string
  runtime: PluginInstallationRuntimePort
}

export class PluginInstallationService {
  private readonly installPromises = new Map<string, Promise<PluginMeta>>()
  private readonly pluginsDir: string
  private readonly runtime: PluginInstallationRuntimePort
  private readonly preparation: PluginInstallPreparation
  private recoveryBlockedPluginNames = new Set<string>()
  private readonly allowLegacyFullTrust: boolean

  constructor(options: PluginInstallationServiceOptions) {
    this.pluginsDir = options.pluginsDir
    this.runtime = options.runtime
    this.allowLegacyFullTrust = options.allowLegacyFullTrust ?? false
    this.recoverInterruptedTransactions()
    this.preparation = new PluginInstallPreparation({
      pluginsDir: this.pluginsDir,
      allowLegacyFullTrust: this.allowLegacyFullTrust,
      findPluginByName: (name) => PluginRepository.findByName(name),
      assertPluginCanRun: (name) => this.assertPluginCanRun(name),
      installFromDirectory: (dirPath) => this.installFromDirectory(dirPath)
    })
  }

  installFromZip(zipPath: string): Promise<PluginMeta> {
    return this.preparation.installFromZip(zipPath)
  }

  previewInstall(source: PluginInstallSource): PluginInstallPreview {
    return this.preparation.previewInstall(source)
  }

  commitPreparedInstall(installToken: string): Promise<PluginMeta> {
    return this.preparation.commitPreparedInstall(installToken)
  }

  discardPreparedInstall(installToken: string): void {
    this.preparation.discardPreparedInstall(installToken)
  }

  installFromDirectory(dirPath: string): Promise<PluginMeta> {
    const pluginRoot = this.preparation.resolvePluginRoot(dirPath)
    assertPluginCandidateHasNoTransactionMarker(pluginRoot)
    const manifest = readPluginManifest(pluginRoot)
    validatePluginEntrypoints(pluginRoot, manifest)
    assertPluginManifestInstallable(manifest, this.allowLegacyFullTrust)
    this.assertPluginCanRun(manifest.name)
    if (this.installPromises.has(manifest.name)) {
      return Promise.reject(new Error(`Plugin ${manifest.name} is already being installed`))
    }

    const operation = Promise.resolve().then(() =>
      this.performInstallFromDirectory(pluginRoot, manifest)
    )
    this.installPromises.set(manifest.name, operation)
    const clear = (): void => {
      if (this.installPromises.get(manifest.name) === operation) {
        this.installPromises.delete(manifest.name)
      }
    }
    operation.then(clear, clear)
    return operation
  }

  async uninstall(id: string): Promise<void> {
    let plugin = PluginRepository.findById(id)
    if (!plugin) throw new Error(`插件 ${id} 未找到`)
    this.assertPluginCanRun(plugin.name)
    if (this.installPromises.has(plugin.name)) {
      throw new Error(`Plugin ${plugin.name} is currently being installed or upgraded`)
    }

    const releaseMaintenance = this.runtime.acquireMaintenance(id)
    try {
      const pendingDeactivation = this.runtime.getPendingDeactivation(id)
      if (pendingDeactivation) await pendingDeactivation
      plugin = PluginRepository.findById(id)
      if (!plugin) throw new Error(`Plugin metadata disappeared during uninstall: ${id}`)

      const pluginDir = join(this.pluginsDir, plugin.name)
      await this.runtime.stopRuntime(id)
      if (!existsSync(pluginDir)) {
        PluginRepository.delete(id)
        return
      }

      const transaction = new PluginDirectoryRemovalTransaction({
        pluginsDir: this.pluginsDir,
        pluginName: plugin.name
      })
      let journal = this.createTransactionJournal(
        'uninstall',
        plugin.name,
        transaction.transactionId,
        this.toRecoveryMetadata(plugin)
      )
      let metadataDeleted = false
      try {
        this.writeTransactionJournal(transaction.targetDir, journal)
        transaction.quarantine()
        journal = this.transitionTransactionJournal(transaction.quarantineDir, journal, 'applied')
        PluginRepository.delete(id)
        metadataDeleted = true
        journal = this.transitionTransactionJournal(transaction.quarantineDir, journal, 'committed')
        try {
          transaction.commit()
        } catch (error) {
          if (transaction.phase !== 'committed') throw error
          console.error(`Plugin ${plugin.name} uninstalled but quarantine cleanup failed:`, error)
        }
      } catch (error) {
        const rollbackErrors: unknown[] = []
        if (!metadataDeleted && transaction.phase !== 'committed') {
          try {
            transaction.rollback()
          } catch (rollbackError) {
            rollbackErrors.push(rollbackError)
          }
        }
        if (!metadataDeleted && transaction.phase === 'rolled-back') {
          try {
            this.clearTransactionJournal(transaction.targetDir, journal)
          } catch (rollbackError) {
            rollbackErrors.push(rollbackError)
          }
        }
        if (metadataDeleted || transaction.phase !== 'rolled-back' || rollbackErrors.length > 0) {
          this.recoveryBlockedPluginNames.add(plugin.name)
        }
        if (
          plugin.enabled &&
          transaction.phase === 'rolled-back' &&
          !this.recoveryBlockedPluginNames.has(plugin.name)
        ) {
          try {
            await this.runtime.activateRuntime(id)
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

  assertPluginCanRun(pluginName: string): void {
    if (this.recoveryBlockedPluginNames.has(pluginName)) {
      throw new Error(
        `Plugin ${pluginName} is blocked because an interrupted transaction requires manual recovery`
      )
    }
  }

  private async performInstallFromDirectory(
    pluginRoot: string,
    manifest: PluginManifest
  ): Promise<PluginMeta> {
    const existing = PluginRepository.findByName(manifest.name)
    if (existing) {
      const releaseMaintenance = this.runtime.acquireMaintenance(existing.id)
      try {
        const comparison = compareVersions(manifest.version, existing.version)
        if (comparison < 0) {
          throw new Error(`插件 "${manifest.name}" 已安装更高版本 ${existing.version}，无法降级`)
        }
        if (comparison === 0) {
          throw new Error(
            `插件 "${manifest.name}" 已安装（版本 ${existing.version}），如需覆盖请先卸载`
          )
        }
        return await this.upgradePlugin(existing, pluginRoot, manifest)
      } finally {
        releaseMaintenance()
      }
    }

    const pluginDir = join(this.pluginsDir, manifest.name)
    if (existsSync(pluginDir)) {
      throw new Error(
        `Plugin directory already exists without metadata; refusing to overwrite it: ${pluginDir}`
      )
    }

    const id = randomUUID()
    const transaction = new PluginDirectoryTransaction({
      pluginsDir: this.pluginsDir,
      pluginName: manifest.name,
      sourceDir: pluginRoot,
      expectedTargetExists: false,
      allowedFiles: trustedUniEnvPolicyFiles(manifest)
    })
    const stagedManifest = stagePluginCandidate(transaction, manifest)
    let journal = this.createTransactionJournal(
      'install',
      stagedManifest.name,
      transaction.transactionId,
      null
    )

    let recordCreated = false
    try {
      this.writeTransactionJournal(transaction.stageDir, journal)
      transaction.swap()
      journal = this.transitionTransactionJournal(transaction.targetDir, journal, 'applied')
      PluginRepository.create({
        id,
        name: stagedManifest.name,
        version: stagedManifest.version,
        display_name: stagedManifest.displayName,
        description: stagedManifest.description,
        author: stagedManifest.author,
        icon: stagedManifest.icon || '',
        entry_main: stagedManifest.main,
        entry_renderer: stagedManifest.renderer,
        permissions: JSON.stringify(stagedManifest.permissions),
        config_schema: JSON.stringify(stagedManifest.config || {}),
        config_data: JSON.stringify(this.getDefaultsFromSchema(stagedManifest.config || {})),
        enabled: 0,
        installed_path: pluginDir
      })
      recordCreated = true
      const installed = PluginRepository.findById(id)
      if (!installed) throw new Error(`Plugin metadata was not persisted: ${stagedManifest.name}`)
      this.commitDirectoryTransaction(transaction, journal)
      return installed
    } catch (error) {
      const rollbackErrors: unknown[] = []
      let metadataRemoved = !recordCreated
      if (recordCreated) {
        try {
          PluginRepository.delete(id)
          metadataRemoved = true
        } catch (rollbackError) {
          rollbackErrors.push(rollbackError)
        }
      }
      if (metadataRemoved && transaction.phase !== 'committed') {
        try {
          transaction.rollback()
        } catch (rollbackError) {
          this.recoveryBlockedPluginNames.add(stagedManifest.name)
          rollbackErrors.push(rollbackError)
        }
      } else if (!metadataRemoved) {
        this.recoveryBlockedPluginNames.add(stagedManifest.name)
      }
      throw this.withRollbackErrors(error, rollbackErrors)
    }
  }

  private async upgradePlugin(
    existing: PluginMeta,
    pluginRoot: string,
    manifest: PluginManifest
  ): Promise<PluginMeta> {
    let wasEnabled = existing.enabled
    let previousPlugin: PluginMeta | null = null
    let journal: PluginTransactionJournal
    const pluginDir = join(this.pluginsDir, existing.name)
    if (!existsSync(pluginDir)) {
      throw new Error(`Installed plugin directory is missing: ${pluginDir}`)
    }
    const transaction = new PluginDirectoryTransaction({
      pluginsDir: this.pluginsDir,
      pluginName: existing.name,
      sourceDir: pluginRoot,
      expectedTargetExists: true,
      allowedFiles: trustedUniEnvPolicyFiles(manifest)
    })
    const stagedManifest = stagePluginCandidate(transaction, manifest)

    let metadataUpdated = false
    try {
      const pendingDeactivation = this.runtime.getPendingDeactivation(existing.id)
      if (pendingDeactivation) await pendingDeactivation
      previousPlugin = PluginRepository.findById(existing.id)
      if (!previousPlugin) {
        throw new Error(`Plugin metadata disappeared during upgrade: ${existing.name}`)
      }
      wasEnabled = previousPlugin.enabled
      journal = this.createTransactionJournal(
        'upgrade',
        existing.name,
        transaction.transactionId,
        this.toRecoveryMetadata(previousPlugin)
      )
      this.writeTransactionJournal(transaction.stageDir, journal)
      if (this.runtime.hasRuntime(existing.id)) await this.runtime.stopRuntime(existing.id)
      transaction.swap()
      journal = this.transitionTransactionJournal(transaction.targetDir, journal, 'applied')
      PluginRepository.updatePluginVersion(
        existing.id,
        this.toPluginVersionFields(stagedManifest, pluginDir)
      )
      metadataUpdated = true

      if (wasEnabled) await this.runtime.activateRuntime(existing.id)
      const upgraded = PluginRepository.findById(existing.id)
      if (!upgraded) throw new Error(`Plugin metadata disappeared during upgrade: ${existing.name}`)
      this.commitDirectoryTransaction(transaction, journal)
      return upgraded
    } catch (error) {
      const rollbackErrors: unknown[] = []
      if (this.runtime.hasRuntime(existing.id)) {
        try {
          await this.runtime.stopRuntime(existing.id)
        } catch (rollbackError) {
          rollbackErrors.push(rollbackError)
        }
      }

      let metadataRestored = !metadataUpdated
      if (metadataUpdated && previousPlugin) {
        try {
          PluginRepository.updatePluginVersion(
            existing.id,
            this.toPluginVersionFields(previousPlugin, pluginDir)
          )
          metadataRestored = true
        } catch (rollbackError) {
          rollbackErrors.push(rollbackError)
        }
      }

      let directoryRestored = transaction.phase === 'rolled-back'
      if (metadataRestored && transaction.phase !== 'committed') {
        try {
          transaction.rollback()
          directoryRestored = true
        } catch (rollbackError) {
          directoryRestored = transaction.phase === 'rolled-back'
          rollbackErrors.push(rollbackError)
        }
      }
      if (!metadataRestored || !directoryRestored) {
        this.recoveryBlockedPluginNames.add(existing.name)
      }

      if (wasEnabled && directoryRestored && metadataRestored) {
        try {
          await this.runtime.activateRuntime(existing.id)
        } catch (rollbackError) {
          rollbackErrors.push(rollbackError)
        }
      }
      throw this.withRollbackErrors(error, rollbackErrors)
    }
  }

  private recoverInterruptedTransactions(): void {
    const report = recoverPluginTransactions({
      pluginsDir: this.pluginsDir,
      findMetadata: (pluginName) => {
        const plugin = PluginRepository.findByName(pluginName)
        return plugin ? this.toRecoveryMetadata(plugin) : null
      },
      restoreMetadata: (pluginName, value) => {
        const snapshot = this.parseRecoveryMetadata(pluginName, value)
        const current = PluginRepository.findByName(pluginName)
        if (!current || current.id !== snapshot.id) {
          throw new Error(`Cannot safely restore metadata for plugin ${pluginName}`)
        }
        PluginRepository.updatePluginVersion(
          current.id,
          this.toPluginVersionFields(snapshot.manifest, join(this.pluginsDir, pluginName))
        )
        PluginRepository.updateEnabled(current.id, snapshot.enabled)
      }
    })
    this.recoveryBlockedPluginNames = new Set(report.blockedPlugins)
    for (const action of report.actions) {
      console.info(
        `[plugin-recovery] ${action.type} ${action.pluginName} (${action.transactionId})`
      )
    }
    for (const issue of report.issues) {
      console.error(`[plugin-recovery] ${issue.code} at ${issue.path}: ${issue.message}`)
    }
  }

  private toRecoveryMetadata(plugin: PluginMeta): PluginRecoveryMetadata {
    const manifest = parsePluginManifest({
      name: plugin.name,
      version: plugin.version,
      displayName: plugin.displayName,
      description: plugin.description,
      author: plugin.author,
      ...(plugin.icon ? { icon: plugin.icon } : {}),
      main: plugin.entryMain,
      renderer: plugin.entryRenderer,
      permissions: [...plugin.permissions],
      config: structuredClone(plugin.configSchema)
    })
    return {
      id: plugin.id,
      enabled: plugin.enabled,
      manifest
    }
  }

  private parseRecoveryMetadata(pluginName: string, value: unknown): PluginRecoveryMetadata {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      throw new Error(`Recovery metadata for ${pluginName} must be an object`)
    }
    const prototype = Object.getPrototypeOf(value)
    if (prototype !== Object.prototype && prototype !== null) {
      throw new Error(`Recovery metadata for ${pluginName} must be a plain object`)
    }
    const record = value as Record<string, unknown>
    const keys = Reflect.ownKeys(record)
    if (
      keys.length !== 3 ||
      !keys.every((key) => typeof key === 'string' && ['id', 'enabled', 'manifest'].includes(key))
    ) {
      throw new Error(`Recovery metadata for ${pluginName} contains unsupported fields`)
    }
    if (
      typeof record.id !== 'string' ||
      record.id.length === 0 ||
      record.id.length > 128 ||
      record.id !== record.id.trim() ||
      Array.from(record.id).some((character) => {
        const code = character.charCodeAt(0)
        return code <= 0x1f || code === 0x7f
      })
    ) {
      throw new Error(`Recovery metadata for ${pluginName} has an invalid id`)
    }
    if (typeof record.enabled !== 'boolean') {
      throw new Error(`Recovery metadata for ${pluginName} has an invalid enabled flag`)
    }
    const manifest = parsePluginManifest(record.manifest)
    if (manifest.name !== pluginName) {
      throw new Error(`Recovery metadata does not match plugin ${pluginName}`)
    }
    return { id: record.id, enabled: record.enabled, manifest }
  }

  private toPluginVersionFields(
    plugin: PluginManifest | PluginMeta,
    installedPath: string
  ): {
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
  } {
    const isInstalled = 'entryMain' in plugin
    return {
      version: plugin.version,
      display_name: plugin.displayName,
      description: plugin.description,
      author: plugin.author,
      icon: plugin.icon || '',
      entry_main: isInstalled ? plugin.entryMain : plugin.main,
      entry_renderer: isInstalled ? plugin.entryRenderer : plugin.renderer,
      permissions: JSON.stringify(plugin.permissions),
      config_schema: JSON.stringify(isInstalled ? plugin.configSchema : plugin.config || {}),
      installed_path: installedPath
    }
  }

  private createTransactionJournal(
    operation: PluginTransactionOperation,
    pluginName: string,
    transactionId: string,
    previousMetadata: PluginRecoveryMetadata | null
  ): PluginTransactionJournal {
    return {
      version: PLUGIN_TRANSACTION_JOURNAL_VERSION,
      operation,
      phase: 'prepared',
      pluginName,
      transactionId,
      previousMetadata,
      createdAt: new Date().toISOString()
    }
  }

  private writeTransactionJournal(
    transactionRoot: string,
    journal: PluginTransactionJournal
  ): void {
    writePluginTransactionJournal({
      pluginsDir: this.pluginsDir,
      transactionRoot,
      journal
    })
  }

  private transitionTransactionJournal(
    transactionRoot: string,
    journal: PluginTransactionJournal,
    phase: 'applied' | 'committed'
  ): PluginTransactionJournal {
    const transitioned = { ...journal, phase } satisfies PluginTransactionJournal
    this.writeTransactionJournal(transactionRoot, transitioned)
    return transitioned
  }

  private clearTransactionJournal(
    transactionRoot: string,
    journal: PluginTransactionJournal
  ): void {
    clearPluginTransactionJournal({
      pluginsDir: this.pluginsDir,
      transactionRoot,
      pluginName: journal.pluginName,
      transactionId: journal.transactionId
    })
  }

  private commitDirectoryTransaction(
    transaction: PluginDirectoryTransaction,
    journal: PluginTransactionJournal
  ): void {
    this.transitionTransactionJournal(transaction.targetDir, journal, 'committed')
    try {
      transaction.commit()
    } catch (error) {
      if (transaction.phase !== 'committed') throw error
      console.error(`Plugin ${journal.pluginName} committed but backup cleanup failed:`, error)
      return
    }
    try {
      this.clearTransactionJournal(transaction.targetDir, journal)
    } catch (error) {
      console.error(`Plugin ${journal.pluginName} journal cleanup failed:`, error)
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

  private getDefaultsFromSchema(schema: Record<string, ConfigField>): PluginConfig {
    const config: PluginConfig = {}
    for (const [key, field] of Object.entries(schema)) {
      if (field.default !== undefined) config[key] = field.default
    }
    return config
  }
}

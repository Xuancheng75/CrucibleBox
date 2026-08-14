import { randomUUID } from 'crypto'
import { existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from 'fs'
import { join } from 'path'

import type { PluginManifest, PluginMeta } from '@shared/types/plugin.types'

import { extractPluginArchive } from './PluginArchivePolicy'
import { PluginDirectoryTransaction } from './PluginDirectoryTransaction'
import {
  assertPluginManifestInstallable,
  readPluginManifest,
  validatePluginEntrypoints
} from './PluginManifestPolicy'
import { assertPluginCandidateHasNoTransactionMarker } from './PluginTransactionRecovery'
import { trustedUniEnvPolicyFiles } from './TrustedServiceRuntime'
import { withRollbackErrors } from './runtime/transactionErrors'
import { assertPluginUpgradeAllowed } from './runtime/installUpgradePolicy'

interface PreparedPluginInstall {
  expiresAt: number
  manifest: PluginManifest
  transaction: PluginDirectoryTransaction
}

export type PluginInstallSource = { type: 'zip' | 'directory'; path: string }

export type PluginInstallPreview =
  | {
      addedPermissions: PluginManifest['permissions']
      backend: boolean
      backendApiVersion: 1 | 2 | null
      installToken: string
      isUpgrade: boolean
      legacyFullTrust: boolean
      manifestVersion: 1 | 2
      name: string
      previousVersion: string | null
      removedPermissions: PluginManifest['permissions']
      rendererApiVersion: 1 | 2
      version: string
      displayName: string
      author: string
      description: string
      permissions: PluginManifest['permissions']
    }
  | { error: string }

export interface PluginInstallPreparationOptions {
  allowLegacyFullTrust: boolean
  pluginsDir: string
  findPluginByName: (name: string) => PluginMeta | null
  assertPluginCanRun: (name: string) => void
  installFromDirectory: (dirPath: string) => Promise<PluginMeta>
}

export function stagePluginCandidate(
  transaction: PluginDirectoryTransaction,
  expectedManifest: PluginManifest
): PluginManifest {
  try {
    transaction.stage()
    assertPluginCandidateHasNoTransactionMarker(transaction.stageDir)
    const stagedManifest = readPluginManifest(transaction.stageDir)
    validatePluginEntrypoints(transaction.stageDir, stagedManifest)
    if (JSON.stringify(stagedManifest) !== JSON.stringify(expectedManifest)) {
      throw new Error('Plugin manifest changed while the candidate was being staged')
    }
    return stagedManifest
  } catch (error) {
    const rollbackErrors: unknown[] = []
    if (transaction.phase !== 'committed') {
      try {
        transaction.rollback()
      } catch (rollbackError) {
        rollbackErrors.push(rollbackError)
      }
    }
    throw withRollbackErrors(error, rollbackErrors)
  }
}

export class PluginInstallPreparation {
  private readonly preparedInstalls = new Map<string, PreparedPluginInstall>()
  private readonly options: PluginInstallPreparationOptions

  constructor(options: PluginInstallPreparationOptions) {
    this.options = options
  }

  async installFromZip(zipPath: string): Promise<PluginMeta> {
    const tempDir = join(this.options.pluginsDir, `.tmp-${randomUUID()}`)
    mkdirSync(tempDir, { recursive: true })

    try {
      extractPluginArchive(zipPath, tempDir)
      return await this.options.installFromDirectory(tempDir)
    } finally {
      this.cleanupTemporaryDirectory(tempDir)
    }
  }

  previewInstall(source: PluginInstallSource): PluginInstallPreview {
    this.cleanupExpiredPreparedInstalls()
    let tempDir: string | null = null
    let preparedTransaction: PluginDirectoryTransaction | null = null
    try {
      let pluginRoot: string
      if (source.type === 'zip') {
        tempDir = join(this.options.pluginsDir, `.tmp-${randomUUID()}`)
        mkdirSync(tempDir)
        extractPluginArchive(source.path, tempDir)
        pluginRoot = this.resolvePluginRoot(tempDir)
      } else {
        pluginRoot = this.resolvePluginRoot(source.path)
      }

      assertPluginCandidateHasNoTransactionMarker(pluginRoot)
      const manifest = readPluginManifest(pluginRoot)
      validatePluginEntrypoints(pluginRoot, manifest)
      assertPluginManifestInstallable(manifest, this.options.allowLegacyFullTrust)
      this.options.assertPluginCanRun(manifest.name)
      const existing = this.options.findPluginByName(manifest.name)
      if (existing) {
        assertPluginUpgradeAllowed(manifest.name, manifest.version, existing.version, '')
      }

      preparedTransaction = new PluginDirectoryTransaction({
        pluginsDir: this.options.pluginsDir,
        pluginName: manifest.name,
        sourceDir: pluginRoot,
        expectedTargetExists: existing !== null,
        allowedFiles: trustedUniEnvPolicyFiles(manifest)
      })
      const stagedManifest = stagePluginCandidate(preparedTransaction, manifest)
      const installToken = randomUUID()
      this.preparedInstalls.set(installToken, {
        expiresAt: Date.now() + 15 * 60 * 1_000,
        manifest: stagedManifest,
        transaction: preparedTransaction
      })
      preparedTransaction = null
      return {
        addedPermissions: stagedManifest.permissions.filter(
          (permission) => !existing?.permissions.includes(permission)
        ),
        backend: stagedManifest.backend !== false,
        backendApiVersion:
          stagedManifest.backend === false ? null : (stagedManifest.backendApiVersion ?? 1),
        installToken,
        isUpgrade: existing !== null,
        legacyFullTrust: (stagedManifest.manifestVersion ?? 1) === 1,
        manifestVersion: stagedManifest.manifestVersion ?? 1,
        name: stagedManifest.name,
        previousVersion: existing?.version ?? null,
        removedPermissions:
          existing?.permissions.filter(
            (permission) => !stagedManifest.permissions.includes(permission)
          ) ?? [],
        rendererApiVersion: stagedManifest.rendererApiVersion ?? 1,
        version: stagedManifest.version,
        displayName: stagedManifest.displayName,
        author: stagedManifest.author,
        description: stagedManifest.description,
        permissions: [...stagedManifest.permissions]
      }
    } catch (error) {
      if (preparedTransaction) {
        try {
          preparedTransaction.rollback()
        } catch (cleanupError) {
          console.error('Failed to clean rejected plugin preview:', cleanupError)
        }
      }
      const message = error instanceof Error ? error.message : String(error)
      console.error('[previewInstall] 读取插件清单失败:', error)
      return { error: `读取插件清单失败: ${message}` }
    } finally {
      if (tempDir) this.cleanupTemporaryDirectory(tempDir)
    }
  }

  commitPreparedInstall(installToken: string): Promise<PluginMeta> {
    this.cleanupExpiredPreparedInstalls()
    const prepared = this.preparedInstalls.get(installToken)
    if (!prepared) return Promise.reject(new Error('Prepared plugin install is missing or expired'))
    this.preparedInstalls.delete(installToken)
    const operation = Promise.resolve().then(() =>
      this.options.installFromDirectory(prepared.transaction.stageDir)
    )
    return operation.finally(() => this.discardPreparedTransaction(prepared))
  }

  discardPreparedInstall(installToken: string): void {
    const prepared = this.preparedInstalls.get(installToken)
    if (!prepared) return
    this.preparedInstalls.delete(installToken)
    this.discardPreparedTransaction(prepared)
  }

  resolvePluginRoot(dirPath: string): string {
    if (existsSync(join(dirPath, 'plugin.json'))) return dirPath

    const childDirs = readdirSync(dirPath)
      .map((entry) => join(dirPath, entry))
      .filter((entryPath) => {
        const stats = lstatSync(entryPath)
        return !stats.isSymbolicLink() && stats.isDirectory()
      })
    if (childDirs.length === 1 && existsSync(join(childDirs[0], 'plugin.json'))) {
      return childDirs[0]
    }
    return dirPath
  }

  private cleanupTemporaryDirectory(tempDir: string): void {
    if (!existsSync(tempDir)) return
    try {
      rmSync(tempDir, { recursive: true, force: true })
    } catch (error) {
      console.error(`Failed to clean plugin temporary directory ${tempDir}:`, error)
    }
  }

  private cleanupExpiredPreparedInstalls(): void {
    const now = Date.now()
    for (const [token, prepared] of this.preparedInstalls) {
      if (prepared.expiresAt > now) continue
      this.preparedInstalls.delete(token)
      this.discardPreparedTransaction(prepared)
    }
  }

  private discardPreparedTransaction(prepared: PreparedPluginInstall): void {
    if (prepared.transaction.phase === 'committed') return
    try {
      prepared.transaction.rollback()
    } catch (error) {
      console.error(`Failed to clean prepared plugin ${prepared.manifest.name}:`, error)
    }
  }
}

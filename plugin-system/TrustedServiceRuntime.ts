// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { createHash } from 'crypto'
import { lstatSync, readFileSync, readdirSync } from 'fs'
import { join, relative, resolve, sep } from 'path'
import { inspectPluginRendererRpcPayload } from '@shared/plugin-renderer-rpc'
import type { PluginBackendRpcJsonValue } from '@shared/types/plugin-backend-rpc.types'
import type { PluginConfig, PluginLogger, PluginManifest } from '@shared/types/plugin.types'
import { Permission } from '@shared/types/permissions'
import policies from '@shared/trusted-service-policies.json'

const MAX_TRUSTED_BUNDLE_BYTES = 5 * 1024 * 1024
const TRUSTED_UNIENV_FILES = ['dist/main.js', 'dist/renderer.js', 'plugin.json'] as const

export interface TrustedBundlePolicy {
  name: string
  version: string
  digest: string
  files: readonly string[]
}

export const TRUSTED_UNIENV_POLICY: Readonly<TrustedBundlePolicy> = Object.freeze({
  ...policies.unienv,
  files: TRUSTED_UNIENV_FILES
})

/**
 * Host-controlled pinned runtime file set for a plugin whose manifest requests
 * the trusted UniEnv service. Directory installs must be staged with exactly
 * these files; the manifest's own entries are never trusted as a file list.
 * Returns undefined for ordinary plugins, keeping their install behavior
 * completely unchanged.
 */
export function trustedUniEnvPolicyFiles(
  manifest: Pick<PluginManifest, 'permissions'>
): readonly string[] | undefined {
  if (!manifest.permissions.includes(Permission.TrustedUniEnv)) return undefined
  return TRUSTED_UNIENV_POLICY.files
}

export interface TrustedServiceRuntimeOptions {
  pluginId: string
  pluginDirectory: string
  manifest: PluginManifest
  config: PluginConfig
  logger: PluginLogger
  policy?: Readonly<TrustedBundlePolicy>
}

interface UniEnvTrustedServiceModule {
  activate(context: {
    id: string
    config: Record<string, unknown>
    logger: PluginLogger
  }): void | Promise<void>
  deactivate(): void | Promise<void>
  onMessage(message: unknown): unknown | Promise<unknown>
}

function listRegularBundleFiles(root: string): string[] {
  const rootPath = resolve(root)
  const files: string[] = []
  const visit = (directory: string): void => {
    for (const name of readdirSync(directory)) {
      const absolutePath = join(directory, name)
      const stats = lstatSync(absolutePath)
      if (stats.isSymbolicLink()) throw new Error('Trusted plugin bundles cannot contain links')
      if (stats.isDirectory()) {
        visit(absolutePath)
        continue
      }
      if (!stats.isFile()) throw new Error('Trusted plugin bundles must contain regular files')
      const relation = relative(rootPath, absolutePath).split(sep).join('/')
      if (!relation || relation.startsWith('../')) throw new Error('Trusted bundle path escaped')
      files.push(relation)
    }
  }
  visit(rootPath)
  return files.sort()
}

export function calculateTrustedBundleDigest(
  pluginDirectory: string,
  expectedFiles: readonly string[]
): string {
  const actualFiles = listRegularBundleFiles(pluginDirectory)
  const normalizedExpected = [...expectedFiles].sort()
  if (
    actualFiles.length !== normalizedExpected.length ||
    actualFiles.some((file, index) => file !== normalizedExpected[index])
  ) {
    throw new Error('Trusted plugin bundle file set does not match the pinned policy')
  }

  const digest = createHash('sha256')
  let totalBytes = 0
  for (const file of actualFiles) {
    const content = readFileSync(join(pluginDirectory, ...file.split('/')))
    totalBytes += content.byteLength
    if (totalBytes > MAX_TRUSTED_BUNDLE_BYTES) throw new Error('Trusted plugin bundle is too large')
    digest.update(file)
    digest.update('\0')
    digest.update(content)
    digest.update('\0')
  }
  return digest.digest('hex')
}

function assertTrustedUniEnvBundle(
  pluginDirectory: string,
  manifest: PluginManifest,
  policy: Readonly<TrustedBundlePolicy>
): void {
  if (
    manifest.name !== policy.name ||
    manifest.version !== policy.version ||
    manifest.manifestVersion !== 2 ||
    manifest.backendApiVersion !== 2 ||
    manifest.main !== 'dist/main.js' ||
    manifest.renderer !== 'dist/renderer.js' ||
    manifest.permissions.length !== 1 ||
    manifest.permissions[0] !== Permission.TrustedUniEnv
  ) {
    throw new Error('UniEnv manifest does not match the pinned trusted-service policy')
  }
  const digest = calculateTrustedBundleDigest(pluginDirectory, policy.files)
  if (digest !== policy.digest) throw new Error('UniEnv trusted bundle digest mismatch')
}

export class TrustedServiceRuntime {
  private active = false
  private disposed = false
  private service: UniEnvTrustedServiceModule | null = null
  private readonly authorizedForUniEnv: boolean
  private readonly options: TrustedServiceRuntimeOptions

  constructor(options: TrustedServiceRuntimeOptions) {
    this.options = options
    this.authorizedForUniEnv = options.manifest.permissions.includes(Permission.TrustedUniEnv)
    if (this.authorizedForUniEnv) {
      assertTrustedUniEnvBundle(
        options.pluginDirectory,
        options.manifest,
        options.policy ?? TRUSTED_UNIENV_POLICY
      )
    }
  }

  private async loadUniEnv(): Promise<UniEnvTrustedServiceModule> {
    if (!this.authorizedForUniEnv)
      throw new Error('Plugin is not authorized for the UniEnv service')
    if (!this.service) {
      // 宿主固定加载：实现位于 plugin-system/trusted-services/unienv/，
      // 不再依赖插件源码目录相对路径（1.6.2 物理隔离）。
      const loaded = await import('./trusted-services/unienv/trusted-service')
      this.service = loaded.default as UniEnvTrustedServiceModule
    }
    return this.service
  }

  async invoke(serviceName: string, operation: string, payload: unknown): Promise<unknown> {
    if (this.disposed) throw new Error('Trusted service runtime is disposed')
    if (serviceName !== 'unienv') throw new Error(`Unknown trusted service: ${serviceName}`)
    const service = await this.loadUniEnv()
    switch (operation) {
      case 'activate':
        if (!this.active) {
          await service.activate({
            id: this.options.pluginId,
            config: this.options.config,
            logger: this.options.logger
          })
          this.active = true
        }
        return null
      case 'message': {
        if (!this.active) throw new Error('UniEnv trusted service is not active')
        const result = await service.onMessage(payload)
        inspectPluginRendererRpcPayload(result === undefined ? null : result)
        return result === undefined ? null : (result as PluginBackendRpcJsonValue)
      }
      case 'deactivate':
        if (this.active) {
          this.active = false
          await service.deactivate()
        }
        return null
      default:
        throw new Error(`Unknown UniEnv service operation: ${operation}`)
    }
  }

  async dispose(): Promise<void> {
    if (this.disposed) return
    this.disposed = true
    if (this.active && this.service) {
      this.active = false
      await this.service.deactivate()
    }
  }
}

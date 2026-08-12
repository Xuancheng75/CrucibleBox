import { lstatSync, readdirSync, realpathSync, renameSync, rmSync } from 'node:fs'
import { basename, dirname, join, resolve } from 'node:path'

export const PLUGIN_NAME_PATTERN = /^[a-z0-9_-]+$/
export const TRANSACTION_ID_PATTERN = /^[a-zA-Z0-9-]+$/
const ARTIFACT_PATTERN = /^\.([a-z0-9_-]+)\.(stage|backup|remove)-([a-zA-Z0-9-]+)$/
const MAX_CLEANUP_ENTRIES = 10_000

export type PluginTransactionArtifactKind = 'stage' | 'backup' | 'remove'

export interface PluginTransactionArtifact {
  kind: PluginTransactionArtifactKind
  pluginName: string
  transactionId: string
  path: string
}

export interface PluginTransactionArtifactGroup {
  pluginName: string
  transactionId: string
  stage?: PluginTransactionArtifact
  backup?: PluginTransactionArtifact
  remove?: PluginTransactionArtifact
}

export interface PluginTransactionRoot {
  kind: 'target' | PluginTransactionArtifactKind
  path: string
  pluginName: string
  transactionId?: string
}

export interface PluginTransactionRootScan {
  roots: PluginTransactionRoot[]
  groups: Map<string, PluginTransactionArtifactGroup>
}

export function pluginTransactionErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export function pluginTransactionPathExists(path: string): boolean {
  try {
    lstatSync(path)
    return true
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false
    throw error
  }
}

export function assertPluginTransactionName(pluginName: string): void {
  if (!PLUGIN_NAME_PATTERN.test(pluginName)) {
    throw new TypeError('pluginName contains unsupported characters')
  }
}

export function assertPluginTransactionId(transactionId: string): void {
  if (!TRANSACTION_ID_PATTERN.test(transactionId)) {
    throw new TypeError('transactionId contains unsupported characters')
  }
}

export function canonicalizePluginTransactionDirectory(pluginsDir: string): string {
  const stats = lstatSync(pluginsDir)
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`pluginsDir must be a regular directory: ${pluginsDir}`)
  }
  return realpathSync(pluginsDir)
}

export function assertPluginTransactionDirectChild(pluginsDir: string, childPath: string): void {
  const resolvedChild = resolve(childPath)
  const childParent = realpathSync(dirname(resolvedChild))
  if (childParent !== pluginsDir || basename(resolvedChild) === '') {
    throw new Error(`Plugin transaction path must be a direct child of pluginsDir: ${childPath}`)
  }
}

export function assertPluginTransactionDirectory(path: string, label: string): void {
  const stats = lstatSync(path)
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`${label} must be a regular directory: ${path}`)
  }
}

export function assertPluginTransactionFile(path: string, label: string): void {
  const stats = lstatSync(path)
  if (!stats.isFile() || stats.isSymbolicLink()) {
    throw new Error(`${label} must be a regular file: ${path}`)
  }
}

export function pluginTransactionKey(pluginName: string, transactionId: string): string {
  return `${pluginName}\0${transactionId}`
}

export function scanPluginTransactionRoots(
  pluginsDir: string,
  onUnsafe: (path: string, message: string, pluginName: string) => void
): PluginTransactionRootScan {
  const roots: PluginTransactionRoot[] = []
  const groups = new Map<string, PluginTransactionArtifactGroup>()
  for (const entry of readdirSync(pluginsDir, { withFileTypes: true })) {
    const path = join(pluginsDir, entry.name)
    const match = ARTIFACT_PATTERN.exec(entry.name)
    if (match === null && !PLUGIN_NAME_PATTERN.test(entry.name)) continue
    if (entry.isSymbolicLink() || !entry.isDirectory()) {
      onUnsafe(
        path,
        'Plugin transaction entry is not a regular directory',
        match?.[1] ?? entry.name
      )
      continue
    }
    try {
      assertPluginTransactionDirectory(path, 'Plugin transaction entry')
    } catch (error) {
      onUnsafe(path, pluginTransactionErrorMessage(error), match?.[1] ?? entry.name)
      continue
    }
    if (match === null) {
      roots.push({ kind: 'target', path, pluginName: entry.name })
      continue
    }
    const [, pluginName, kindValue, transactionId] = match
    const kind = kindValue as PluginTransactionArtifactKind
    const artifact = { kind, path, pluginName, transactionId }
    const key = pluginTransactionKey(pluginName, transactionId)
    const group = groups.get(key) ?? { pluginName, transactionId }
    group[kind] = artifact
    groups.set(key, group)
    roots.push({ kind, path, pluginName, transactionId })
  }
  return { roots, groups }
}

function assertCleanupTree(path: string, count: { value: number }): void {
  const stats = lstatSync(path)
  if (stats.isSymbolicLink()) throw new Error(`Cleanup path contains a symbolic link: ${path}`)
  count.value += 1
  if (count.value > MAX_CLEANUP_ENTRIES) throw new Error('Cleanup path contains too many entries')
  if (!stats.isDirectory()) return
  for (const entry of readdirSync(path)) assertCleanupTree(join(path, entry), count)
}

export function removePluginTransactionDirectory(pluginsDir: string, path: string): void {
  assertPluginTransactionDirectChild(pluginsDir, path)
  assertPluginTransactionDirectory(path, 'Recovery directory')
  assertCleanupTree(path, { value: 0 })
  rmSync(path, { force: true, recursive: true })
}

export function renamePluginTransactionDirectory(
  pluginsDir: string,
  source: string,
  destination: string
): void {
  assertPluginTransactionDirectChild(pluginsDir, source)
  assertPluginTransactionDirectChild(pluginsDir, destination)
  assertPluginTransactionDirectory(source, 'Recovery source')
  if (pluginTransactionPathExists(destination)) {
    throw new Error(`Recovery destination already exists: ${destination}`)
  }
  renameSync(source, destination)
}

export type PluginTransactionOrphanAction =
  | 'remove-orphan-stage'
  | 'restore-orphan-backup'
  | 'restore-orphan-uninstall'
  | 'commit-orphan-uninstall'

export interface ReconcilePluginTransactionOrphanOptions {
  pluginsDir: string
  group: PluginTransactionArtifactGroup
  findMetadata(pluginName: string): unknown | null
  onAction(type: PluginTransactionOrphanAction, path: string): void
  onIssue(
    code: 'conflict' | 'metadata-error' | 'cleanup-error',
    path: string,
    message: string
  ): void
}

export function reconcilePluginTransactionOrphan(
  options: ReconcilePluginTransactionOrphanOptions
): void {
  const { group, pluginsDir } = options
  const target = join(pluginsDir, group.pluginName)
  if (group.remove !== undefined) {
    let metadata: unknown | null
    try {
      metadata = options.findMetadata(group.pluginName)
    } catch (error) {
      options.onIssue('metadata-error', group.remove.path, pluginTransactionErrorMessage(error))
      return
    }
    try {
      if (metadata !== null) {
        if (pluginTransactionPathExists(target)) throw new Error('Target and quarantine both exist')
        renamePluginTransactionDirectory(pluginsDir, group.remove.path, target)
        options.onAction('restore-orphan-uninstall', target)
      } else {
        removePluginTransactionDirectory(pluginsDir, group.remove.path)
        options.onAction('commit-orphan-uninstall', group.remove.path)
      }
    } catch (error) {
      const code = metadata === null ? 'cleanup-error' : 'conflict'
      options.onIssue(code, group.remove.path, pluginTransactionErrorMessage(error))
    }
    return
  }
  if (group.backup !== undefined) {
    if (pluginTransactionPathExists(target)) {
      options.onIssue('conflict', group.backup.path, 'Orphan backup retained')
      return
    }
    try {
      renamePluginTransactionDirectory(pluginsDir, group.backup.path, target)
      options.onAction('restore-orphan-backup', target)
    } catch (error) {
      options.onIssue('conflict', group.backup.path, pluginTransactionErrorMessage(error))
      return
    }
  }
  if (group.stage !== undefined && pluginTransactionPathExists(group.stage.path)) {
    try {
      removePluginTransactionDirectory(pluginsDir, group.stage.path)
      options.onAction('remove-orphan-stage', group.stage.path)
    } catch (error) {
      options.onIssue('cleanup-error', group.stage.path, pluginTransactionErrorMessage(error))
    }
  }
}

// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import {
  chmodSync,
  closeSync,
  fstatSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readSync,
  readdirSync,
  realpathSync,
  renameSync,
  rmSync,
  writeSync
} from 'node:fs'
import type { Stats } from 'node:fs'
import { basename, dirname, isAbsolute, join, posix, resolve, win32 } from 'node:path'
import { randomUUID } from 'node:crypto'

const DEFAULT_MAX_ENTRIES = 5_000
const DEFAULT_MAX_TOTAL_BYTES = 600 * 1024 * 1024
const PLUGIN_NAME_PATTERN = /^[a-z0-9_-]+$/
const TRANSACTION_ID_PATTERN = /^[a-zA-Z0-9-]+$/

export interface PluginDirectoryTransactionOptions {
  pluginsDir: string
  pluginName: string
  sourceDir: string
  transactionId?: string
  maxEntries?: number
  maxTotalBytes?: number
  expectedTargetExists?: boolean
  /**
   * Host-controlled allowlist of source-relative files to copy during staging.
   * When undefined the full source directory tree is copied. When provided only
   * these files are staged and every allowlisted path must be a regular file
   * (never a directory, link, or traversal/absolute path) that actually exists.
   */
  allowedFiles?: readonly string[]
}

export type PluginDirectoryTransactionPhase =
  'created' | 'staged' | 'swapped' | 'committed' | 'rolled-back'

export type PluginDirectoryRemovalPhase = 'created' | 'quarantined' | 'committed' | 'rolled-back'

interface CopyBudget {
  entries: number
  maxEntries: number
  maxTotalBytes: number
  totalBytes: number
}

function assertPositiveSafeInteger(value: number, label: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new TypeError(`${label} must be a positive safe integer`)
  }
}

function pathEntryExists(path: string): boolean {
  try {
    lstatSync(path)
    return true
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false
    throw error
  }
}

function assertPlainDirectory(path: string, label: string): void {
  const stats = lstatSync(path)
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`${label} must be a regular directory: ${path}`)
  }
}

function assertDirectChild(parent: string, child: string, expectedBasename: string): void {
  const canonicalParent = realpathSync(parent)
  const resolvedChild = resolve(child)
  const childParent = realpathSync(dirname(resolvedChild))
  if (childParent !== canonicalParent || basename(resolvedChild) !== expectedBasename) {
    throw new Error(`Transaction path escaped the plugin directory: ${child}`)
  }
}

function removeInternalDirectory(parent: string, child: string, expectedBasename: string): void {
  assertDirectChild(parent, child, expectedBasename)
  if (!pathEntryExists(child)) return
  assertPlainDirectory(child, 'Transaction directory')
  rmSync(child, { force: true, recursive: true })
}

function consumeEntry(budget: CopyBudget, size: number): void {
  budget.entries += 1
  if (budget.entries > budget.maxEntries) {
    throw new Error(`Plugin directory exceeds ${budget.maxEntries} entries`)
  }
  if (!Number.isSafeInteger(size) || size < 0 || budget.totalBytes > budget.maxTotalBytes - size) {
    throw new Error(`Plugin directory exceeds ${budget.maxTotalBytes} bytes`)
  }
  budget.totalBytes += size
}

function copyRegularFile(
  source: string,
  destination: string,
  before: Stats,
  budget: CopyBudget
): void {
  const sourceFd = openSync(source, 'r')
  let destinationFd: number | null = null
  try {
    const opened = fstatSync(sourceFd)
    if (
      !opened.isFile() ||
      opened.dev !== before.dev ||
      opened.ino !== before.ino ||
      opened.size !== before.size
    ) {
      throw new Error(`Plugin source changed while it was being staged: ${source}`)
    }
    consumeEntry(budget, opened.size)
    destinationFd = openSync(destination, 'wx', opened.mode)
    const buffer = Buffer.allocUnsafe(64 * 1024)
    let remaining = opened.size
    while (remaining > 0) {
      const bytesRead = readSync(sourceFd, buffer, 0, Math.min(buffer.length, remaining), null)
      if (bytesRead === 0) {
        throw new Error(`Plugin source was truncated while it was being staged: ${source}`)
      }
      let written = 0
      while (written < bytesRead) {
        const bytesWritten = writeSync(destinationFd, buffer, written, bytesRead - written)
        if (bytesWritten === 0) throw new Error(`Failed to stage plugin file: ${source}`)
        written += bytesWritten
      }
      remaining -= bytesRead
    }
    fsyncSync(destinationFd)
    const after = fstatSync(sourceFd)
    if (
      after.size !== opened.size ||
      after.mtimeMs !== opened.mtimeMs ||
      after.ctimeMs !== opened.ctimeMs
    ) {
      throw new Error(`Plugin source changed while it was being staged: ${source}`)
    }
    chmodSync(destination, opened.mode)
  } finally {
    if (destinationFd !== null) closeSync(destinationFd)
    closeSync(sourceFd)
  }
}

function copyDirectoryContents(source: string, destination: string, budget: CopyBudget): void {
  for (const entry of readdirSync(source)) {
    const sourcePath = join(source, entry)
    const destinationPath = join(destination, entry)
    const stats = lstatSync(sourcePath)
    if (stats.isSymbolicLink()) {
      throw new Error(`Plugin directory contains a symbolic link: ${sourcePath}`)
    }
    if (stats.isDirectory()) {
      consumeEntry(budget, 0)
      mkdirSync(destinationPath)
      copyDirectoryContents(sourcePath, destinationPath, budget)
      continue
    }
    if (!stats.isFile()) {
      throw new Error(`Plugin directory contains an unsupported entry: ${sourcePath}`)
    }
    copyRegularFile(sourcePath, destinationPath, stats, budget)
  }
}

function containsControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code <= 0x1f || code === 0x7f) return true
  }
  return false
}

function normalizeAllowedFilePaths(allowedFiles: readonly string[]): readonly string[] {
  const normalized = new Set<string>()
  for (const raw of allowedFiles) {
    if (typeof raw !== 'string') {
      throw new TypeError('allowedFiles must be an array of relative file paths')
    }
    const portable = raw.replaceAll('\\', '/')
    if (
      portable.length === 0 ||
      portable.length > 512 ||
      portable.startsWith('/') ||
      isAbsolute(portable) ||
      win32.isAbsolute(portable) ||
      containsControlCharacter(portable) ||
      portable
        .split('/')
        .some(
          (segment) =>
            segment === '' ||
            segment === '.' ||
            segment === '..' ||
            /[<>:"|?*]/.test(segment) ||
            segment.endsWith('.') ||
            segment.endsWith(' ')
        ) ||
      posix.normalize(portable) !== portable
    ) {
      throw new Error(`Restricted copy contains an unsafe file path: ${raw}`)
    }
    if (normalized.has(portable)) {
      throw new Error(`Restricted copy contains a duplicate file path: ${raw}`)
    }
    normalized.add(portable)
  }
  return [...normalized]
}

function assertTrustedRuntimeFile(source: string, relativePath: string): void {
  const segments = relativePath.split('/')
  let current = source
  for (let index = 0; index < segments.length; index += 1) {
    current = join(current, segments[index])
    const isLeaf = index === segments.length - 1
    let stats: Stats
    try {
      stats = lstatSync(current)
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
        throw new Error(
          `Trusted plugin directory is missing required runtime file ${relativePath}; it should contain only the pinned runtime files`,
          { cause: error }
        )
      }
      throw error
    }
    if (stats.isSymbolicLink()) {
      throw new Error(`Trusted plugin directory contains a symbolic link at: ${relativePath}`)
    }
    if (isLeaf) {
      if (!stats.isFile()) {
        throw new Error(`Trusted plugin runtime file is not a regular file: ${relativePath}`)
      }
    } else if (!stats.isDirectory()) {
      throw new Error(`Trusted plugin directory structure is invalid at: ${relativePath}`)
    }
  }
}

function copyAllowedFiles(
  source: string,
  destination: string,
  allowedFiles: readonly string[],
  budget: CopyBudget
): void {
  for (const file of allowedFiles) {
    assertTrustedRuntimeFile(source, file)
    const sourcePath = join(source, ...file.split('/'))
    const destinationPath = join(destination, ...file.split('/'))
    mkdirSync(dirname(destinationPath), { recursive: true })
    copyRegularFile(sourcePath, destinationPath, lstatSync(sourcePath), budget)
  }
}

export class PluginDirectoryTransaction {
  readonly backupDir: string
  readonly stageDir: string
  readonly targetDir: string
  readonly transactionId: string

  private readonly maxEntries: number
  private readonly maxTotalBytes: number
  private readonly pluginsDir: string
  private readonly sourceDir: string
  private readonly expectedTargetExists: boolean | undefined
  private readonly allowedFiles: readonly string[] | undefined
  private readonly backupBasename: string
  private readonly stageBasename: string
  private originalTargetExisted = false
  private currentPhase: PluginDirectoryTransactionPhase = 'created'

  constructor(options: PluginDirectoryTransactionOptions) {
    if (!PLUGIN_NAME_PATTERN.test(options.pluginName)) {
      throw new TypeError('pluginName contains unsupported characters')
    }
    const transactionId = options.transactionId ?? randomUUID()
    if (!TRANSACTION_ID_PATTERN.test(transactionId)) {
      throw new TypeError('transactionId contains unsupported characters')
    }
    this.transactionId = transactionId

    this.maxEntries = options.maxEntries ?? DEFAULT_MAX_ENTRIES
    this.maxTotalBytes = options.maxTotalBytes ?? DEFAULT_MAX_TOTAL_BYTES
    this.expectedTargetExists = options.expectedTargetExists
    this.allowedFiles =
      options.allowedFiles === undefined
        ? undefined
        : normalizeAllowedFilePaths(options.allowedFiles)
    assertPositiveSafeInteger(this.maxEntries, 'maxEntries')
    assertPositiveSafeInteger(this.maxTotalBytes, 'maxTotalBytes')

    this.pluginsDir = realpathSync(options.pluginsDir)
    assertPlainDirectory(options.sourceDir, 'Plugin source')
    this.sourceDir = realpathSync(options.sourceDir)
    assertPlainDirectory(this.sourceDir, 'Plugin source')
    if (this.sourceDir === this.pluginsDir) {
      throw new Error('Plugin source cannot be the plugins directory')
    }

    this.stageBasename = `.${options.pluginName}.stage-${transactionId}`
    this.backupBasename = `.${options.pluginName}.backup-${transactionId}`
    this.stageDir = join(this.pluginsDir, this.stageBasename)
    this.backupDir = join(this.pluginsDir, this.backupBasename)
    this.targetDir = join(this.pluginsDir, options.pluginName)
    assertDirectChild(this.pluginsDir, this.stageDir, this.stageBasename)
    assertDirectChild(this.pluginsDir, this.backupDir, this.backupBasename)
    assertDirectChild(this.pluginsDir, this.targetDir, options.pluginName)
  }

  get phase(): PluginDirectoryTransactionPhase {
    return this.currentPhase
  }

  stage(): void {
    if (this.currentPhase !== 'created') {
      throw new Error(`Cannot stage a transaction in phase ${this.currentPhase}`)
    }
    if (pathEntryExists(this.stageDir) || pathEntryExists(this.backupDir)) {
      throw new Error('Plugin transaction path already exists')
    }

    this.originalTargetExisted = pathEntryExists(this.targetDir)
    if (
      this.expectedTargetExists !== undefined &&
      this.originalTargetExisted !== this.expectedTargetExists
    ) {
      throw new Error(
        `Installed plugin directory was expected to be ${this.expectedTargetExists ? 'present' : 'absent'}`
      )
    }
    if (this.originalTargetExisted) assertPlainDirectory(this.targetDir, 'Installed plugin')
    mkdirSync(this.stageDir)
    try {
      const budget: CopyBudget = {
        entries: 0,
        maxEntries: this.maxEntries,
        maxTotalBytes: this.maxTotalBytes,
        totalBytes: 0
      }
      if (this.allowedFiles === undefined) {
        copyDirectoryContents(this.sourceDir, this.stageDir, budget)
      } else {
        copyAllowedFiles(this.sourceDir, this.stageDir, this.allowedFiles, budget)
      }
      this.currentPhase = 'staged'
    } catch (error) {
      removeInternalDirectory(this.pluginsDir, this.stageDir, this.stageBasename)
      throw error
    }
  }

  swap(): void {
    if (this.currentPhase !== 'staged') {
      throw new Error(`Cannot swap a transaction in phase ${this.currentPhase}`)
    }
    if (pathEntryExists(this.targetDir) !== this.originalTargetExisted) {
      throw new Error('Installed plugin directory changed during staging')
    }
    assertPlainDirectory(this.stageDir, 'Staged plugin')

    if (this.originalTargetExisted) renameSync(this.targetDir, this.backupDir)
    try {
      renameSync(this.stageDir, this.targetDir)
      this.currentPhase = 'swapped'
    } catch (error) {
      if (this.originalTargetExisted && pathEntryExists(this.backupDir)) {
        renameSync(this.backupDir, this.targetDir)
      }
      throw error
    }
  }

  commit(): void {
    if (this.currentPhase !== 'swapped') {
      throw new Error(`Cannot commit a transaction in phase ${this.currentPhase}`)
    }
    assertPlainDirectory(this.targetDir, 'Installed plugin')
    // Once the new target is active, cleanup failure must never make callers
    // roll the transaction back. A leftover backup is recoverable; replacing
    // a working target after commit is not.
    this.currentPhase = 'committed'
    if (this.originalTargetExisted) {
      removeInternalDirectory(this.pluginsDir, this.backupDir, this.backupBasename)
    }
  }

  rollback(): void {
    if (this.currentPhase === 'rolled-back') return
    if (this.currentPhase === 'committed') {
      throw new Error('Cannot roll back a committed plugin transaction')
    }
    if (this.currentPhase === 'created') {
      this.currentPhase = 'rolled-back'
      return
    }
    if (this.currentPhase === 'staged') {
      if (pathEntryExists(this.backupDir)) {
        if (pathEntryExists(this.targetDir)) {
          throw new Error('Cannot restore plugin backup because the target already exists')
        }
        renameSync(this.backupDir, this.targetDir)
      }
      this.currentPhase = 'rolled-back'
      removeInternalDirectory(this.pluginsDir, this.stageDir, this.stageBasename)
      return
    }

    if (pathEntryExists(this.stageDir)) {
      throw new Error('Cannot roll back because the staging path unexpectedly exists')
    }
    if (pathEntryExists(this.targetDir)) renameSync(this.targetDir, this.stageDir)

    try {
      if (this.originalTargetExisted) {
        if (!pathEntryExists(this.backupDir)) {
          throw new Error('Plugin backup is missing during rollback')
        }
        renameSync(this.backupDir, this.targetDir)
      }
    } catch (error) {
      if (!pathEntryExists(this.targetDir) && pathEntryExists(this.stageDir)) {
        renameSync(this.stageDir, this.targetDir)
      }
      throw error
    }

    this.currentPhase = 'rolled-back'
    removeInternalDirectory(this.pluginsDir, this.stageDir, this.stageBasename)
  }
}

export interface PluginDirectoryRemovalOptions {
  pluginsDir: string
  pluginName: string
  transactionId?: string
}

export class PluginDirectoryRemovalTransaction {
  readonly quarantineDir: string
  readonly targetDir: string
  readonly transactionId: string

  private readonly pluginsDir: string
  private readonly quarantineBasename: string
  private currentPhase: PluginDirectoryRemovalPhase = 'created'

  constructor(options: PluginDirectoryRemovalOptions) {
    if (!PLUGIN_NAME_PATTERN.test(options.pluginName)) {
      throw new TypeError('pluginName contains unsupported characters')
    }
    const transactionId = options.transactionId ?? randomUUID()
    if (!TRANSACTION_ID_PATTERN.test(transactionId)) {
      throw new TypeError('transactionId contains unsupported characters')
    }
    this.transactionId = transactionId
    this.pluginsDir = realpathSync(options.pluginsDir)
    this.quarantineBasename = `.${options.pluginName}.remove-${transactionId}`
    this.targetDir = join(this.pluginsDir, options.pluginName)
    this.quarantineDir = join(this.pluginsDir, this.quarantineBasename)
    assertDirectChild(this.pluginsDir, this.targetDir, options.pluginName)
    assertDirectChild(this.pluginsDir, this.quarantineDir, this.quarantineBasename)
  }

  get phase(): PluginDirectoryRemovalPhase {
    return this.currentPhase
  }

  quarantine(): void {
    if (this.currentPhase !== 'created') {
      throw new Error(`Cannot quarantine a removal in phase ${this.currentPhase}`)
    }
    if (pathEntryExists(this.quarantineDir)) {
      throw new Error('Plugin removal transaction path already exists')
    }
    assertPlainDirectory(this.targetDir, 'Installed plugin')
    renameSync(this.targetDir, this.quarantineDir)
    this.currentPhase = 'quarantined'
  }

  rollback(): void {
    if (this.currentPhase === 'rolled-back') return
    if (this.currentPhase === 'committed') {
      throw new Error('Cannot roll back a committed plugin removal')
    }
    if (this.currentPhase === 'created') {
      this.currentPhase = 'rolled-back'
      return
    }
    if (pathEntryExists(this.targetDir)) {
      throw new Error('Cannot restore removed plugin because the target already exists')
    }
    assertPlainDirectory(this.quarantineDir, 'Quarantined plugin')
    renameSync(this.quarantineDir, this.targetDir)
    this.currentPhase = 'rolled-back'
  }

  commit(): void {
    if (this.currentPhase !== 'quarantined') {
      throw new Error(`Cannot commit a removal in phase ${this.currentPhase}`)
    }
    this.currentPhase = 'committed'
    removeInternalDirectory(this.pluginsDir, this.quarantineDir, this.quarantineBasename)
  }
}

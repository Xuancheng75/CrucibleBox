// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import {
  closeSync,
  fsyncSync,
  lstatSync,
  openSync,
  readFileSync,
  renameSync,
  unlinkSync,
  writeFileSync
} from 'node:fs'
import { basename, join } from 'node:path'
import {
  assertPluginTransactionDirectChild,
  assertPluginTransactionDirectory,
  assertPluginTransactionFile,
  assertPluginTransactionId,
  assertPluginTransactionName,
  canonicalizePluginTransactionDirectory,
  pluginTransactionErrorMessage,
  pluginTransactionKey,
  pluginTransactionPathExists,
  type PluginTransactionRoot
} from './PluginTransactionFs'

export const PLUGIN_TRANSACTION_JOURNAL_VERSION = 1 as const
export const PLUGIN_TRANSACTION_JOURNAL_FILENAME = '.openbox-host-transaction.json'
export const PLUGIN_TRANSACTION_JOURNAL_TEMP_FILENAME = '.openbox-host-transaction.json.pending'
export const PLUGIN_TRANSACTION_MAX_JOURNAL_BYTES = 256 * 1024
export const PLUGIN_TRANSACTION_RESERVED_FILENAMES = Object.freeze([
  PLUGIN_TRANSACTION_JOURNAL_FILENAME,
  PLUGIN_TRANSACTION_JOURNAL_TEMP_FILENAME
])

const JOURNAL_FIELDS = new Set([
  'version',
  'operation',
  'phase',
  'pluginName',
  'transactionId',
  'previousMetadata',
  'createdAt'
])
const FORBIDDEN_JSON_KEYS = new Set(['__proto__', 'constructor', 'prototype'])
const MAX_JSON_DEPTH = 24
const MAX_JSON_ENTRIES = 10_000

export type PluginTransactionOperation = 'install' | 'upgrade' | 'uninstall'
export type PluginTransactionPhase = 'prepared' | 'applied' | 'committed'

export interface PluginTransactionJournal {
  version: typeof PLUGIN_TRANSACTION_JOURNAL_VERSION
  operation: PluginTransactionOperation
  phase: PluginTransactionPhase
  pluginName: string
  transactionId: string
  previousMetadata: unknown | null
  createdAt: string
}

export interface PluginTransactionJournalLocation {
  pluginsDir: string
  transactionRoot: string
}

export interface WritePluginTransactionJournalOptions extends PluginTransactionJournalLocation {
  journal: PluginTransactionJournal
}

export interface ClearPluginTransactionJournalOptions extends PluginTransactionJournalLocation {
  pluginName: string
  transactionId: string
}

export interface PluginTransactionJournalRecord {
  journal: PluginTransactionJournal
  rootPath: string
  rootKind: PluginTransactionRoot['kind']
}

export interface PluginTransactionJournalScan {
  records: Map<string, PluginTransactionJournalRecord>
  claimedRoots: Set<string>
  claimedPlugins: Set<string>
}

function validateJsonValue(value: unknown, depth: number, count: { value: number }): void {
  count.value += 1
  if (count.value > MAX_JSON_ENTRIES) throw new Error('Journal metadata contains too many values')
  if (depth > MAX_JSON_DEPTH) throw new Error('Journal metadata is too deeply nested')
  if (
    value === null ||
    typeof value === 'string' ||
    typeof value === 'boolean' ||
    (typeof value === 'number' && Number.isFinite(value))
  ) {
    return
  }
  if (Array.isArray(value)) {
    for (const entry of value) validateJsonValue(entry, depth + 1, count)
    return
  }
  if (typeof value !== 'object' || Object.getPrototypeOf(value) !== Object.prototype) {
    throw new Error('Journal metadata must contain only JSON values')
  }
  for (const [key, entry] of Object.entries(value)) {
    if (FORBIDDEN_JSON_KEYS.has(key))
      throw new Error(`Journal metadata contains forbidden key: ${key}`)
    validateJsonValue(entry, depth + 1, count)
  }
}

function validateJournal(value: unknown): PluginTransactionJournal {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('Plugin transaction journal must be an object')
  }
  const record = value as Record<string, unknown>
  for (const key of Object.keys(record)) {
    if (!JOURNAL_FIELDS.has(key))
      throw new Error(`Unknown plugin transaction journal field: ${key}`)
  }
  for (const key of JOURNAL_FIELDS) {
    if (!Object.prototype.hasOwnProperty.call(record, key)) {
      throw new Error(`Missing plugin transaction journal field: ${key}`)
    }
  }
  if (record.version !== PLUGIN_TRANSACTION_JOURNAL_VERSION) {
    throw new Error(`Unsupported plugin transaction journal version: ${String(record.version)}`)
  }
  if (!['install', 'upgrade', 'uninstall'].includes(record.operation as string)) {
    throw new Error('Unsupported plugin transaction operation')
  }
  if (!['prepared', 'applied', 'committed'].includes(record.phase as string)) {
    throw new Error('Unsupported plugin transaction phase')
  }
  if (typeof record.pluginName !== 'string') throw new Error('Journal pluginName must be a string')
  if (typeof record.transactionId !== 'string') {
    throw new Error('Journal transactionId must be a string')
  }
  assertPluginTransactionName(record.pluginName)
  assertPluginTransactionId(record.transactionId)
  if (typeof record.createdAt !== 'string' || !Number.isFinite(Date.parse(record.createdAt))) {
    throw new Error('Journal createdAt must be an ISO date string')
  }
  if (record.operation === 'install') {
    if (record.previousMetadata !== null) {
      throw new Error('Install journal previousMetadata must be null')
    }
  } else {
    if (
      typeof record.previousMetadata !== 'object' ||
      record.previousMetadata === null ||
      Array.isArray(record.previousMetadata)
    ) {
      throw new Error('Journal previousMetadata must be an object')
    }
    validateJsonValue(record.previousMetadata, 0, { value: 0 })
  }
  return record as unknown as PluginTransactionJournal
}

export function pluginTransactionJournalPath(transactionRoot: string): string {
  return join(transactionRoot, PLUGIN_TRANSACTION_JOURNAL_FILENAME)
}

export function pluginTransactionJournalTempPath(transactionRoot: string): string {
  return join(transactionRoot, PLUGIN_TRANSACTION_JOURNAL_TEMP_FILENAME)
}

export function readPluginTransactionJournalFile(path: string): PluginTransactionJournal {
  assertPluginTransactionFile(path, 'Plugin transaction journal')
  if (lstatSync(path).size > PLUGIN_TRANSACTION_MAX_JOURNAL_BYTES) {
    throw new Error('Plugin transaction journal is too large')
  }
  return validateJournal(JSON.parse(readFileSync(path, 'utf8')))
}

export function validatePluginTransactionJournalLocation(
  root: PluginTransactionRoot,
  journal: PluginTransactionJournal
): void {
  if (root.pluginName !== journal.pluginName) {
    throw new Error('Journal pluginName does not match its directory')
  }
  if (root.transactionId !== undefined && root.transactionId !== journal.transactionId) {
    throw new Error('Journal transactionId does not match its directory')
  }
  const allowed =
    journal.operation === 'install'
      ? root.kind === 'target' || root.kind === 'stage'
      : journal.operation === 'upgrade'
        ? root.kind === 'target' || root.kind === 'stage'
        : root.kind === 'target' || root.kind === 'remove'
  if (!allowed) throw new Error(`${journal.operation} journal is stored in an invalid directory`)
}

export function assertPluginCandidateHasNoTransactionMarker(candidateDir: string): void {
  assertPluginTransactionDirectory(candidateDir, 'Plugin candidate')
  for (const filename of PLUGIN_TRANSACTION_RESERVED_FILENAMES) {
    if (pluginTransactionPathExists(join(candidateDir, filename))) {
      throw new Error(`Plugin candidate contains reserved host marker: ${filename}`)
    }
  }
}

export function readPluginTransactionJournal(
  options: PluginTransactionJournalLocation
): PluginTransactionJournal | null {
  const pluginsDir = canonicalizePluginTransactionDirectory(options.pluginsDir)
  assertPluginTransactionDirectChild(pluginsDir, options.transactionRoot)
  assertPluginTransactionDirectory(options.transactionRoot, 'Plugin transaction root')
  const path = pluginTransactionJournalPath(options.transactionRoot)
  return pluginTransactionPathExists(path) ? readPluginTransactionJournalFile(path) : null
}

function sameIdentity(
  journal: PluginTransactionJournal,
  pluginName: string,
  transactionId: string
): boolean {
  return journal.pluginName === pluginName && journal.transactionId === transactionId
}

export function writePluginTransactionJournal(options: WritePluginTransactionJournalOptions): void {
  const pluginsDir = canonicalizePluginTransactionDirectory(options.pluginsDir)
  assertPluginTransactionDirectChild(pluginsDir, options.transactionRoot)
  assertPluginTransactionDirectory(options.transactionRoot, 'Plugin transaction root')
  const raw = JSON.stringify(options.journal)
  if (raw === undefined) throw new Error('Plugin transaction journal is not serializable')
  const journal = validateJournal(JSON.parse(raw))
  const path = pluginTransactionJournalPath(options.transactionRoot)
  const tempPath = pluginTransactionJournalTempPath(options.transactionRoot)
  if (pluginTransactionPathExists(tempPath)) {
    throw new Error(`Plugin transaction root contains reserved host marker: ${basename(tempPath)}`)
  }
  if (pluginTransactionPathExists(path)) {
    const existing = readPluginTransactionJournalFile(path)
    if (
      !sameIdentity(existing, journal.pluginName, journal.transactionId) ||
      existing.operation !== journal.operation ||
      existing.createdAt !== journal.createdAt
    ) {
      throw new Error('Plugin transaction root contains a journal from another transaction')
    }
  }
  const serialized = `${JSON.stringify(journal)}\n`
  if (Buffer.byteLength(serialized) > PLUGIN_TRANSACTION_MAX_JOURNAL_BYTES) {
    throw new Error('Plugin transaction journal is too large')
  }
  let descriptor: number | undefined
  try {
    descriptor = openSync(tempPath, 'wx', 0o600)
    writeFileSync(descriptor, serialized, 'utf8')
    fsyncSync(descriptor)
    closeSync(descriptor)
    descriptor = undefined
    renameSync(tempPath, path)
  } catch (error) {
    if (descriptor !== undefined) closeSync(descriptor)
    if (pluginTransactionPathExists(tempPath)) {
      try {
        assertPluginTransactionFile(tempPath, 'Plugin transaction temporary journal')
        unlinkSync(tempPath)
      } catch (cleanupError) {
        void cleanupError
      }
    }
    throw error
  }
}

export function clearPluginTransactionJournal(
  options: ClearPluginTransactionJournalOptions
): boolean {
  assertPluginTransactionName(options.pluginName)
  assertPluginTransactionId(options.transactionId)
  const pluginsDir = canonicalizePluginTransactionDirectory(options.pluginsDir)
  assertPluginTransactionDirectChild(pluginsDir, options.transactionRoot)
  assertPluginTransactionDirectory(options.transactionRoot, 'Plugin transaction root')
  const path = pluginTransactionJournalPath(options.transactionRoot)
  if (!pluginTransactionPathExists(path)) return false
  const journal = readPluginTransactionJournalFile(path)
  if (!sameIdentity(journal, options.pluginName, options.transactionId)) {
    throw new Error('Refusing to clear a journal from another transaction')
  }
  unlinkSync(path)
  return true
}

export function scanPluginTransactionJournals(
  roots: PluginTransactionRoot[],
  onInvalid: (pluginName: string, path: string, message: string) => void
): PluginTransactionJournalScan {
  const records = new Map<string, PluginTransactionJournalRecord>()
  const blockedKeys = new Set<string>()
  const claimedRoots = new Set<string>()
  const claimedPlugins = new Set<string>()
  for (const root of roots) {
    const path = pluginTransactionJournalPath(root.path)
    const pendingPath = pluginTransactionJournalTempPath(root.path)
    if (pluginTransactionPathExists(pendingPath)) {
      claimedRoots.add(root.path)
      claimedPlugins.add(root.pluginName)
      try {
        const pending = readPluginTransactionJournalFile(pendingPath)
        validatePluginTransactionJournalLocation(root, pending)
        if (pluginTransactionPathExists(path)) {
          const current = readPluginTransactionJournalFile(path)
          validatePluginTransactionJournalLocation(root, current)
          if (
            !sameIdentity(current, pending.pluginName, pending.transactionId) ||
            current.operation !== pending.operation ||
            current.createdAt !== pending.createdAt
          ) {
            throw new Error('Pending journal does not match the active journal')
          }
        }
        renameSync(pendingPath, path)
      } catch (error) {
        onInvalid(root.pluginName, pendingPath, pluginTransactionErrorMessage(error))
        continue
      }
    }
    if (!pluginTransactionPathExists(path)) continue
    claimedRoots.add(root.path)
    claimedPlugins.add(root.pluginName)
    try {
      const journal = readPluginTransactionJournalFile(path)
      validatePluginTransactionJournalLocation(root, journal)
      const key = pluginTransactionKey(journal.pluginName, journal.transactionId)
      if (records.has(key) || blockedKeys.has(key)) {
        records.delete(key)
        blockedKeys.add(key)
        throw new Error('Multiple host journals exist for one transaction')
      }
      records.set(key, { journal, rootPath: root.path, rootKind: root.kind })
    } catch (error) {
      onInvalid(root.pluginName, path, pluginTransactionErrorMessage(error))
    }
  }
  return { records, claimedRoots, claimedPlugins }
}

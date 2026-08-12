import { join } from 'node:path'
import {
  canonicalizePluginTransactionDirectory,
  pluginTransactionErrorMessage,
  pluginTransactionPathExists,
  reconcilePluginTransactionOrphan,
  removePluginTransactionDirectory,
  renamePluginTransactionDirectory,
  scanPluginTransactionRoots,
  type PluginTransactionArtifact,
  type PluginTransactionArtifactGroup
} from './PluginTransactionFs'
import {
  clearPluginTransactionJournal,
  pluginTransactionJournalPath,
  scanPluginTransactionJournals,
  type PluginTransactionJournal,
  type PluginTransactionJournalRecord
} from './PluginTransactionJournal'

export {
  PLUGIN_TRANSACTION_JOURNAL_FILENAME,
  PLUGIN_TRANSACTION_JOURNAL_TEMP_FILENAME,
  PLUGIN_TRANSACTION_JOURNAL_VERSION,
  PLUGIN_TRANSACTION_MAX_JOURNAL_BYTES,
  PLUGIN_TRANSACTION_RESERVED_FILENAMES,
  assertPluginCandidateHasNoTransactionMarker,
  clearPluginTransactionJournal,
  readPluginTransactionJournal,
  writePluginTransactionJournal
} from './PluginTransactionJournal'
export type {
  ClearPluginTransactionJournalOptions,
  PluginTransactionJournal,
  PluginTransactionJournalLocation,
  PluginTransactionOperation,
  PluginTransactionPhase,
  WritePluginTransactionJournalOptions
} from './PluginTransactionJournal'

export interface PluginTransactionMetadataCallbacks {
  findMetadata(pluginName: string): unknown | null
  restoreMetadata(pluginName: string, previousMetadata: unknown): void
}

export interface RecoverPluginTransactionsOptions extends PluginTransactionMetadataCallbacks {
  pluginsDir: string
}

export type PluginTransactionRecoveryActionType =
  | 'rollback-install'
  | 'commit-install'
  | 'rollback-upgrade'
  | 'cleanup-committed-upgrade'
  | 'restore-uninstall'
  | 'commit-uninstall'
  | 'remove-orphan-stage'
  | 'restore-orphan-backup'
  | 'restore-orphan-uninstall'
  | 'commit-orphan-uninstall'

export interface PluginTransactionRecoveryAction {
  type: PluginTransactionRecoveryActionType
  pluginName: string
  transactionId: string
  path: string
}

export type PluginTransactionRecoveryIssueCode =
  'unsafe-entry' | 'invalid-journal' | 'conflict' | 'metadata-error' | 'cleanup-error'

export interface PluginTransactionRecoveryIssue {
  code: PluginTransactionRecoveryIssueCode
  path: string
  message: string
}

export interface PluginTransactionRecoveryReport {
  actions: PluginTransactionRecoveryAction[]
  issues: PluginTransactionRecoveryIssue[]
  blockedPlugins: string[]
}

interface RecoveryContext {
  options: RecoverPluginTransactionsOptions
  pluginsDir: string
  report: PluginTransactionRecoveryReport
  blocked: Set<string>
}

function issue(
  context: RecoveryContext,
  code: PluginTransactionRecoveryIssueCode,
  pluginName: string,
  path: string,
  message: string
): void {
  context.report.issues.push({ code, path, message })
  if (code !== 'cleanup-error') context.blocked.add(pluginName)
}

function action(
  context: RecoveryContext,
  type: PluginTransactionRecoveryActionType,
  pluginName: string,
  transactionId: string,
  path: string
): void {
  context.report.actions.push({ type, pluginName, transactionId, path })
}

function journalIssue(
  context: RecoveryContext,
  code: PluginTransactionRecoveryIssueCode,
  journal: PluginTransactionJournal,
  path: string,
  message: string
): void {
  issue(context, code, journal.pluginName, path, message)
}

function caughtIssue(
  context: RecoveryContext,
  code: PluginTransactionRecoveryIssueCode,
  pluginName: string,
  path: string,
  error: unknown
): void {
  issue(context, code, pluginName, path, pluginTransactionErrorMessage(error))
}

function journalCaughtIssue(
  context: RecoveryContext,
  code: PluginTransactionRecoveryIssueCode,
  journal: PluginTransactionJournal,
  path: string,
  error: unknown
): void {
  caughtIssue(context, code, journal.pluginName, path, error)
}

function journalAction(
  context: RecoveryContext,
  type: PluginTransactionRecoveryActionType,
  journal: PluginTransactionJournal,
  path: string
): void {
  action(context, type, journal.pluginName, journal.transactionId, path)
}

function findMetadata(context: RecoveryContext, pluginName: string, path: string) {
  try {
    return { ok: true as const, value: context.options.findMetadata(pluginName) }
  } catch (error) {
    caughtIssue(context, 'metadata-error', pluginName, path, error)
    return { ok: false as const }
  }
}

function metadataEqual(left: unknown, right: unknown): boolean {
  try {
    return JSON.stringify(left) === JSON.stringify(right)
  } catch {
    return false
  }
}

function clearJournal(context: RecoveryContext, root: string, journal: PluginTransactionJournal) {
  try {
    clearPluginTransactionJournal({
      pluginsDir: context.pluginsDir,
      transactionRoot: root,
      pluginName: journal.pluginName,
      transactionId: journal.transactionId
    })
    return true
  } catch (error) {
    journalCaughtIssue(context, 'cleanup-error', journal, root, error)
    return false
  }
}

function recoverInstall(
  context: RecoveryContext,
  record: PluginTransactionJournalRecord,
  group?: PluginTransactionArtifactGroup
): void {
  const { journal } = record
  const target = join(context.pluginsDir, journal.pluginName)
  const metadata = findMetadata(context, journal.pluginName, record.rootPath)
  if (!metadata.ok) return
  if (record.rootKind === 'stage') {
    if (pluginTransactionPathExists(target) || metadata.value !== null) {
      journalIssue(context, 'conflict', journal, record.rootPath, 'Prepared install is ambiguous')
      return
    }
    try {
      removePluginTransactionDirectory(context.pluginsDir, record.rootPath)
      journalAction(context, 'rollback-install', journal, record.rootPath)
    } catch (error) {
      journalCaughtIssue(context, 'cleanup-error', journal, record.rootPath, error)
    }
    return
  }
  if (group?.stage !== undefined || group?.backup !== undefined || group?.remove !== undefined) {
    journalIssue(context, 'conflict', journal, target, 'Install target has conflicting artifacts')
    return
  }
  if (metadata.value !== null) {
    if (clearJournal(context, target, journal)) {
      journalAction(context, 'commit-install', journal, target)
    }
    return
  }
  if (journal.phase === 'committed') {
    journalIssue(context, 'conflict', journal, target, 'Committed install metadata is missing')
    return
  }
  try {
    removePluginTransactionDirectory(context.pluginsDir, target)
    journalAction(context, 'rollback-install', journal, target)
  } catch (error) {
    journalCaughtIssue(context, 'cleanup-error', journal, target, error)
  }
}

function recoverUpgrade(
  context: RecoveryContext,
  record: PluginTransactionJournalRecord,
  group?: PluginTransactionArtifactGroup
): void {
  const { journal } = record
  const target = join(context.pluginsDir, journal.pluginName)
  const stage =
    group?.stage?.path ??
    join(context.pluginsDir, `.${journal.pluginName}.stage-${journal.transactionId}`)
  const backup = group?.backup?.path
  if (journal.phase === 'committed') {
    const metadata = findMetadata(context, journal.pluginName, record.rootPath)
    if (
      record.rootKind !== 'target' ||
      !metadata.ok ||
      metadata.value === null ||
      metadataEqual(metadata.value, journal.previousMetadata)
    ) {
      if (metadata.ok)
        journalIssue(
          context,
          'conflict',
          journal,
          record.rootPath,
          'Committed upgrade is ambiguous'
        )
      return
    }
    try {
      if (backup !== undefined) removePluginTransactionDirectory(context.pluginsDir, backup)
      if (group?.stage !== undefined) removePluginTransactionDirectory(context.pluginsDir, stage)
    } catch (error) {
      journalCaughtIssue(context, 'cleanup-error', journal, record.rootPath, error)
      return
    }
    if (clearJournal(context, target, journal)) {
      journalAction(context, 'cleanup-committed-upgrade', journal, target)
    }
    return
  }
  try {
    if (backup !== undefined) {
      if (pluginTransactionPathExists(target)) {
        if (pluginTransactionPathExists(stage))
          throw new Error('Upgrade staging path already exists')
        renamePluginTransactionDirectory(context.pluginsDir, target, stage)
      }
      renamePluginTransactionDirectory(context.pluginsDir, backup, target)
    } else if (record.rootKind !== 'stage' || !pluginTransactionPathExists(target)) {
      throw new Error('Cannot roll back an upgrade without its backup')
    }
  } catch (error) {
    journalCaughtIssue(context, 'conflict', journal, record.rootPath, error)
    return
  }
  const metadata = findMetadata(context, journal.pluginName, record.rootPath)
  if (!metadata.ok) return
  if (!metadataEqual(metadata.value, journal.previousMetadata)) {
    try {
      context.options.restoreMetadata(journal.pluginName, journal.previousMetadata)
    } catch (error) {
      journalCaughtIssue(context, 'metadata-error', journal, record.rootPath, error)
      return
    }
  }
  try {
    if (pluginTransactionPathExists(stage))
      removePluginTransactionDirectory(context.pluginsDir, stage)
    else if (pluginTransactionPathExists(pluginTransactionJournalPath(target))) {
      if (!clearJournal(context, target, journal)) return
    }
  } catch (error) {
    journalCaughtIssue(context, 'cleanup-error', journal, stage, error)
  }
  journalAction(context, 'rollback-upgrade', journal, target)
}

function recoverUninstall(
  context: RecoveryContext,
  record: PluginTransactionJournalRecord,
  group?: PluginTransactionArtifactGroup
): void {
  const { journal } = record
  const target = join(context.pluginsDir, journal.pluginName)
  const quarantine = group?.remove?.path
  const metadata = findMetadata(context, journal.pluginName, record.rootPath)
  if (!metadata.ok) return
  if (metadata.value !== null) {
    if (pluginTransactionPathExists(target)) {
      if (quarantine !== undefined) {
        journalIssue(context, 'conflict', journal, quarantine, 'Target and quarantine both exist')
        return
      }
    } else if (quarantine !== undefined) {
      try {
        renamePluginTransactionDirectory(context.pluginsDir, quarantine, target)
      } catch (error) {
        journalCaughtIssue(context, 'conflict', journal, quarantine, error)
        return
      }
    } else {
      journalIssue(context, 'conflict', journal, record.rootPath, 'Uninstall quarantine is missing')
      return
    }
    if (clearJournal(context, target, journal)) {
      journalAction(context, 'restore-uninstall', journal, target)
    }
    return
  }
  if (quarantine === undefined || !pluginTransactionPathExists(quarantine)) {
    journalIssue(
      context,
      'conflict',
      journal,
      record.rootPath,
      'Uninstall cannot be committed safely'
    )
    return
  }
  try {
    removePluginTransactionDirectory(context.pluginsDir, quarantine)
    journalAction(context, 'commit-uninstall', journal, quarantine)
  } catch (error) {
    journalCaughtIssue(context, 'cleanup-error', journal, quarantine, error)
  }
}

function orphanArtifacts(group: PluginTransactionArtifactGroup): PluginTransactionArtifact[] {
  return [group.stage, group.backup, group.remove].filter(
    (artifact): artifact is PluginTransactionArtifact => artifact !== undefined
  )
}

export function recoverPluginTransactions(
  options: RecoverPluginTransactionsOptions
): PluginTransactionRecoveryReport {
  const pluginsDir = canonicalizePluginTransactionDirectory(options.pluginsDir)
  const report = { actions: [], issues: [], blockedPlugins: [] } as PluginTransactionRecoveryReport
  const context: RecoveryContext = { options, pluginsDir, report, blocked: new Set() }
  const { roots, groups } = scanPluginTransactionRoots(pluginsDir, (path, message, pluginName) => {
    issue(context, 'unsafe-entry', pluginName, path, message)
  })
  const { records, claimedRoots, claimedPlugins } = scanPluginTransactionJournals(
    roots,
    (pluginName, path, message) => issue(context, 'invalid-journal', pluginName, path, message)
  )
  for (const [key, record] of records) {
    const group = groups.get(key)
    for (const artifact of group === undefined ? [] : orphanArtifacts(group)) {
      claimedRoots.add(artifact.path)
    }
    if (record.journal.operation === 'install') recoverInstall(context, record, group)
    else if (record.journal.operation === 'upgrade') recoverUpgrade(context, record, group)
    else recoverUninstall(context, record, group)
  }
  for (const [key, group] of groups) {
    if (records.has(key)) continue
    const artifacts = orphanArtifacts(group)
    if (artifacts.some((artifact) => claimedRoots.has(artifact.path))) continue
    if (claimedPlugins.has(group.pluginName)) {
      for (const artifact of artifacts) {
        issue(
          context,
          'conflict',
          group.pluginName,
          artifact.path,
          'Artifact conflicts with a journal'
        )
      }
      continue
    }
    reconcilePluginTransactionOrphan({
      pluginsDir,
      group,
      findMetadata: options.findMetadata,
      onAction: (type, path) => action(context, type, group.pluginName, group.transactionId, path),
      onIssue: (code, path, message) => issue(context, code, group.pluginName, path, message)
    })
  }
  report.blockedPlugins = [...context.blocked].sort()
  return report
}

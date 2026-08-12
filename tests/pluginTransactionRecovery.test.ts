import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  PLUGIN_TRANSACTION_JOURNAL_FILENAME,
  PLUGIN_TRANSACTION_JOURNAL_TEMP_FILENAME,
  PLUGIN_TRANSACTION_JOURNAL_VERSION,
  assertPluginCandidateHasNoTransactionMarker,
  clearPluginTransactionJournal,
  readPluginTransactionJournal,
  recoverPluginTransactions,
  writePluginTransactionJournal,
  type PluginTransactionJournal
} from '../plugin-system/PluginTransactionRecovery'

interface Metadata {
  name: string
  version: string
  enabled: boolean
}

describe('PluginTransactionRecovery', () => {
  let testDir: string
  let pluginsDir: string
  let metadata: Map<string, Metadata>

  const oldMetadata: Metadata = { name: 'example', version: '1.0.0', enabled: true }
  const newMetadata: Metadata = { name: 'example', version: '2.0.0', enabled: true }

  function targetPath(pluginName = 'example'): string {
    return join(pluginsDir, pluginName)
  }

  function artifactPath(
    kind: 'stage' | 'backup' | 'remove',
    transactionId = 'tx-1',
    pluginName = 'example'
  ): string {
    return join(pluginsDir, `.${pluginName}.${kind}-${transactionId}`)
  }

  function createPlugin(path: string, version: string): void {
    mkdirSync(path)
    writeFileSync(join(path, 'plugin.json'), JSON.stringify({ version }))
    mkdirSync(join(path, 'dist'))
    writeFileSync(join(path, 'dist', 'main.js'), `module.exports = '${version}'`)
  }

  function pluginVersion(path: string): string {
    return JSON.parse(readFileSync(join(path, 'plugin.json'), 'utf8')).version as string
  }

  function makeJournal(
    operation: 'install' | 'upgrade' | 'uninstall',
    phase: 'prepared' | 'applied' | 'committed',
    transactionId = 'tx-1'
  ): PluginTransactionJournal {
    return {
      version: PLUGIN_TRANSACTION_JOURNAL_VERSION,
      operation,
      phase,
      pluginName: 'example',
      transactionId,
      previousMetadata: operation === 'install' ? null : oldMetadata,
      createdAt: '2026-08-09T00:00:00.000Z'
    }
  }

  function writeJournal(root: string, journal: PluginTransactionJournal): void {
    writePluginTransactionJournal({ pluginsDir, transactionRoot: root, journal })
  }

  function recover() {
    return recoverPluginTransactions({
      pluginsDir,
      findMetadata: (pluginName) => metadata.get(pluginName) ?? null,
      restoreMetadata: (pluginName, previousMetadata) => {
        metadata.set(pluginName, previousMetadata as Metadata)
      }
    })
  }

  beforeEach(() => {
    testDir = mkdtempSync(join(tmpdir(), 'openbox-plugin-recovery-'))
    pluginsDir = join(testDir, 'plugins')
    mkdirSync(pluginsDir)
    metadata = new Map([['example', oldMetadata]])
  })

  afterEach(() => {
    const resolved = resolve(testDir)
    const tempRoot = resolve(tmpdir())
    if (resolved.startsWith(`${tempRoot}\\`) || resolved.startsWith(`${tempRoot}/`)) {
      rmSync(resolved, { force: true, recursive: true })
    }
  })

  it('writes, updates, reads, and identity-checks an atomic host journal', () => {
    const stage = artifactPath('stage')
    createPlugin(stage, '2.0.0')
    const prepared = makeJournal('upgrade', 'prepared')
    writeJournal(stage, prepared)
    expect(readPluginTransactionJournal({ pluginsDir, transactionRoot: stage })).toEqual(prepared)

    const applied = { ...prepared, phase: 'applied' as const }
    writeJournal(stage, applied)
    expect(readPluginTransactionJournal({ pluginsDir, transactionRoot: stage })?.phase).toBe(
      'applied'
    )
    expect(() =>
      clearPluginTransactionJournal({
        pluginsDir,
        transactionRoot: stage,
        pluginName: 'example',
        transactionId: 'another-transaction'
      })
    ).toThrow('another transaction')
    expect(existsSync(join(stage, PLUGIN_TRANSACTION_JOURNAL_FILENAME))).toBe(true)
    expect(
      clearPluginTransactionJournal({
        pluginsDir,
        transactionRoot: stage,
        pluginName: 'example',
        transactionId: 'tx-1'
      })
    ).toBe(true)
  })

  it('rejects candidate markers, linked roots, and transaction roots outside pluginsDir', () => {
    const candidate = join(testDir, 'candidate')
    createPlugin(candidate, '2.0.0')
    writeFileSync(join(candidate, PLUGIN_TRANSACTION_JOURNAL_FILENAME), '{}')
    expect(() => assertPluginCandidateHasNoTransactionMarker(candidate)).toThrow(
      'reserved host marker'
    )

    const linkedCandidate = join(testDir, 'linked-candidate')
    symlinkSync(candidate, linkedCandidate, process.platform === 'win32' ? 'junction' : 'dir')
    expect(() => assertPluginCandidateHasNoTransactionMarker(linkedCandidate)).toThrow(
      'regular directory'
    )

    expect(() =>
      writePluginTransactionJournal({
        pluginsDir,
        transactionRoot: candidate,
        journal: makeJournal('upgrade', 'prepared')
      })
    ).toThrow('direct child')
  })

  it('discards a prepared stage without touching the active upgrade target', () => {
    const target = targetPath()
    const stage = artifactPath('stage')
    createPlugin(target, '1.0.0')
    createPlugin(stage, '2.0.0')
    writeJournal(stage, makeJournal('upgrade', 'prepared'))

    const report = recover()

    expect(pluginVersion(target)).toBe('1.0.0')
    expect(existsSync(stage)).toBe(false)
    expect(report.actions.map((action) => action.type)).toContain('rollback-upgrade')
    expect(report.issues).toEqual([])
  })

  it('promotes a complete pending journal after a crash during the atomic write', () => {
    const target = targetPath()
    const stage = artifactPath('stage')
    createPlugin(target, '1.0.0')
    createPlugin(stage, '2.0.0')
    writeFileSync(
      join(stage, PLUGIN_TRANSACTION_JOURNAL_TEMP_FILENAME),
      JSON.stringify(makeJournal('upgrade', 'prepared'))
    )

    const report = recover()

    expect(pluginVersion(target)).toBe('1.0.0')
    expect(existsSync(stage)).toBe(false)
    expect(report.actions.map((action) => action.type)).toContain('rollback-upgrade')
    expect(report.issues).toEqual([])
  })

  it('removes a prepared install stage after a crash', () => {
    const stage = artifactPath('stage')
    createPlugin(stage, '2.0.0')
    writeJournal(stage, makeJournal('install', 'prepared'))
    metadata.delete('example')

    const report = recover()

    expect(existsSync(stage)).toBe(false)
    expect(report.actions.map((entry) => entry.type)).toEqual(['rollback-install'])
    expect(report.blockedPlugins).toEqual([])
  })

  it('rolls back an applied install target when metadata was not created', () => {
    const stage = artifactPath('stage')
    const target = targetPath()
    createPlugin(stage, '2.0.0')
    writeJournal(stage, makeJournal('install', 'prepared'))
    renameSync(stage, target)
    writeJournal(target, makeJournal('install', 'applied'))
    metadata.delete('example')

    const report = recover()

    expect(existsSync(target)).toBe(false)
    expect(report.actions.map((entry) => entry.type)).toEqual(['rollback-install'])
    expect(report.blockedPlugins).toEqual([])
  })

  it('commits an applied install target when metadata already exists', () => {
    const stage = artifactPath('stage')
    const target = targetPath()
    createPlugin(stage, '2.0.0')
    writeJournal(stage, makeJournal('install', 'prepared'))
    renameSync(stage, target)
    writeJournal(target, makeJournal('install', 'applied'))
    metadata.set('example', newMetadata)

    const report = recover()

    expect(pluginVersion(target)).toBe('2.0.0')
    expect(existsSync(join(target, PLUGIN_TRANSACTION_JOURNAL_FILENAME))).toBe(false)
    expect(report.actions.map((entry) => entry.type)).toEqual(['commit-install'])
    expect(report.blockedPlugins).toEqual([])
  })

  it('recovers an upgrade interrupted between the two directory renames', () => {
    const target = targetPath()
    const stage = artifactPath('stage')
    const backup = artifactPath('backup')
    createPlugin(target, '1.0.0')
    createPlugin(stage, '2.0.0')
    writeJournal(stage, makeJournal('upgrade', 'prepared'))
    renameSync(target, backup)

    const report = recover()

    expect(pluginVersion(target)).toBe('1.0.0')
    expect(existsSync(stage)).toBe(false)
    expect(existsSync(backup)).toBe(false)
    expect(report.issues).toEqual([])
  })

  it('rolls back swapped files and metadata before an upgrade commit', () => {
    const target = targetPath()
    const stage = artifactPath('stage')
    const backup = artifactPath('backup')
    createPlugin(target, '1.0.0')
    createPlugin(stage, '2.0.0')
    writeJournal(stage, makeJournal('upgrade', 'prepared'))
    renameSync(target, backup)
    renameSync(stage, target)
    writeJournal(target, makeJournal('upgrade', 'applied'))
    metadata.set('example', newMetadata)

    const report = recover()

    expect(pluginVersion(target)).toBe('1.0.0')
    expect(metadata.get('example')).toEqual(oldMetadata)
    expect(existsSync(stage)).toBe(false)
    expect(existsSync(backup)).toBe(false)
    expect(existsSync(join(target, PLUGIN_TRANSACTION_JOURNAL_FILENAME))).toBe(false)
    expect(report.issues).toEqual([])
  })

  it('only cleans the backup after an upgrade is committed', () => {
    const target = targetPath()
    const backup = artifactPath('backup')
    createPlugin(target, '2.0.0')
    createPlugin(backup, '1.0.0')
    writeJournal(target, makeJournal('upgrade', 'committed'))
    metadata.set('example', newMetadata)

    const report = recover()

    expect(pluginVersion(target)).toBe('2.0.0')
    expect(metadata.get('example')).toEqual(newMetadata)
    expect(existsSync(backup)).toBe(false)
    expect(existsSync(join(target, PLUGIN_TRANSACTION_JOURNAL_FILENAME))).toBe(false)
    expect(report.actions.map((action) => action.type)).toEqual(['cleanup-committed-upgrade'])
    expect(report.issues).toEqual([])
  })

  it('restores a quarantined uninstall while metadata still exists', () => {
    const target = targetPath()
    const quarantine = artifactPath('remove')
    createPlugin(target, '1.0.0')
    writeJournal(target, makeJournal('uninstall', 'prepared'))
    renameSync(target, quarantine)
    writeJournal(quarantine, makeJournal('uninstall', 'applied'))

    const report = recover()

    expect(pluginVersion(target)).toBe('1.0.0')
    expect(existsSync(quarantine)).toBe(false)
    expect(existsSync(join(target, PLUGIN_TRANSACTION_JOURNAL_FILENAME))).toBe(false)
    expect(report.actions.map((action) => action.type)).toEqual(['restore-uninstall'])
    expect(report.issues).toEqual([])
  })

  it('finishes a quarantined uninstall after metadata deletion', () => {
    const target = targetPath()
    const quarantine = artifactPath('remove')
    createPlugin(target, '1.0.0')
    writeJournal(target, makeJournal('uninstall', 'prepared'))
    renameSync(target, quarantine)
    writeJournal(quarantine, makeJournal('uninstall', 'applied'))
    metadata.delete('example')

    const report = recover()

    expect(existsSync(target)).toBe(false)
    expect(existsSync(quarantine)).toBe(false)
    expect(report.actions.map((action) => action.type)).toEqual(['commit-uninstall'])
    expect(report.issues).toEqual([])
  })

  it('uses metadata lookup to resolve orphan backup, stage, and uninstall directories', () => {
    const backup = artifactPath('backup', 'tx-backup')
    const stage = artifactPath('stage', 'tx-backup')
    createPlugin(backup, '1.0.0')
    createPlugin(stage, '2.0.0')

    const orphanRemove = artifactPath('remove', 'tx-remove', 'deleted-plugin')
    createPlugin(orphanRemove, '1.0.0')

    const report = recover()

    expect(pluginVersion(targetPath())).toBe('1.0.0')
    expect(existsSync(stage)).toBe(false)
    expect(existsSync(orphanRemove)).toBe(false)
    expect(report.actions.map((action) => action.type)).toEqual(
      expect.arrayContaining([
        'restore-orphan-backup',
        'remove-orphan-stage',
        'commit-orphan-uninstall'
      ])
    )
    expect(report.issues).toEqual([])
  })

  it('reports unsafe cleanup without damaging an already restored target', () => {
    const target = targetPath()
    const stage = artifactPath('stage')
    const backup = artifactPath('backup')
    const external = join(testDir, 'external')
    createPlugin(target, '1.0.0')
    createPlugin(stage, '2.0.0')
    mkdirSync(external)
    writeFileSync(join(external, 'keep.txt'), 'external data')
    symlinkSync(external, join(stage, 'linked'), process.platform === 'win32' ? 'junction' : 'dir')
    writeJournal(stage, makeJournal('upgrade', 'prepared'))
    renameSync(target, backup)
    renameSync(stage, target)
    metadata.set('example', newMetadata)

    const report = recover()

    expect(pluginVersion(target)).toBe('1.0.0')
    expect(metadata.get('example')).toEqual(oldMetadata)
    expect(existsSync(stage)).toBe(true)
    expect(readFileSync(join(external, 'keep.txt'), 'utf8')).toBe('external data')
    expect(report.issues.some((issue) => issue.code === 'cleanup-error')).toBe(true)
    expect(report.actions.map((action) => action.type)).toContain('rollback-upgrade')
    expect(report.blockedPlugins).toEqual([])
  })

  it('reports a linked journal marker and leaves the target untouched', () => {
    const target = targetPath()
    const external = join(testDir, 'external-marker')
    createPlugin(target, '1.0.0')
    mkdirSync(external)
    symlinkSync(
      external,
      join(target, PLUGIN_TRANSACTION_JOURNAL_FILENAME),
      process.platform === 'win32' ? 'junction' : 'dir'
    )

    const report = recover()

    expect(pluginVersion(target)).toBe('1.0.0')
    expect(report.actions).toEqual([])
    expect(report.issues).toHaveLength(1)
    expect(report.issues[0].code).toBe('invalid-journal')
    expect(report.blockedPlugins).toEqual(['example'])
  })

  it('rejects a linked transaction root without following or deleting it', () => {
    const external = join(testDir, 'external-root')
    const linkedStage = artifactPath('stage')
    createPlugin(external, '2.0.0')
    symlinkSync(external, linkedStage, process.platform === 'win32' ? 'junction' : 'dir')

    const report = recover()

    expect(pluginVersion(external)).toBe('2.0.0')
    expect(existsSync(linkedStage)).toBe(true)
    expect(report.actions).toEqual([])
    expect(report.issues).toHaveLength(1)
    expect(report.issues[0].code).toBe('unsafe-entry')
    expect(report.blockedPlugins).toEqual(['example'])
  })
})

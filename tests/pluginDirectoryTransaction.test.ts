import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
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
  PluginDirectoryRemovalTransaction,
  PluginDirectoryTransaction
} from '../plugin-system/PluginDirectoryTransaction'

describe('PluginDirectoryTransaction', () => {
  let testDir: string
  let pluginsDir: string
  let sourceDir: string

  beforeEach(() => {
    testDir = mkdtempSync(join(tmpdir(), 'openbox-plugin-transaction-'))
    pluginsDir = join(testDir, 'plugins')
    sourceDir = join(testDir, 'candidate')
    mkdirSync(pluginsDir)
    mkdirSync(sourceDir)
    writeFileSync(join(sourceDir, 'plugin.json'), '{"version":"2.0.0"}')
    mkdirSync(join(sourceDir, 'dist'))
    writeFileSync(join(sourceDir, 'dist', 'main.js'), 'module.exports = "new"')
    writeFileSync(join(sourceDir, 'dist', 'renderer.js'), 'module.exports = "renderer"')
  })

  afterEach(() => {
    const resolved = resolve(testDir)
    const tempRoot = resolve(tmpdir())
    if (resolved.startsWith(`${tempRoot}\\`) || resolved.startsWith(`${tempRoot}/`)) {
      rmSync(resolved, { force: true, recursive: true })
    }
  })

  it('stages without touching the active plugin and can roll back a swap', () => {
    const targetDir = join(pluginsDir, 'example')
    mkdirSync(targetDir)
    writeFileSync(join(targetDir, 'plugin.json'), '{"version":"1.0.0"}')

    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'example',
      sourceDir,
      transactionId: 'rollback-test'
    })
    transaction.stage()
    expect(readFileSync(join(targetDir, 'plugin.json'), 'utf8')).toContain('1.0.0')

    transaction.swap()
    expect(readFileSync(join(targetDir, 'plugin.json'), 'utf8')).toContain('2.0.0')
    expect(existsSync(transaction.backupDir)).toBe(true)

    transaction.rollback()
    expect(transaction.phase).toBe('rolled-back')
    expect(readFileSync(join(targetDir, 'plugin.json'), 'utf8')).toContain('1.0.0')
    expect(existsSync(transaction.backupDir)).toBe(false)
    expect(existsSync(transaction.stageDir)).toBe(false)
  })

  it('commits a same-volume swap and removes the backup', () => {
    const targetDir = join(pluginsDir, 'example')
    mkdirSync(targetDir)
    writeFileSync(join(targetDir, 'old.txt'), 'old')
    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'example',
      sourceDir,
      transactionId: 'commit-test'
    })

    transaction.stage()
    transaction.swap()
    transaction.commit()

    expect(transaction.phase).toBe('committed')
    expect(readFileSync(join(targetDir, 'dist', 'main.js'), 'utf8')).toContain('new')
    expect(existsSync(join(targetDir, 'old.txt'))).toBe(false)
    expect(existsSync(transaction.backupDir)).toBe(false)
  })

  it('removes a newly installed target when rolling back', () => {
    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'new-plugin',
      sourceDir,
      transactionId: 'new-install-test'
    })
    transaction.stage()
    transaction.swap()
    expect(existsSync(transaction.targetDir)).toBe(true)

    transaction.rollback()
    expect(existsSync(transaction.targetDir)).toBe(false)
    expect(existsSync(transaction.stageDir)).toBe(false)
  })

  it('enforces entry and byte budgets before swapping', () => {
    const entryLimited = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'entry-limited',
      sourceDir,
      transactionId: 'entry-budget',
      maxEntries: 1
    })
    expect(() => entryLimited.stage()).toThrow('entries')
    expect(existsSync(entryLimited.stageDir)).toBe(false)

    const byteLimited = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'byte-limited',
      sourceDir,
      transactionId: 'byte-budget',
      maxTotalBytes: 1
    })
    expect(() => byteLimited.stage()).toThrow('bytes')
    expect(existsSync(byteLimited.stageDir)).toBe(false)
  })

  it('refuses invalid names and transaction path collisions', () => {
    expect(
      () =>
        new PluginDirectoryTransaction({
          pluginsDir,
          pluginName: '../escape',
          sourceDir
        })
    ).toThrow('pluginName')

    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'example',
      sourceDir,
      transactionId: 'collision'
    })
    mkdirSync(transaction.stageDir)
    expect(() => transaction.stage()).toThrow('already exists')
  })

  it('rejects linked source roots and linked entries instead of following them', () => {
    const linkedRoot = join(testDir, 'linked-candidate')
    symlinkSync(sourceDir, linkedRoot, process.platform === 'win32' ? 'junction' : 'dir')
    expect(
      () =>
        new PluginDirectoryTransaction({
          pluginsDir,
          pluginName: 'linked-root',
          sourceDir: linkedRoot
        })
    ).toThrow('regular directory')

    const externalDirectory = join(testDir, 'external')
    mkdirSync(externalDirectory)
    writeFileSync(join(externalDirectory, 'secret.txt'), 'must not be copied')
    symlinkSync(
      externalDirectory,
      join(sourceDir, 'linked-entry'),
      process.platform === 'win32' ? 'junction' : 'dir'
    )
    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'linked-entry',
      sourceDir,
      transactionId: 'linked-entry-test'
    })
    expect(() => transaction.stage()).toThrow('symbolic link')
    expect(existsSync(transaction.stageDir)).toBe(false)
  })

  it('enforces the expected target state and recovers an interrupted first rename', () => {
    const targetDir = join(pluginsDir, 'example')
    mkdirSync(targetDir)
    writeFileSync(join(targetDir, 'old.txt'), 'old')

    const absent = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'example',
      sourceDir,
      transactionId: 'expected-absent',
      expectedTargetExists: false
    })
    expect(() => absent.stage()).toThrow('expected to be absent')
    expect(existsSync(absent.stageDir)).toBe(false)

    const interrupted = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'example',
      sourceDir,
      transactionId: 'interrupted-swap',
      expectedTargetExists: true
    })
    interrupted.stage()
    renameSync(interrupted.targetDir, interrupted.backupDir)
    interrupted.rollback()
    expect(readFileSync(join(targetDir, 'old.txt'), 'utf8')).toBe('old')
    expect(existsSync(interrupted.stageDir)).toBe(false)
    expect(existsSync(interrupted.backupDir)).toBe(false)
  })

  it('quarantines removals so metadata failures can restore the target', () => {
    const targetDir = join(pluginsDir, 'example')
    mkdirSync(targetDir)
    writeFileSync(join(targetDir, 'user-data.txt'), 'preserve me')

    const rollback = new PluginDirectoryRemovalTransaction({
      pluginsDir,
      pluginName: 'example',
      transactionId: 'remove-rollback'
    })
    rollback.quarantine()
    expect(existsSync(targetDir)).toBe(false)
    expect(readFileSync(join(rollback.quarantineDir, 'user-data.txt'), 'utf8')).toBe('preserve me')
    rollback.rollback()
    expect(readFileSync(join(targetDir, 'user-data.txt'), 'utf8')).toBe('preserve me')

    const commit = new PluginDirectoryRemovalTransaction({
      pluginsDir,
      pluginName: 'example',
      transactionId: 'remove-commit'
    })
    commit.quarantine()
    commit.commit()
    expect(commit.phase).toBe('committed')
    expect(existsSync(targetDir)).toBe(false)
    expect(existsSync(commit.quarantineDir)).toBe(false)
  })

  it('stages only the allowed files for a restricted copy', () => {
    mkdirSync(join(sourceDir, 'src'))
    writeFileSync(join(sourceDir, 'src', 'deep.js'), 'must not enter staging')
    writeFileSync(join(sourceDir, 'version.txt'), 'extra top-level file')
    writeFileSync(join(sourceDir, 'dist', 'extra.js'), 'extra dist file')

    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'restricted',
      sourceDir,
      transactionId: 'restricted-copy',
      allowedFiles: ['plugin.json', 'dist/main.js', 'dist/renderer.js']
    })
    transaction.stage()

    expect(readdirSync(transaction.stageDir).sort()).toEqual(['dist', 'plugin.json'])
    expect(readdirSync(join(transaction.stageDir, 'dist')).sort()).toEqual([
      'main.js',
      'renderer.js'
    ])
    expect(existsSync(join(transaction.stageDir, 'version.txt'))).toBe(false)
    expect(existsSync(join(transaction.stageDir, 'src'))).toBe(false)
    expect(existsSync(join(transaction.stageDir, 'dist', 'extra.js'))).toBe(false)
  })

  it('rejects a missing allowed file and cleans the staging directory', () => {
    rmSync(join(sourceDir, 'dist', 'main.js'))
    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'missing-file',
      sourceDir,
      transactionId: 'restricted-missing',
      allowedFiles: ['plugin.json', 'dist/main.js', 'dist/renderer.js']
    })
    expect(() => transaction.stage()).toThrow('missing required runtime file')
    expect(existsSync(transaction.stageDir)).toBe(false)
  })

  it('rejects an allowed path that resolves to a directory', () => {
    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'directory-allowed',
      sourceDir,
      transactionId: 'restricted-directory',
      allowedFiles: ['plugin.json', 'dist']
    })
    expect(() => transaction.stage()).toThrow('not a regular file')
    expect(existsSync(transaction.stageDir)).toBe(false)
  })

  it('rejects symbolic links at an allowed path', () => {
    const linkedDist = join(testDir, 'linked-dist')
    mkdirSync(linkedDist)
    writeFileSync(join(linkedDist, 'renderer.js'), 'must not be staged')
    const originalDist = join(sourceDir, 'dist')
    renameSync(originalDist, join(testDir, 'real-dist'))
    symlinkSync(linkedDist, originalDist, process.platform === 'win32' ? 'junction' : 'dir')
    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'linked-parent',
      sourceDir,
      transactionId: 'restricted-parent-link',
      allowedFiles: ['plugin.json', 'dist/main.js', 'dist/renderer.js']
    })
    expect(() => transaction.stage()).toThrow('symbolic link')
    expect(existsSync(transaction.stageDir)).toBe(false)
  })

  it('rejects traversal and absolute paths in the allowed files', () => {
    for (const badPath of [
      '../escape',
      '/absolute',
      'C:\\absolute',
      'dist//main.js',
      'dist/./main.js'
    ]) {
      expect(
        () =>
          new PluginDirectoryTransaction({
            pluginsDir,
            pluginName: 'unsafe-allowlist',
            sourceDir,
            allowedFiles: ['plugin.json', badPath]
          })
      ).toThrow('unsafe file path')
    }
  })

  it('copies extra files for ordinary plugins when no allowlist is set', () => {
    mkdirSync(join(sourceDir, 'src'))
    writeFileSync(join(sourceDir, 'src', 'deep.js'), 'deep content')
    writeFileSync(join(sourceDir, 'version.txt'), 'extra top-level file')
    writeFileSync(join(sourceDir, 'dist', 'extra.js'), 'extra dist file')

    const transaction = new PluginDirectoryTransaction({
      pluginsDir,
      pluginName: 'ordinary',
      sourceDir,
      transactionId: 'ordinary-copy'
    })
    transaction.stage()

    expect(readFileSync(join(transaction.stageDir, 'version.txt'), 'utf8')).toBe(
      'extra top-level file'
    )
    expect(readFileSync(join(transaction.stageDir, 'src', 'deep.js'), 'utf8')).toBe('deep content')
    expect(readFileSync(join(transaction.stageDir, 'dist', 'extra.js'), 'utf8')).toBe(
      'extra dist file'
    )
  })
})

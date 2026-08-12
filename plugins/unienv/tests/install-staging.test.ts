import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  cleanupInstallStagingDir,
  createInstallStagingDir,
  prepareDirectInstallDirectory,
  promoteStagedRuntime,
  recoverInterruptedInstallStaging
} from '../src/tools/base'

describe('install staging directories', () => {
  let testDir: string
  let versionRoot: string

  beforeEach(() => {
    testDir = mkdtempSync(join(tmpdir(), 'unienv-staging-test-'))
    versionRoot = join(testDir, 'version')
    mkdirSync(versionRoot)
  })

  afterEach(() => {
    rmSync(testDir, { force: true, recursive: true })
  })

  it('atomically promotes a runtime and only removes its staging directory', () => {
    const stagingDir = createInstallStagingDir(versionRoot)
    const sourceDir = join(stagingDir, 'extracted', 'runtime')
    const finalDir = join(versionRoot, 'runtime')
    mkdirSync(sourceDir, { recursive: true })
    writeFileSync(join(sourceDir, 'node.exe'), 'runtime')
    writeFileSync(join(versionRoot, 'user-note.txt'), 'preserve')

    promoteStagedRuntime(versionRoot, stagingDir, sourceDir, finalDir)
    cleanupInstallStagingDir(versionRoot, stagingDir)

    expect(readFileSync(join(finalDir, 'node.exe'), 'utf8')).toBe('runtime')
    expect(readFileSync(join(versionRoot, 'user-note.txt'), 'utf8')).toBe('preserve')
    expect(existsSync(stagingDir)).toBe(false)
  })

  it('refuses to overwrite an existing runtime and preserves both copies', () => {
    const finalDir = join(versionRoot, 'runtime')
    mkdirSync(finalDir)
    writeFileSync(join(finalDir, 'existing.txt'), 'existing')
    const stagingDir = createInstallStagingDir(versionRoot)
    const sourceDir = join(stagingDir, 'extracted', 'runtime')
    mkdirSync(sourceDir, { recursive: true })
    writeFileSync(join(sourceDir, 'new.txt'), 'new')

    expect(() => promoteStagedRuntime(versionRoot, stagingDir, sourceDir, finalDir)).toThrow(
      '拒绝覆盖'
    )
    expect(readFileSync(join(finalDir, 'existing.txt'), 'utf8')).toBe('existing')
    expect(readFileSync(join(sourceDir, 'new.txt'), 'utf8')).toBe('new')

    cleanupInstallStagingDir(versionRoot, stagingDir)
    expect(readFileSync(join(finalDir, 'existing.txt'), 'utf8')).toBe('existing')
  })

  it('refuses recursive cleanup outside the direct staging boundary', () => {
    const sibling = join(testDir, '.unienv-staging-do-not-delete')
    mkdirSync(sibling)
    writeFileSync(join(sibling, 'keep.txt'), 'keep')

    expect(() => cleanupInstallStagingDir(versionRoot, sibling)).toThrow('version 目录外')
    expect(readFileSync(join(sibling, 'keep.txt'), 'utf8')).toBe('keep')
  })

  it('creates an absent direct-install target but refuses to overwrite existing content', () => {
    const freshTarget = join(testDir, 'python-fresh')
    prepareDirectInstallDirectory(freshTarget, 'Python')
    expect(existsSync(freshTarget)).toBe(true)

    const existingTarget = join(testDir, 'python-existing')
    mkdirSync(existingTarget)
    writeFileSync(join(existingTarget, 'python.exe'), 'preserve')
    expect(() => prepareDirectInstallDirectory(existingTarget, 'Python')).toThrow('拒绝覆盖')
    expect(readFileSync(join(existingTarget, 'python.exe'), 'utf8')).toBe('preserve')
  })

  it('removes interrupted staging directories and preserves installed runtimes', () => {
    const secondVersionRoot = join(testDir, 'second-version')
    mkdirSync(secondVersionRoot)
    const firstStaging = createInstallStagingDir(versionRoot)
    const secondStaging = createInstallStagingDir(secondVersionRoot)
    mkdirSync(join(versionRoot, 'runtime'))
    writeFileSync(join(versionRoot, 'runtime', 'python.exe'), 'installed')
    writeFileSync(join(firstStaging, 'partial.zip'), 'partial')
    writeFileSync(join(secondStaging, 'partial.zip'), 'partial')

    const removed = recoverInterruptedInstallStaging([versionRoot, secondVersionRoot])

    expect(removed).toEqual([firstStaging, secondStaging])
    expect(existsSync(firstStaging)).toBe(false)
    expect(existsSync(secondStaging)).toBe(false)
    expect(readFileSync(join(versionRoot, 'runtime', 'python.exe'), 'utf8')).toBe('installed')
  })
})

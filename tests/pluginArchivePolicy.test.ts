import { existsSync, mkdirSync, mkdtempSync, rmSync, truncateSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import AdmZip from 'adm-zip'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  MAX_PLUGIN_ARCHIVE_BYTES,
  MAX_PLUGIN_ZIP_BYTES,
  extractPluginArchive,
  normalizeArchiveEntryPath,
  validatePluginArchive
} from '../plugin-system/PluginArchivePolicy'

describe('plugin archive policy', () => {
  let testDir: string

  beforeEach(() => {
    testDir = mkdtempSync(join(tmpdir(), 'openbox-archive-policy-'))
  })

  afterEach(() => {
    const resolved = resolve(testDir)
    const tempRoot = resolve(tmpdir())
    if (resolved.startsWith(`${tempRoot}\\`) || resolved.startsWith(`${tempRoot}/`)) {
      rmSync(resolved, { force: true, recursive: true })
    }
  })

  it('normalizes portable entry paths and rejects traversal or Windows aliases', () => {
    expect(normalizeArchiveEntryPath('dist/main.js')).toBe('dist/main.js')
    expect(normalizeArchiveEntryPath('dist\\main.js')).toBe('dist/main.js')
    for (const value of [
      '../outside',
      '/absolute',
      'C:\\absolute',
      'dist//main.js',
      'dist/./main.js',
      'CON/file.js',
      'file.js. ',
      'file.js:stream'
    ]) {
      expect(() => normalizeArchiveEntryPath(value)).toThrow('unsafe path')
    }
  })

  it('rejects symbolic-link entries and impossible uncompressed sizes', () => {
    const linked = new AdmZip()
    const linkedEntry = linked.addFile('linked', Buffer.from('target'))
    linkedEntry.header.attr = (0o120777 << 16) >>> 0
    expect(() => validatePluginArchive(linked)).toThrow('symbolic link')

    const oversized = new AdmZip()
    const oversizedEntry = oversized.addFile('large.bin', Buffer.alloc(0))
    oversizedEntry.header.size = MAX_PLUGIN_ARCHIVE_BYTES + 1
    expect(() => validatePluginArchive(oversized)).toThrow('byte budget')
  })

  it('checks compressed size before parsing and extracts a valid archive', () => {
    const oversizedPath = join(testDir, 'oversized.zip')
    writeFileSync(oversizedPath, '')
    truncateSync(oversizedPath, MAX_PLUGIN_ZIP_BYTES + 1)
    expect(() => extractPluginArchive(oversizedPath, join(testDir, 'unused'))).toThrow('too large')

    const archivePath = join(testDir, 'plugin.zip')
    const destination = join(testDir, 'destination')
    const archive = new AdmZip()
    archive.addFile('plugin.json', Buffer.from('{}'))
    archive.addFile('dist/main.js', Buffer.from('module.exports = {}'))
    archive.writeZip(archivePath)
    mkdirSync(destination)
    extractPluginArchive(archivePath, destination)
    expect(existsSync(join(destination, 'plugin.json'))).toBe(true)
    expect(existsSync(join(destination, 'dist', 'main.js'))).toBe(true)
  })
})

import { closeSync, fstatSync, lstatSync, openSync, readSync } from 'node:fs'
import { posix, win32 } from 'node:path'
import AdmZip from 'adm-zip'

export const MAX_PLUGIN_ZIP_BYTES = 200 * 1024 * 1024
export const MAX_PLUGIN_ARCHIVE_ENTRIES = 5_000
export const MAX_PLUGIN_ARCHIVE_BYTES = 600 * 1024 * 1024

const WINDOWS_RESERVED_BASENAME = /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])$/i

function containsControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code <= 0x1f || code === 0x7f) return true
  }
  return false
}

export function normalizeArchiveEntryPath(value: string): string {
  const portable = value.replaceAll('\\', '/')
  const withoutTrailingSlash = portable.endsWith('/') ? portable.slice(0, -1) : portable
  const segments = withoutTrailingSlash.split('/')
  if (
    withoutTrailingSlash.length === 0 ||
    withoutTrailingSlash.length > 512 ||
    portable.startsWith('/') ||
    win32.isAbsolute(value) ||
    containsControlCharacter(value) ||
    segments.some(
      (segment) =>
        segment === '' ||
        segment === '.' ||
        segment === '..' ||
        segment.endsWith('.') ||
        segment.endsWith(' ') ||
        /[<>:"|?*]/.test(segment) ||
        WINDOWS_RESERVED_BASENAME.test(segment.split('.')[0] ?? '')
    ) ||
    posix.normalize(withoutTrailingSlash) !== withoutTrailingSlash
  ) {
    throw new Error(`Plugin archive contains an unsafe path: ${value}`)
  }
  return withoutTrailingSlash
}

export function validatePluginArchive(zip: AdmZip): void {
  const entries = zip.getEntries()
  if (entries.length === 0 || entries.length > MAX_PLUGIN_ARCHIVE_ENTRIES) {
    throw new Error('Plugin archive has an invalid entry count')
  }
  let total = 0
  const paths = new Set<string>()
  for (const entry of entries) {
    const normalized = normalizeArchiveEntryPath(entry.entryName)
    normalizeArchiveEntryPath(entry.rawEntryName.toString('utf8'))
    const pathKey = normalized.toLocaleLowerCase('en-US')
    if (paths.has(pathKey)) {
      throw new Error(`Plugin archive contains duplicate paths: ${normalized}`)
    }
    paths.add(pathKey)

    const isSymbolicLink = [entry.attr, entry.header.attr].some(
      (attributes) => ((attributes >>> 16) & 0o170000) === 0o120000
    )
    if (isSymbolicLink) {
      throw new Error(`Plugin archive contains a symbolic link: ${normalized}`)
    }
    if (entry.header.encrypted) throw new Error('Encrypted plugin archives are not supported')
    const size = entry.header.size
    if (!Number.isSafeInteger(size) || size < 0 || total > MAX_PLUGIN_ARCHIVE_BYTES - size) {
      throw new Error('Plugin archive exceeds its uncompressed byte budget')
    }
    total += size
  }
}

export function extractPluginArchive(zipPath: string, destination: string): void {
  const stats = lstatSync(zipPath)
  if (!stats.isFile() || stats.isSymbolicLink()) {
    throw new Error('Plugin package must be a regular ZIP file')
  }
  if (stats.size > MAX_PLUGIN_ZIP_BYTES) throw new Error('Plugin package is too large')
  const descriptor = openSync(zipPath, 'r')
  let bytes: Buffer
  try {
    const opened = fstatSync(descriptor)
    if (
      !opened.isFile() ||
      opened.dev !== stats.dev ||
      opened.ino !== stats.ino ||
      opened.size !== stats.size ||
      opened.size > MAX_PLUGIN_ZIP_BYTES
    ) {
      throw new Error('Plugin package changed while it was being opened')
    }
    bytes = Buffer.allocUnsafe(opened.size)
    let offset = 0
    while (offset < bytes.length) {
      const bytesRead = readSync(descriptor, bytes, offset, bytes.length - offset, null)
      if (bytesRead === 0) throw new Error('Plugin package was truncated while it was being read')
      offset += bytesRead
    }
    const after = fstatSync(descriptor)
    if (
      after.size !== opened.size ||
      after.mtimeMs !== opened.mtimeMs ||
      after.ctimeMs !== opened.ctimeMs
    ) {
      throw new Error('Plugin package changed while it was being read')
    }
  } finally {
    closeSync(descriptor)
  }
  const zip = new AdmZip(bytes)
  validatePluginArchive(zip)
  zip.extractAllTo(destination, true)
}

import { closeSync, fstatSync, lstatSync, openSync, readSync, realpathSync } from 'node:fs'
import { isAbsolute, join, posix, relative, resolve, sep, win32 } from 'node:path'
import type { ConfigField, PluginManifest } from '@shared/types/plugin.types'
import { ALL_PERMISSIONS, type Permission } from '@shared/types/permissions'
import { parseSemVer } from './semver'

export const MAX_PLUGIN_MANIFEST_BYTES = 256 * 1024

const PLUGIN_NAME_PATTERN = /^[a-z0-9][a-z0-9_-]{0,63}$/
const CONFIG_KEY_PATTERN = /^[a-zA-Z][a-zA-Z0-9_.-]{0,63}$/
const FORBIDDEN_KEYS = new Set(['__proto__', 'prototype', 'constructor'])
const CONFIG_FIELD_TYPES = new Set<ConfigField['type']>([
  'string',
  'number',
  'boolean',
  'select',
  'multiselect'
])

export class PluginManifestValidationError extends Error {
  readonly path: string

  constructor(path: string, message: string) {
    super(`${path}: ${message}`)
    this.name = 'PluginManifestValidationError'
    this.path = path
  }
}

function fail(path: string, message: string): never {
  throw new PluginManifestValidationError(path, message)
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

function readRecord(value: unknown, path: string): Record<string, unknown> {
  if (!isPlainRecord(value)) fail(path, 'must be a plain object')
  return value
}

function assertKeys(
  record: Record<string, unknown>,
  allowed: readonly string[],
  path: string
): void {
  const allowedKeys = new Set(allowed)
  for (const key of Reflect.ownKeys(record)) {
    if (typeof key !== 'string' || FORBIDDEN_KEYS.has(key) || !allowedKeys.has(key)) {
      fail(`${path}.${String(key)}`, 'is not a supported field')
    }
  }
}

function containsControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code <= 0x1f || code === 0x7f) return true
  }
  return false
}

function readString(
  value: unknown,
  path: string,
  maxLength: number,
  options: { allowEmpty?: boolean; pattern?: RegExp } = {}
): string {
  if (typeof value !== 'string') fail(path, 'must be a string')
  if ((!options.allowEmpty && value.length === 0) || value.length > maxLength) {
    fail(path, `must contain ${options.allowEmpty ? '0' : '1'}-${maxLength} characters`)
  }
  if (containsControlCharacter(value) || value !== value.trim()) {
    fail(path, 'contains control characters or surrounding whitespace')
  }
  if (options.pattern && !options.pattern.test(value)) fail(path, 'has an invalid format')
  return value
}

function parsePermissions(value: unknown): Permission[] {
  if (!Array.isArray(value) || value.length > ALL_PERMISSIONS.length) {
    fail('manifest.permissions', 'must be a bounded array')
  }
  const known = new Set<string>(ALL_PERMISSIONS)
  const seen = new Set<Permission>()
  const permissions: Permission[] = []
  for (let index = 0; index < value.length; index += 1) {
    const permission = readString(value[index], `manifest.permissions[${index}]`, 64)
    if (!known.has(permission)) fail(`manifest.permissions[${index}]`, 'is unknown')
    if (seen.has(permission as Permission)) fail(`manifest.permissions[${index}]`, 'is duplicated')
    seen.add(permission as Permission)
    permissions.push(permission as Permission)
  }
  return permissions
}

function parseOption(value: unknown, path: string): { label: string; value: string } {
  const option = readRecord(value, path)
  assertKeys(option, ['label', 'value'], path)
  return {
    label: readString(option.label, `${path}.label`, 100),
    value: readString(option.value, `${path}.value`, 200)
  }
}

function assertDefaultValue(type: ConfigField['type'], value: unknown, path: string): void {
  const valid =
    (type === 'number' && typeof value === 'number' && Number.isFinite(value)) ||
    (type === 'boolean' && typeof value === 'boolean') ||
    ((type === 'string' || type === 'select') && typeof value === 'string') ||
    (type === 'multiselect' &&
      Array.isArray(value) &&
      value.length <= 100 &&
      value.every((entry) => typeof entry === 'string'))
  if (!valid) fail(path, `does not match config field type ${type}`)
}

function parseConfigField(value: unknown, path: string): ConfigField {
  const field = readRecord(value, path)
  assertKeys(field, ['type', 'label', 'description', 'default', 'required', 'options'], path)
  const type = readString(field.type, `${path}.type`, 32) as ConfigField['type']
  if (!CONFIG_FIELD_TYPES.has(type)) fail(`${path}.type`, 'is unsupported')
  const label = readString(field.label, `${path}.label`, 100)
  const description =
    field.description === undefined
      ? undefined
      : readString(field.description, `${path}.description`, 500, { allowEmpty: true })
  const required = field.required === undefined ? undefined : field.required
  if (required !== undefined && typeof required !== 'boolean') {
    fail(`${path}.required`, 'must be a boolean')
  }

  let options: { label: string; value: string }[] | undefined
  if (field.options !== undefined) {
    if (
      (type !== 'select' && type !== 'multiselect') ||
      !Array.isArray(field.options) ||
      field.options.length === 0 ||
      field.options.length > 100
    ) {
      fail(`${path}.options`, 'must contain 1-100 options for a select field')
    }
    options = field.options.map((option, index) => parseOption(option, `${path}.options[${index}]`))
    if (new Set(options.map((option) => option.value)).size !== options.length) {
      fail(`${path}.options`, 'contains duplicate values')
    }
  }
  if (field.default !== undefined) assertDefaultValue(type, field.default, `${path}.default`)

  return {
    type,
    label,
    ...(description !== undefined ? { description } : {}),
    ...(field.default !== undefined ? { default: field.default } : {}),
    ...(required !== undefined ? { required } : {}),
    ...(options ? { options } : {})
  }
}

function parseConfig(value: unknown): Record<string, ConfigField> {
  if (value === undefined) return {}
  const config = readRecord(value, 'manifest.config')
  if (Reflect.ownKeys(config).length > 100) fail('manifest.config', 'contains too many fields')
  const result: Record<string, ConfigField> = {}
  for (const key of Reflect.ownKeys(config)) {
    if (typeof key !== 'string' || FORBIDDEN_KEYS.has(key) || !CONFIG_KEY_PATTERN.test(key)) {
      fail(`manifest.config.${String(key)}`, 'has an invalid key')
    }
    result[key] = parseConfigField(config[key], `manifest.config.${key}`)
  }
  return result
}

export function normalizePluginEntry(value: unknown, path: string): string {
  const entry = readString(value, path, 240)
  const portable = entry.replaceAll('\\', '/')
  if (
    isAbsolute(entry) ||
    win32.isAbsolute(entry) ||
    portable.startsWith('/') ||
    portable.split('/').some((segment) => segment === '' || segment === '.' || segment === '..')
  ) {
    fail(path, 'must be a normalized relative path')
  }
  const normalized = posix.normalize(portable)
  if (normalized !== portable || !normalized.endsWith('.js') || normalized.includes(':')) {
    fail(path, 'must be a normalized relative JavaScript file')
  }
  return normalized
}

export function parsePluginManifest(value: unknown): PluginManifest {
  const manifest = readRecord(value, 'manifest')
  assertKeys(
    manifest,
    [
      'name',
      'version',
      'displayName',
      'description',
      'author',
      'icon',
      'main',
      'renderer',
      'backend',
      'manifestVersion',
      'backendApiVersion',
      'rendererApiVersion',
      'permissions',
      'config'
    ],
    'manifest'
  )

  const name = readString(manifest.name, 'manifest.name', 64, { pattern: PLUGIN_NAME_PATTERN })
  const version = readString(manifest.version, 'manifest.version', 100)
  parseSemVer(version)
  const manifestVersion = manifest.manifestVersion
  if (manifestVersion !== undefined && manifestVersion !== 1 && manifestVersion !== 2) {
    fail('manifest.manifestVersion', 'must be 1 or 2')
  }
  const backendApiVersion = manifest.backendApiVersion
  if (backendApiVersion !== undefined && backendApiVersion !== 1 && backendApiVersion !== 2) {
    fail('manifest.backendApiVersion', 'must be 1 or 2')
  }
  const rendererApiVersion = manifest.rendererApiVersion
  if (rendererApiVersion !== undefined && rendererApiVersion !== 1 && rendererApiVersion !== 2) {
    fail('manifest.rendererApiVersion', 'must be 1 or 2')
  }
  const backend = manifest.backend
  if (backend !== undefined && typeof backend !== 'boolean') {
    fail('manifest.backend', 'must be a boolean')
  }
  if (
    manifestVersion === 2 &&
    (rendererApiVersion !== 2 || (backend !== false && backendApiVersion !== 2))
  ) {
    fail(
      'manifest',
      'version 2 requires rendererApiVersion 2 and backendApiVersion 2 when backend is enabled'
    )
  }
  return {
    ...(manifestVersion === undefined ? {} : { manifestVersion }),
    name,
    version,
    displayName: readString(manifest.displayName, 'manifest.displayName', 100),
    description:
      manifest.description === undefined
        ? ''
        : readString(manifest.description, 'manifest.description', 2_000, { allowEmpty: true }),
    author:
      manifest.author === undefined
        ? ''
        : readString(manifest.author, 'manifest.author', 200, { allowEmpty: true }),
    ...(manifest.icon === undefined
      ? {}
      : { icon: readString(manifest.icon, 'manifest.icon', 512, { allowEmpty: true }) }),
    main: normalizePluginEntry(manifest.main, 'manifest.main'),
    renderer: normalizePluginEntry(manifest.renderer, 'manifest.renderer'),
    ...(backend === undefined ? {} : { backend }),
    ...(backendApiVersion === undefined ? {} : { backendApiVersion }),
    ...(rendererApiVersion === undefined ? {} : { rendererApiVersion }),
    permissions: parsePermissions(manifest.permissions),
    config: parseConfig(manifest.config)
  }
}

export function assertPluginManifestInstallable(
  manifest: PluginManifest,
  allowLegacyFullTrust = false
): void {
  if ((manifest.manifestVersion ?? 1) === 2 || allowLegacyFullTrust) return
  throw new PluginManifestValidationError(
    'manifest.manifestVersion',
    'legacy v1 packages can no longer be installed; migrate this plugin to Manifest v2'
  )
}

export function readPluginManifest(pluginRoot: string): PluginManifest {
  let rootStats
  try {
    rootStats = lstatSync(pluginRoot)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      fail('manifest', 'plugin root does not exist')
    }
    throw error
  }
  if (!rootStats.isDirectory() || rootStats.isSymbolicLink()) {
    fail('manifest', 'plugin root must be a regular directory')
  }
  const manifestPath = join(pluginRoot, 'plugin.json')
  let stats
  try {
    stats = lstatSync(manifestPath)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      fail('manifest', 'plugin.json does not exist')
    }
    throw error
  }
  if (!stats.isFile() || stats.isSymbolicLink()) {
    fail('manifest', 'plugin.json must be a regular file')
  }
  if (stats.size > MAX_PLUGIN_MANIFEST_BYTES) {
    fail('manifest', `plugin.json exceeds ${MAX_PLUGIN_MANIFEST_BYTES} bytes`)
  }
  const fileDescriptor = openSync(manifestPath, 'r')
  let manifestText: string
  try {
    const opened = fstatSync(fileDescriptor)
    if (
      !opened.isFile() ||
      opened.dev !== stats.dev ||
      opened.ino !== stats.ino ||
      opened.size !== stats.size ||
      opened.size > MAX_PLUGIN_MANIFEST_BYTES
    ) {
      fail('manifest', 'plugin.json changed while it was being read')
    }
    const bytes = Buffer.alloc(opened.size)
    let offset = 0
    while (offset < bytes.length) {
      const bytesRead = readSync(fileDescriptor, bytes, offset, bytes.length - offset, null)
      if (bytesRead === 0) fail('manifest', 'plugin.json was truncated while it was being read')
      offset += bytesRead
    }
    const after = fstatSync(fileDescriptor)
    if (
      after.size !== opened.size ||
      after.mtimeMs !== opened.mtimeMs ||
      after.ctimeMs !== opened.ctimeMs
    ) {
      fail('manifest', 'plugin.json changed while it was being read')
    }
    manifestText = bytes.toString('utf8')
  } finally {
    closeSync(fileDescriptor)
  }
  let decoded: unknown
  try {
    decoded = JSON.parse(manifestText) as unknown
  } catch {
    fail('manifest', 'plugin.json is not valid JSON')
  }
  return parsePluginManifest(decoded)
}

export function resolvePluginEntrypoint(pluginRoot: string, entry: string): string {
  const rootStats = lstatSync(pluginRoot)
  if (!rootStats.isDirectory() || rootStats.isSymbolicLink()) {
    fail('manifest.entry', 'plugin root must be a regular directory')
  }
  const canonicalRoot = realpathSync(pluginRoot)
  const candidate = resolve(canonicalRoot, ...entry.split('/'))
  const candidateRelative = relative(canonicalRoot, candidate)
  if (
    candidateRelative === '' ||
    candidateRelative === '..' ||
    candidateRelative.startsWith(`..${sep}`) ||
    isAbsolute(candidateRelative)
  ) {
    fail('manifest.entry', 'resolved outside the plugin root')
  }
  let stats
  try {
    stats = lstatSync(candidate)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      fail('manifest.entry', `does not exist: ${entry}`)
    }
    throw error
  }
  if (!stats.isFile() || stats.isSymbolicLink()) {
    fail('manifest.entry', `is not a regular file: ${entry}`)
  }
  const canonicalCandidate = realpathSync(candidate)
  const realRelative = relative(canonicalRoot, canonicalCandidate)
  if (realRelative === '..' || realRelative.startsWith(`..${sep}`) || isAbsolute(realRelative)) {
    fail('manifest.entry', `resolved through a link outside the plugin root: ${entry}`)
  }
  return candidate
}

export function validatePluginEntrypoints(pluginRoot: string, manifest: PluginManifest): void {
  resolvePluginEntrypoint(pluginRoot, manifest.main)
  resolvePluginEntrypoint(pluginRoot, manifest.renderer)
}

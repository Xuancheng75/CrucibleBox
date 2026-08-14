// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
export const SUPPORTED_TOOL_VERSIONS = {
  python: ['3.8.10', '3.9.13', '3.10.11', '3.11.9', '3.12.5', '3.14.7'],
  node: ['16.20.2', '18.20.4', '20.15.1', '22.5.1', '24.18.1'],
  git: ['2.43.0', '2.44.0', '2.45.2', '2.46.0', '2.54.0'],
  go: ['1.21.6', '1.22.4', '1.23.0', '1.26.5'],
  java: ['17.0.11', '17.0.12', '17.0.20', '21.0.3', '21.0.5', '21.0.12', '22.0.1', '25.0.4']
} as const

export const SUPPORTED_MIRRORS = ['direct', 'huawei', 'aliyun', 'tuna'] as const

export const MAX_PROTOCOL_STRING_LENGTH = 512
export const MAX_CUSTOM_COMBOS_JSON_LENGTH = 64 * 1024
export const MAX_CUSTOM_COMBOS = 20
export const MAX_COMBO_ITEMS = 10

const MAX_TYPE_LENGTH = 32
const MAX_TOOL_LENGTH = 16
const MAX_VERSION_LENGTH = 32
const MAX_REQUEST_ID_LENGTH = 128
const MAX_COMBO_ID_LENGTH = 64
const MAX_COMBO_NAME_LENGTH = 80
const MAX_COMBO_DESCRIPTION_LENGTH = MAX_PROTOCOL_STRING_LENGTH
const TOKEN_PATTERN = /^[a-zA-Z0-9][a-zA-Z0-9._:-]*$/
const COMBO_ID_PATTERN = /^[a-z0-9][a-z0-9._-]*$/
const FORBIDDEN_OBJECT_KEYS = new Set(['__proto__', 'prototype', 'constructor'])
const BUILTIN_COMBO_IDS = new Set([
  'python-fullstack',
  'java-dev',
  'go-dev',
  'frontend-dev',
  'fullstack-universal'
])

export type ToolId = keyof typeof SUPPORTED_TOOL_VERSIONS
export type ToolVersion = (typeof SUPPORTED_TOOL_VERSIONS)[ToolId][number]
export type DownloadMirror = (typeof SUPPORTED_MIRRORS)[number]

interface RequestMetadata {
  requestId?: string
}

export type UniEnvRequest =
  | ({ type: 'listTools' } & RequestMetadata)
  | ({ type: 'detect'; tool: ToolId } & RequestMetadata)
  | ({ type: 'listVersions'; tool: ToolId } & RequestMetadata)
  | ({ type: 'install'; tool: ToolId; version: ToolVersion } & RequestMetadata)
  | ({ type: 'getTask'; taskId: string } & RequestMetadata)
  | ({ type: 'cancelTask'; taskId: string } & RequestMetadata)
  | ({ type: 'uninstall'; tool: ToolId } & RequestMetadata)
  | ({ type: 'switchVersion'; tool: ToolId; version: ToolVersion } & RequestMetadata)
  | ({ type: 'listCombos' } & RequestMetadata)
  | ({ type: 'installCombo'; comboId: string } & RequestMetadata)

export interface CustomComboItem {
  toolId: ToolId
  version: ToolVersion
}

export interface CustomCombo {
  id: string
  name: string
  description: string
  items: CustomComboItem[]
}

export interface UniEnvConfig {
  installRoot: string
  downloadMirror: DownloadMirror
  customCombos: CustomCombo[]
}

export type UniEnvProtocolErrorCode =
  | 'invalid-value'
  | 'unknown-field'
  | 'unknown-type'
  | 'unknown-tool'
  | 'unknown-mirror'
  | 'unknown-version'
  | 'string-limit'
  | 'invalid-custom-combos'

export class UniEnvProtocolError extends Error {
  readonly code: UniEnvProtocolErrorCode
  readonly path: string

  constructor(code: UniEnvProtocolErrorCode, path: string, message: string) {
    super(message)
    this.name = 'UniEnvProtocolError'
    this.code = code
    this.path = path
  }
}

function fail(code: UniEnvProtocolErrorCode, path: string, message: string): never {
  throw new UniEnvProtocolError(code, path, message)
}

function hasControlCharacters(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index)
    if (code <= 0x1f || code === 0x7f) return true
  }
  return false
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

function readRecord(value: unknown, path: string): Record<string, unknown> {
  if (!isPlainRecord(value)) fail('invalid-value', path, `${path} must be a plain object`)
  return value
}

function assertAllowedKeys(
  value: Record<string, unknown>,
  allowedKeys: readonly string[],
  path: string
): void {
  const allowed = new Set(allowedKeys)
  for (const key of Reflect.ownKeys(value)) {
    if (typeof key !== 'string' || FORBIDDEN_OBJECT_KEYS.has(key) || !allowed.has(key)) {
      fail('unknown-field', `${path}.${String(key)}`, `Unexpected field at ${path}.${String(key)}`)
    }
  }
}

function readString(
  value: unknown,
  path: string,
  maxLength: number,
  options: { allowEmpty?: boolean; pattern?: RegExp } = {}
): string {
  if (typeof value !== 'string') fail('invalid-value', path, `${path} must be a string`)
  if (value.length > maxLength) {
    fail('string-limit', path, `${path} exceeds ${maxLength} characters`)
  }
  if ((!options.allowEmpty && value.length === 0) || hasControlCharacters(value)) {
    fail('invalid-value', path, `${path} is empty or contains control characters`)
  }
  if (value !== value.trim()) fail('invalid-value', path, `${path} has surrounding whitespace`)
  if (options.pattern && !options.pattern.test(value)) {
    fail('invalid-value', path, `${path} has an invalid format`)
  }
  return value
}

function readOptionalRequestId(value: Record<string, unknown>): RequestMetadata {
  if (!Object.prototype.hasOwnProperty.call(value, 'requestId')) return {}
  return {
    requestId: readString(value.requestId, 'message.requestId', MAX_REQUEST_ID_LENGTH, {
      pattern: TOKEN_PATTERN
    })
  }
}

export function isSupportedToolId(value: unknown): value is ToolId {
  return (
    typeof value === 'string' &&
    Object.prototype.hasOwnProperty.call(SUPPORTED_TOOL_VERSIONS, value)
  )
}

export function parseToolId(value: unknown, path = 'tool'): ToolId {
  const tool = readString(value, path, MAX_TOOL_LENGTH, { pattern: TOKEN_PATTERN })
  if (!isSupportedToolId(tool)) fail('unknown-tool', path, `Unsupported tool: ${tool}`)
  return tool
}

export function isSupportedToolVersion(tool: ToolId, value: unknown): value is ToolVersion {
  return (
    typeof value === 'string' &&
    (SUPPORTED_TOOL_VERSIONS[tool] as readonly string[]).includes(value)
  )
}

export function parseToolVersion(tool: ToolId, value: unknown, path = 'version'): ToolVersion {
  const version = readString(value, path, MAX_VERSION_LENGTH, { pattern: TOKEN_PATTERN })
  if (!isSupportedToolVersion(tool, version)) {
    fail('unknown-version', path, `Unsupported ${tool} version: ${version}`)
  }
  return version
}

export function parseDownloadMirror(value: unknown, path = 'downloadMirror'): DownloadMirror {
  const mirror = readString(value, path, MAX_TOOL_LENGTH, { pattern: TOKEN_PATTERN })
  if (!(SUPPORTED_MIRRORS as readonly string[]).includes(mirror)) {
    fail('unknown-mirror', path, `Unsupported download mirror: ${mirror}`)
  }
  return mirror as DownloadMirror
}

function parseComboItem(value: unknown, path: string): CustomComboItem {
  const item = readRecord(value, path)
  assertAllowedKeys(item, ['toolId', 'version'], path)
  const toolId = parseToolId(item.toolId, `${path}.toolId`)
  return {
    toolId,
    version: parseToolVersion(toolId, item.version, `${path}.version`)
  }
}

function parseCombo(value: unknown, index: number): CustomCombo {
  const path = `customCombos[${index}]`
  const combo = readRecord(value, path)
  assertAllowedKeys(combo, ['id', 'name', 'description', 'items'], path)
  const id = readString(combo.id, `${path}.id`, MAX_COMBO_ID_LENGTH, {
    pattern: COMBO_ID_PATTERN
  })
  if (BUILTIN_COMBO_IDS.has(id)) {
    fail('invalid-custom-combos', `${path}.id`, `Custom combo shadows built-in combo: ${id}`)
  }
  const name = readString(combo.name, `${path}.name`, MAX_COMBO_NAME_LENGTH)
  const description =
    combo.description === undefined
      ? ''
      : readString(combo.description, `${path}.description`, MAX_COMBO_DESCRIPTION_LENGTH, {
          allowEmpty: true
        })
  if (
    !Array.isArray(combo.items) ||
    combo.items.length === 0 ||
    combo.items.length > MAX_COMBO_ITEMS
  ) {
    fail(
      'invalid-custom-combos',
      `${path}.items`,
      `Combo items must contain 1-${MAX_COMBO_ITEMS} entries`
    )
  }

  const items: CustomComboItem[] = []
  const tools = new Set<ToolId>()
  for (let itemIndex = 0; itemIndex < combo.items.length; itemIndex++) {
    const item = parseComboItem(combo.items[itemIndex], `${path}.items[${itemIndex}]`)
    if (tools.has(item.toolId)) {
      fail(
        'invalid-custom-combos',
        `${path}.items[${itemIndex}].toolId`,
        `Duplicate tool in combo: ${item.toolId}`
      )
    }
    tools.add(item.toolId)
    items.push(item)
  }

  return { id, name, description, items }
}

export function parseCustomCombos(value: unknown): CustomCombo[] {
  if (typeof value !== 'string') {
    fail('invalid-custom-combos', 'customCombos', 'customCombos must be a JSON string')
  }
  if (value.length > MAX_CUSTOM_COMBOS_JSON_LENGTH) {
    fail(
      'string-limit',
      'customCombos',
      `customCombos exceeds ${MAX_CUSTOM_COMBOS_JSON_LENGTH} characters`
    )
  }
  if (value.trim() === '') return []

  let decoded: unknown
  try {
    decoded = JSON.parse(value) as unknown
  } catch {
    fail('invalid-custom-combos', 'customCombos', 'customCombos is not valid JSON')
  }
  if (!Array.isArray(decoded) || decoded.length > MAX_CUSTOM_COMBOS) {
    fail(
      'invalid-custom-combos',
      'customCombos',
      `customCombos must be an array with at most ${MAX_CUSTOM_COMBOS} entries`
    )
  }

  const result: CustomCombo[] = []
  const ids = new Set<string>()
  for (let index = 0; index < decoded.length; index++) {
    const combo = parseCombo(decoded[index], index)
    if (ids.has(combo.id)) {
      fail('invalid-custom-combos', `customCombos[${index}].id`, `Duplicate combo id: ${combo.id}`)
    }
    ids.add(combo.id)
    result.push(combo)
  }
  return result
}

export function parseUniEnvConfig(value: unknown): UniEnvConfig {
  const config = readRecord(value, 'config')
  assertAllowedKeys(config, ['installRoot', 'downloadMirror', 'customCombos'], 'config')
  const installRoot = readString(
    config.installRoot === undefined ? 'C:\\UniEnv' : config.installRoot,
    'config.installRoot',
    MAX_PROTOCOL_STRING_LENGTH
  )
  const downloadMirror = parseDownloadMirror(
    config.downloadMirror === undefined ? 'direct' : config.downloadMirror,
    'config.downloadMirror'
  )
  const customCombos = parseCustomCombos(
    config.customCombos === undefined ? '[]' : config.customCombos
  )
  return { installRoot, downloadMirror, customCombos }
}

export function parseUniEnvRequest(value: unknown): UniEnvRequest {
  const message = readRecord(value, 'message')
  const type = readString(message.type, 'message.type', MAX_TYPE_LENGTH, {
    pattern: TOKEN_PATTERN
  })
  const metadata = readOptionalRequestId(message)

  switch (type) {
    case 'listTools':
    case 'listCombos':
      assertAllowedKeys(message, ['type', 'requestId'], 'message')
      return { type, ...metadata }

    case 'detect':
    case 'listVersions':
    case 'uninstall':
      assertAllowedKeys(message, ['type', 'tool', 'requestId'], 'message')
      return { type, tool: parseToolId(message.tool, 'message.tool'), ...metadata }

    case 'getTask':
    case 'cancelTask':
      assertAllowedKeys(message, ['type', 'taskId', 'requestId'], 'message')
      return {
        type,
        taskId: readString(message.taskId, 'message.taskId', MAX_REQUEST_ID_LENGTH, {
          pattern: TOKEN_PATTERN
        }),
        ...metadata
      }

    case 'install':
    case 'switchVersion': {
      assertAllowedKeys(message, ['type', 'tool', 'version', 'requestId'], 'message')
      const tool = parseToolId(message.tool, 'message.tool')
      return {
        type,
        tool,
        version: parseToolVersion(tool, message.version, 'message.version'),
        ...metadata
      }
    }

    case 'installCombo':
      assertAllowedKeys(message, ['type', 'comboId', 'requestId'], 'message')
      return {
        type,
        comboId: readString(message.comboId, 'message.comboId', MAX_COMBO_ID_LENGTH, {
          pattern: COMBO_ID_PATTERN
        }),
        ...metadata
      }

    default:
      return fail('unknown-type', 'message.type', `Unsupported message type: ${type}`)
  }
}

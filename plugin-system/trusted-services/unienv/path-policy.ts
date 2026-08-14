import { win32 } from 'path'
import {
  isSupportedToolId,
  isSupportedToolVersion,
  type ToolId,
  type ToolVersion
} from './protocol'

export const MAX_INSTALL_PATH_LENGTH = 240

const DRIVE_ABSOLUTE_PATH = /^[a-zA-Z]:[\\/]/
const UNC_OR_DEVICE_PREFIX = /^(?:[\\/]{2}|[\\/](?:\?\?|device)[\\/])/i
const INVALID_WIN32_SEGMENT_CHARACTERS = /[<>:"|?*'`$&^%!;]/
const RESERVED_WIN32_NAME = /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i

export type PathPolicyErrorCode =
  | 'invalid-path'
  | 'path-limit'
  | 'namespace-path'
  | 'relative-path'
  | 'path-traversal'
  | 'drive-root'
  | 'invalid-segment'
  | 'unknown-tool'
  | 'unknown-version'
  | 'outside-root'

export class PathPolicyError extends Error {
  readonly code: PathPolicyErrorCode

  constructor(code: PathPolicyErrorCode, message: string) {
    super(message)
    this.name = 'PathPolicyError'
    this.code = code
  }
}

function fail(code: PathPolicyErrorCode, message: string): never {
  throw new PathPolicyError(code, message)
}

function hasControlCharacters(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index)
    if (code <= 0x1f || code === 0x7f) return true
  }
  return false
}

function assertSafeSegment(segment: string, label: string): void {
  if (segment.length === 0 || segment === '..') {
    fail('path-traversal', `${label} contains an unsafe traversal segment`)
  }
  if (segment === '.') return
  if (
    hasControlCharacters(segment) ||
    INVALID_WIN32_SEGMENT_CHARACTERS.test(segment) ||
    segment.includes('/') ||
    segment.includes('\\') ||
    segment.endsWith('.') ||
    segment.endsWith(' ')
  ) {
    fail('invalid-segment', `${label} contains characters that are unsafe on Windows`)
  }
  if (RESERVED_WIN32_NAME.test(segment)) {
    fail('invalid-segment', `${label} uses a reserved Windows device name`)
  }
}

export function canonicalizeInstallRoot(value: unknown): string {
  if (typeof value !== 'string' || value.length === 0 || value !== value.trimStart()) {
    fail('invalid-path', 'Install root must be a non-empty string without leading whitespace')
  }
  if (value.length > MAX_INSTALL_PATH_LENGTH) {
    fail('path-limit', `Install root exceeds ${MAX_INSTALL_PATH_LENGTH} characters`)
  }
  if (hasControlCharacters(value)) {
    fail('invalid-path', 'Install root contains control characters')
  }
  if (UNC_OR_DEVICE_PREFIX.test(value)) {
    fail('namespace-path', 'UNC and Windows device namespace paths are not supported')
  }
  if (!DRIVE_ABSOLUTE_PATH.test(value) || !win32.isAbsolute(value)) {
    fail('relative-path', 'Install root must be an absolute drive path such as C:\\UniEnv')
  }

  const portable = value.replace(/\//g, '\\')
  const rawSegments = portable.slice(3).split('\\')
  for (let index = 0; index < rawSegments.length; index++) {
    const segment = rawSegments[index]
    if (segment === '') continue
    assertSafeSegment(segment, `Install root segment ${index + 1}`)
  }

  let normalized = win32.normalize(portable)
  normalized = normalized[0].toUpperCase() + normalized.slice(1)
  while (normalized.length > 3 && normalized.endsWith('\\')) normalized = normalized.slice(0, -1)

  if (normalized.toLowerCase() === win32.parse(normalized).root.toLowerCase()) {
    fail('drive-root', 'A drive root cannot be used as the installation root')
  }
  if (normalized.length > MAX_INSTALL_PATH_LENGTH) {
    fail('path-limit', `Canonical install root exceeds ${MAX_INSTALL_PATH_LENGTH} characters`)
  }
  return normalized
}

export function safeJoinVersionDirectory(
  installRoot: unknown,
  tool: ToolId,
  version: ToolVersion
): string {
  const root = canonicalizeInstallRoot(installRoot)
  if (!isSupportedToolId(tool)) fail('unknown-tool', `Unsupported tool: ${String(tool)}`)
  if (!isSupportedToolVersion(tool, version)) {
    fail('unknown-version', `Unsupported ${tool} version: ${String(version)}`)
  }
  assertSafeSegment(tool, 'Tool id')
  assertSafeSegment(version, 'Version')

  const target = win32.join(root, tool, version)
  const relative = win32.relative(root, target)
  if (
    relative.length === 0 ||
    relative === '..' ||
    relative.startsWith(`..${win32.sep}`) ||
    win32.isAbsolute(relative)
  ) {
    fail('outside-root', 'Version directory is not a descendant of the installation root')
  }
  if (target.length > MAX_INSTALL_PATH_LENGTH) {
    fail('path-limit', `Version directory exceeds ${MAX_INSTALL_PATH_LENGTH} characters`)
  }
  return target
}

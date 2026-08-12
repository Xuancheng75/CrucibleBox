import { SUPPORTED_TOOL_VERSIONS, type ToolId } from './protocol'

export const TOOL_VERSION_LIFECYCLE_AS_OF = '2026-08-11'

export type ToolVersionSupportStatus = 'current' | 'maintained-branch' | 'eol' | 'legacy'

export interface ToolVersionLifecycle {
  readonly status: ToolVersionSupportStatus
  readonly label: string
  readonly note: string
  readonly sourceUrl: string
  readonly preferredInCatalog?: true
}

type ToolVersionLifecycleCatalog = {
  readonly [Tool in ToolId]: {
    readonly [Version in (typeof SUPPORTED_TOOL_VERSIONS)[Tool][number]]: ToolVersionLifecycle
  }
}

const PYTHON_SOURCE = 'https://devguide.python.org/versions/'
const NODE_SOURCE = 'https://github.com/nodejs/Release/blob/main/schedule.json'
const GIT_SOURCE = 'https://git-scm.com/install/windows.html'
const GO_SOURCE = 'https://go.dev/doc/devel/release'
const JAVA_SOURCE = 'https://adoptium.net/support/'

const eol = (note: string, sourceUrl: string): ToolVersionLifecycle => ({
  status: 'eol',
  label: '已停止维护',
  note,
  sourceUrl
})

const current = (
  note: string,
  sourceUrl: string,
  preferredInCatalog = false
): ToolVersionLifecycle => ({
  status: 'current',
  label: '当前维护版本',
  note,
  sourceUrl,
  ...(preferredInCatalog ? { preferredInCatalog: true as const } : {})
})

const maintainedBranch = (
  note: string,
  sourceUrl: string,
  preferredInCatalog = false
): ToolVersionLifecycle => ({
  status: 'maintained-branch',
  label: '维护分支的旧补丁',
  note,
  sourceUrl,
  ...(preferredInCatalog ? { preferredInCatalog: true as const } : {})
})

const legacy = (
  note: string,
  sourceUrl: string,
  preferredInCatalog = false
): ToolVersionLifecycle => ({
  status: 'legacy',
  label: '旧版',
  note,
  sourceUrl,
  ...(preferredInCatalog ? { preferredInCatalog: true as const } : {})
})

const TOOL_VERSION_LIFECYCLE = {
  python: {
    '3.8.10': eol('Python 3.8 已结束官方安全维护，仅用于旧项目兼容。', PYTHON_SOURCE),
    '3.9.13': eol('Python 3.9 已结束官方安全维护，仅用于旧项目兼容。', PYTHON_SOURCE),
    '3.10.11': maintainedBranch(
      'Python 3.10 分支仅接收安全修复，3.10.11 是较旧的最后一个官方 Windows 安装器。',
      PYTHON_SOURCE
    ),
    '3.11.9': maintainedBranch(
      'Python 3.11 分支仍接收安全修复，但此固定 Windows 安装器不是该分支最新补丁。',
      PYTHON_SOURCE
    ),
    '3.12.5': maintainedBranch(
      'Python 3.12 分支仍接收安全修复，但此固定 Windows 安装器不是该分支最新补丁。',
      PYTHON_SOURCE
    ),
    '3.14.7': current(
      'Python 3.14.7 是当前稳定维护补丁；传统 Windows 安装器在 3.14/3.15 期间仍由官方提供。',
      PYTHON_SOURCE,
      true
    )
  },
  node: {
    '16.20.2': eol('Node.js 16 已结束官方安全维护，仅用于旧项目兼容。', NODE_SOURCE),
    '18.20.4': eol('Node.js 18 已结束官方安全维护，仅用于旧项目兼容。', NODE_SOURCE),
    '20.15.1': eol('Node.js 20 已结束官方安全维护，仅用于旧项目兼容。', NODE_SOURCE),
    '22.5.1': maintainedBranch(
      'Node.js 22 分支仍处于 Maintenance LTS，但此固定制品不是该分支最新补丁。',
      NODE_SOURCE
    ),
    '24.18.1': current('Node.js 24.18.1 是当前 Active LTS 分支的最新补丁。', NODE_SOURCE, true)
  },
  git: {
    '2.43.0': legacy(
      'Git for Windows 不提供 LTS 分支；此版本已明显落后于当前维护版本。',
      GIT_SOURCE
    ),
    '2.44.0': legacy(
      'Git for Windows 不提供 LTS 分支；此版本已明显落后于当前维护版本。',
      GIT_SOURCE
    ),
    '2.45.2': legacy(
      'Git for Windows 不提供 LTS 分支；此版本已明显落后于当前维护版本。',
      GIT_SOURCE
    ),
    '2.46.0': legacy('Git for Windows 不提供 LTS 分支；此版本已落后于当前维护版本。', GIT_SOURCE),
    '2.54.0': current('Git 2.54.0 是当前 Git for Windows 正式版。', GIT_SOURCE, true)
  },
  go: {
    '1.21.6': eol('Go 只维护最新两个大版本；Go 1.21 已停止维护。', GO_SOURCE),
    '1.22.4': eol('Go 只维护最新两个大版本；Go 1.22 已停止维护。', GO_SOURCE),
    '1.23.0': eol('Go 只维护最新两个大版本；Go 1.23 已停止维护。', GO_SOURCE),
    '1.26.5': current('Go 1.26.5 是当前 Go 1.26 分支的最新安全补丁。', GO_SOURCE, true)
  },
  java: {
    '17.0.11': maintainedBranch(
      'Temurin 17 是 LTS 分支，但此固定制品不是该分支最新安全补丁。',
      JAVA_SOURCE
    ),
    '17.0.12': maintainedBranch(
      'Temurin 17 是 LTS 分支，但此固定制品不是该分支最新安全补丁。',
      JAVA_SOURCE
    ),
    '17.0.20': current('Temurin 17.0.20 是当前 Temurin 17 LTS 安全补丁。', JAVA_SOURCE),
    '21.0.3': maintainedBranch(
      'Temurin 21 是 LTS 分支，但此固定制品不是该分支最新安全补丁。',
      JAVA_SOURCE
    ),
    '21.0.5': maintainedBranch(
      'Temurin 21 是 LTS 分支，但此固定制品不是该分支最新安全补丁。',
      JAVA_SOURCE
    ),
    '21.0.12': current(
      'Temurin 21.0.12 是当前 Temurin 21 LTS 安全补丁，并作为兼容性首选。',
      JAVA_SOURCE,
      true
    ),
    '22.0.1': eol('Temurin 22 是非 LTS 版本，已结束维护。', JAVA_SOURCE),
    '25.0.4': current('Temurin 25.0.4 是当前 Temurin 25 LTS 安全补丁。', JAVA_SOURCE)
  }
} as const satisfies ToolVersionLifecycleCatalog

export function getToolVersionLifecycle(tool: ToolId, version: string): ToolVersionLifecycle {
  const lifecycle = (TOOL_VERSION_LIFECYCLE[tool] as Record<string, ToolVersionLifecycle>)[version]
  if (!lifecycle) throw new Error(`${tool} ${version} 的生命周期信息未维护`)
  return lifecycle
}

export function getPreferredToolVersion(tool: ToolId, versions: readonly string[]): string {
  const preferred = versions.find(
    (version) => getToolVersionLifecycle(tool, version).preferredInCatalog === true
  )
  if (!preferred) throw new Error(`${tool} 的版本目录缺少首选兼容版本`)
  return preferred
}

export function orderToolVersionsForDisplay(tool: ToolId, versions: readonly string[]): string[] {
  const preferred = getPreferredToolVersion(tool, versions)
  return [...versions].sort((left, right) => {
    if (left === preferred) return -1
    if (right === preferred) return 1
    return right.localeCompare(left, undefined, { numeric: true })
  })
}

export function formatToolVersionOption(tool: ToolId, version: string): string {
  const lifecycle = getToolVersionLifecycle(tool, version)
  return `${version} · ${lifecycle.label}${lifecycle.preferredInCatalog ? ' · 目录首选' : ''}`
}

export function requiresToolVersionConfirmation(tool: ToolId, version: string): boolean {
  return getToolVersionLifecycle(tool, version).status !== 'current'
}

export function requiresComboVersionConfirmation(
  items: readonly { toolId: ToolId; version: string }[]
): boolean {
  return items.some(({ toolId, version }) => requiresToolVersionConfirmation(toolId, version))
}

export function formatComboLifecycleSummary(
  items: readonly { toolId: ToolId; version: string }[]
): string {
  return items
    .map(({ toolId, version }) => {
      const lifecycle = getToolVersionLifecycle(toolId, version)
      return `${toolId} ${version}：${lifecycle.label}`
    })
    .join('\n')
}

/**
 * Renderer-side UniEnv catalog.  Keep this in the plugin bundle so the UI
 * remains compatible with older archived Electron protocol files while the
 * Tauri trusted service exposes the Rust and PHP tools.
 */
export type ToolId = 'python' | 'node' | 'git' | 'go' | 'java' | 'rust' | 'php'

export type ToolVersionLifecycle = {
  status: 'current' | 'maintained-branch' | 'eol' | 'legacy'
  label: string
  note: string
  preferredInCatalog?: true
}

export const TOOL_VERSION_LIFECYCLE_AS_OF = '2026-08-30'

const current = (note: string, preferred = false): ToolVersionLifecycle => ({
  status: 'current',
  label: '当前维护版本',
  note,
  ...(preferred ? { preferredInCatalog: true as const } : {})
})

const legacy = (note: string): ToolVersionLifecycle => ({
  status: 'legacy',
  label: '旧版',
  note
})

const CATALOG: Record<ToolId, Record<string, ToolVersionLifecycle>> = {
  python: {
    '3.8.10': legacy('Python 3.8 仅用于旧项目兼容。'),
    '3.9.13': legacy('Python 3.9 仅用于旧项目兼容。'),
    '3.10.11': current('Python 3.10 维护分支。'),
    '3.11.9': current('Python 3.11 维护分支。'),
    '3.12.5': current('Python 3.12 维护分支。'),
    '3.14.7': current('Python 3.14 当前稳定版本。', true)
  },
  node: {
    '16.20.2': legacy('Node.js 16 仅用于旧项目兼容。'),
    '18.20.4': legacy('Node.js 18 仅用于旧项目兼容。'),
    '20.15.1': legacy('Node.js 20 仅用于旧项目兼容。'),
    '22.5.1': current('Node.js 22 维护分支。'),
    '24.18.1': current('Node.js 24 当前稳定版本。', true)
  },
  git: {
    '2.43.0': legacy('Git 旧版，仅用于兼容。'),
    '2.44.0': legacy('Git 旧版，仅用于兼容。'),
    '2.45.2': legacy('Git 旧版，仅用于兼容。'),
    '2.46.0': legacy('Git 旧版，仅用于兼容。'),
    '2.54.0': current('Git for Windows 当前正式版。', true)
  },
  go: {
    '1.21.6': legacy('Go 1.21 仅用于旧项目兼容。'),
    '1.22.4': legacy('Go 1.22 仅用于旧项目兼容。'),
    '1.23.0': legacy('Go 1.23 仅用于旧项目兼容。'),
    '1.26.5': current('Go 1.26 当前安全补丁。', true)
  },
  java: {
    '17.0.11': current('Temurin 17 LTS 旧补丁。'),
    '17.0.12': current('Temurin 17 LTS 旧补丁。'),
    '17.0.20': current('Temurin 17 LTS 安全补丁。'),
    '21.0.3': current('Temurin 21 LTS 旧补丁。'),
    '21.0.5': current('Temurin 21 LTS 旧补丁。'),
    '21.0.12': current('Temurin 21 LTS 首选补丁。', true),
    '22.0.1': legacy('Temurin 22 已结束维护。'),
    '25.0.4': current('Temurin 25 LTS 安全补丁。')
  },
  rust: {
    stable: current('Rust stable 工具链；安装在 UniEnv 独立目录。', true)
  },
  php: {
    '8.3.33': current('PHP 8.3 NTS x64 当前目录中已验证的版本。', true)
  }
}

export function getToolVersionLifecycle(tool: ToolId, version: string): ToolVersionLifecycle {
  return (
    CATALOG[tool]?.[version] ?? {
      status: 'current',
      label: '在线版本',
      note: '该版本来自在线目录，安装前仍会执行制品完整性校验。'
    }
  )
}

export function getPreferredToolVersion(tool: ToolId, versions: readonly string[]): string {
  const preferred = versions.find(
    (version) => getToolVersionLifecycle(tool, version).preferredInCatalog === true
  )
  return preferred ?? versions[0] ?? ''
}

export function orderToolVersionsForDisplay(tool: ToolId, versions: readonly string[]): string[] {
  const preferred = getPreferredToolVersion(tool, versions)
  return [...new Set(versions)].sort((left, right) => {
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
    .map(({ toolId, version }) => `${toolId} ${version}：${getToolVersionLifecycle(toolId, version).label}`)
    .join('\n')
}

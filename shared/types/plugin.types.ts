import type { Permission } from './permissions'

export interface ConfigField {
  type: 'string' | 'number' | 'boolean' | 'select' | 'multiselect'
  label: string
  description?: string
  default?: unknown
  required?: boolean
  options?: { label: string; value: string }[]
}

export interface PluginMeta {
  id: string
  name: string
  version: string
  displayName: string
  description: string
  author: string
  icon?: string
  entryMain: string
  entryRenderer: string
  permissions: Permission[]
  configSchema: Record<string, ConfigField>
  configData: PluginConfig
  enabled: boolean
  installedAt: string
  updatedAt: string
  /** Stable user-facing sort position. Lower values come first in the plugin list. */
  publisherVerified?: boolean
  officialPublisher?: boolean
  publisherId?: string
  sortOrder?: number
}

export interface PluginConfig {
  [key: string]: unknown
}

export enum PluginLifecycleStatus {
  Inactive = 'inactive',
  Activating = 'activating',
  Active = 'active',
  Deactivating = 'deactivating',
  Error = 'error'
}

export interface PluginLogEntry {
  id: number
  pluginId: string
  level: 'info' | 'warn' | 'error' | 'debug'
  message: string
  timestamp: string
}

export interface PluginLogFilter {
  pluginId?: string
  level?: string
  limit?: number
}

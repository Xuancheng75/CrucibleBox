import type { ComponentType } from 'react'
import type { Permission } from './permissions'
import type { ToolboxTheme } from './theme.types'

export interface ConfigField {
  type: 'string' | 'number' | 'boolean' | 'select' | 'multiselect'
  label: string
  description?: string
  default?: unknown
  required?: boolean
  options?: { label: string; value: string }[]
}

export interface PluginManifest {
  manifestVersion?: 1 | 2
  name: string
  version: string
  displayName: string
  description: string
  author: string
  icon?: string
  main: string
  renderer: string
  backend?: boolean
  backendApiVersion?: 1 | 2
  rendererApiVersion?: 1 | 2
  permissions: Permission[]
  config?: Record<string, ConfigField>
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
  sortOrder?: number
}

export interface PluginConfig {
  [key: string]: unknown
}

export interface PluginRecord {
  id: string
  name: string
  version: string
  display_name: string
  description: string
  author: string
  icon?: string
  entry_main: string
  entry_renderer: string
  permissions: string
  config_schema: string
  config_data: string
  enabled: number
  installed_path: string
  installed_at: string
  updated_at: string
  sort_order: number
}

export enum PluginLifecycleStatus {
  Inactive = 'inactive',
  Activating = 'activating',
  Active = 'active',
  Deactivating = 'deactivating',
  Error = 'error'
}

export interface PluginLogger {
  info(message: string, ...args: unknown[]): void
  warn(message: string, ...args: unknown[]): void
  error(message: string, ...args: unknown[]): void
  debug(message: string, ...args: unknown[]): void
}

export interface PluginDatabaseAPI {
  query(sql: string, params?: unknown[]): Promise<unknown[]>
  execute(sql: string, params?: unknown[]): Promise<void>
}

export interface PluginStorageEntry<T = unknown> {
  key: string
  value: T
}

export type PluginStorageMutation =
  { type: 'set'; key: string; value: unknown } | { type: 'delete'; key: string }

export interface PluginStorageAPI {
  get<T = unknown>(key: string): Promise<T | null>
  set(key: string, value: unknown): Promise<void>
  delete(key: string): Promise<void>
  list<T = unknown>(prefix?: string): Promise<PluginStorageEntry<T>[]>
  batch(mutations: PluginStorageMutation[]): Promise<void>
}

export interface SystemInfo {
  os: { name: string; version: string; hostname: string }
  cpu: { brand: string; cores: number; physicalCores: number; usage: number }
  memory: { total: number; available: number; usage: number }
  disks: Array<{ name: string; total: number; available: number }>
  network: Array<{ name: string; ip: string; mac: string }>
}

/**
 * JSON-safe response returned by a backend plugin host.
 *
 * Tauri's QuickJS backend cannot expose the browser `Response` object across
 * the RPC boundary. Electron's legacy backend still returns a native
 * `Response`, so the public plugin contract accepts both representations.
 */
export interface PluginFetchResponse {
  ok: boolean
  status: number
  statusText: string
  headers: Record<string, string>
  body: string
}

export interface PluginHostAPI {
  notify(title: string, body?: string): void
  openDialog(type: 'file' | 'folder'): Promise<string | null>
  fetch(url: string, opts?: RequestInit): Promise<Response | PluginFetchResponse>
  readFile(path: string): Promise<Uint8Array>
  writeFile(path: string, data: string | Uint8Array): Promise<void>
  registerShortcut(keys: string, handler: () => void): () => void
  emitEvent(event: string, data?: unknown): void
  onEvent(event: string, handler: (data: unknown) => void): () => void
  clipboard: {
    read(): Promise<{ text: string }>
    write(text: string): Promise<{ ok: boolean }>
  }
  getSystemInfo(): Promise<SystemInfo>
}

export interface PluginContext {
  id: string
  config: PluginConfig
  logger: PluginLogger
  database: PluginDatabaseAPI
  storage: PluginStorageAPI
  api: PluginHostAPI
}

export interface PluginMain {
  activate(ctx: PluginContext): void | Promise<void>
  deactivate(): void | Promise<void>
  onMessage?(message: unknown): unknown | Promise<unknown>
}

export interface PluginRenderProps {
  config: PluginConfig
  onConfigChange: (config: PluginConfig) => void
  theme?: ToolboxTheme
  api: {
    sendToBackend(message: unknown): Promise<unknown>
    notify(title: string, body?: string): void
    confirm(options: {
      title: string
      message: string
      confirmLabel?: string
      cancelLabel?: string
    }): Promise<boolean>
    dialog: {
      open(options: {
        type: 'file' | 'folder'
        multiple?: boolean
        extensions?: string[]
      }): Promise<string[]>
    }
    onBackendMessage(handler: (msg: unknown) => void): () => void
    theme: {
      get(): Promise<ToolboxTheme>
      list(): Promise<ToolboxTheme[]>
      preview(theme: ToolboxTheme): Promise<boolean>
      commit(): Promise<boolean>
      rollback(): Promise<boolean>
      set(theme: ToolboxTheme): Promise<boolean>
    }
  }
}

export interface PluginMessage {
  type: string
  payload?: unknown
  id?: string
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

export type PluginRendererComponent = ComponentType<PluginRenderProps>

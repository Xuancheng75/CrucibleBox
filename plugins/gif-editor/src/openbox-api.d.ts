declare module 'openbox-plugin-api' {
  import type { ComponentType } from 'react'

  export interface ConfigField {
    type: 'string' | 'number' | 'boolean' | 'select' | 'multiselect'
    label: string
    description?: string
    default?: unknown
    required?: boolean
    options?: { label: string; value: string }[]
  }

  export interface PluginConfig {
    [key: string]: unknown
  }

  export interface PluginLogger {
    info(message: string, ...args: unknown[]): void
    warn(message: string, ...args: unknown[]): void
    error(message: string, ...args: unknown[]): void
    debug(message: string, ...args: unknown[]): void
  }

  export interface PluginHostAPI {
    notify(title: string, body?: string): void
    openDialog(type: 'file' | 'folder'): Promise<string | null>
    fetch(url: string, opts?: RequestInit): Promise<Response>
    readFile(path: string): Promise<Uint8Array>
    writeFile(path: string, data: string | Uint8Array): Promise<void>
    registerShortcut(keys: string, handler: () => void): () => void
    emitEvent(event: string, data?: unknown): void
    onEvent(event: string, handler: (data: unknown) => void): () => void
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

  export interface PluginContext {
    id: string
    config: PluginConfig
    logger: PluginLogger
    database: {
      query(sql: string, params?: unknown[]): Promise<unknown[]>
      execute(sql: string, params?: unknown[]): Promise<void>
    }
    storage: PluginStorageAPI
    api: PluginHostAPI
  }

  export interface PluginMain {
    activate(ctx: PluginContext): void | Promise<void>
    deactivate(): void | Promise<void>
    onMessage?(message: unknown): unknown | Promise<unknown>
  }

  export interface Theme {
    id: string
    name: string
    mode: 'light' | 'dark'
    tokens: Record<string, string | number>
  }

  export interface PluginRenderProps {
    config: PluginConfig
    onConfigChange: (config: PluginConfig) => void
    theme?: Theme
    api: {
      sendToBackend(message: unknown): Promise<unknown>
      notify(title: string, body?: string): void
      confirm(options: {
        title: string
        message: string
        confirmLabel?: string
        cancelLabel?: string
      }): Promise<boolean>
      onBackendMessage(handler: (msg: unknown) => void): () => void
      theme: {
        get(): Promise<Theme>
        list(): Promise<Theme[]>
        preview(theme: Theme): Promise<boolean>
        commit(): Promise<boolean>
        rollback(): Promise<boolean>
        set(theme: Theme): Promise<boolean>
      }
    }
  }

  export type PluginRendererComponent = ComponentType<PluginRenderProps>
}

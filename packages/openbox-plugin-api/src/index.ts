import type { ComponentType } from 'react'

/**
 * 插件 API 类型唯一事实源（Plugin SDK v2）。
 *
 * 此文件由各插件本地复制的 openbox-api.d.ts 收敛而来：
 * - 6 插件 + 模板的 `declare module 'openbox-plugin-api' { ... }` 环境声明合并为真实模块；
 * - unienv 特有的 `PluginHostAPI.invokeTrustedService`（trusted:unienv 权限）并入通用接口；
 * - 宿主侧类型契约见 `shared/types/plugin.types.ts`（本包为插件视角的权威声明）。
 *
 * 修改此文件后，重建所有插件 dist（npm run build:plugins）；若 unienv dist 摘要变化，
 * 运行 `npm run update:trusted-policy` 重钉 trusted-service-policies.json。
 */

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
  /** 仅宿主固定摘要可信服务（trusted:unienv 权限）可用；普通插件调用会失败 */
  invokeTrustedService?(service: string, operation: string, payload?: unknown): Promise<unknown>
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

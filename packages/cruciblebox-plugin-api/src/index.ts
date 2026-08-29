import type { ComponentType } from 'react'

/**
 * 插件 API 类型唯一事实源（Plugin SDK v2）。
 *
 * 此文件由各插件本地复制的 openbox-api.d.ts 收敛而来：
 * - 6 插件 + 模板的 `declare module 'cruciblebox-plugin-api' { ... }` 环境声明合并为真实模块；
 * - 宿主固定可信服务的 `PluginHostAPI.invokeTrustedService`（例如 trusted:unienv、trusted:document-engine）并入通用接口；
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
  fetch(url: string, opts?: RequestInit): Promise<Response | PluginFetchResponse>
  readFile(path: string): Promise<Uint8Array>
  writeFile(path: string, data: string | Uint8Array): Promise<void>
  registerShortcut(keys: string, handler: () => void): () => void
  emitEvent(event: string, data?: unknown): void
  onEvent(event: string, handler: (data: unknown) => void): () => void
  /** 剪贴板读写（需 clipboard 权限） */
  clipboard: {
    read(): Promise<{ text: string }>
    write(text: string): Promise<{ ok: boolean }>
  }
  /** 获取系统信息（无需权限，公开只读） */
  getSystemInfo(): Promise<SystemInfo>
  /** 仅宿主固定摘要可信服务权限（例如 trusted:unienv / trusted:document-engine）可用；普通插件调用会失败 */
  invokeTrustedService?(service: string, operation: string, payload?: unknown): Promise<unknown>
}

/** JSON-safe fetch response used by the Tauri QuickJS backend. */
export interface PluginFetchResponse {
  ok: boolean
  status: number
  statusText: string
  headers: Record<string, string>
  body: string
}

export interface SystemInfo {
  os: { name: string; version: string; hostname: string }
  cpu: { brand: string; cores: number; physicalCores: number; usage: number }
  memory: { total: number; available: number; usage: number }
  disks: Array<{ name: string; total: number; available: number }>
  network: Array<{ name: string; ip: string; mac: string }>
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
    dialog: {
      open(options: {
        type: 'file' | 'folder'
        multiple?: boolean
        extensions?: string[]
      }): Promise<string[]>
    }
    onBackendMessage(handler: (msg: unknown) => void): () => void
    /** Receive real OS file/folder paths when the host supports OS drop forwarding. */
    onFilesDropped?(handler: (paths: string[]) => void): () => void
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

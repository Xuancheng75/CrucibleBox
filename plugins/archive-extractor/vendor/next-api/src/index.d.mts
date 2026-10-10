// Generated; edit contracts/next/contract.json.
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }
export type Request =
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'runtime.ping'
      params: Record<string, never>
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.get'
      params: { key: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.set'
      params: { key: string; value: Json }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'backend.call'
      params: { method: string; args: Json[] }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.delete'
      params: { key: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.batch'
      params: {
        operations: ({ type: 'set'; key: string; value: Json } | { type: 'delete'; key: string })[]
      }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.list'
      params: { prefix: string; limit: number; after?: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.keys'
      params: { prefix: string; limit: number; after?: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.read.begin'
      params: { key: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.read.chunk'
      params: { readId: string; offset: number }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.read.close'
      params: { readId: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.write.begin'
      params: { writes: { key: string; byteLength: number }[]; deletes: string[] }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.write.chunk'
      params: { transactionId: string; key: string; offset: number; data: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.write.commit'
      params: { transactionId: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'storage.write.abort'
      params: { transactionId: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'document.call'
      params: { payload: { [key: string]: Json } }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'environment.call'
      params: { payload: { [key: string]: Json } }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'archive.call'
      params: { payload: { [key: string]: Json } }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'config.get'
      params: Record<string, never>
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'config.patch'
      params: { values: { [key: string]: Json } }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'tasks.get'
      params: { taskId: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'tasks.cancel'
      params: { taskId: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'tasks.list'
      params: { limit: number; after?: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'theme.get'
      params: Record<string, never>
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'theme.list'
      params: Record<string, never>
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'theme.preview'
      params: {
        theme: {
          id: string
          name: string
          mode: 'light' | 'dark'
          tokens: {
            colorBg: string
            colorBgLayout: string
            colorBgContainer: string
            colorBgElevated: string
            colorPrimary: string
            colorPrimaryHover: string
            colorPrimaryBg: string
            colorText: string
            colorTextSecondary: string
            colorTextTertiary: string
            colorBorder: string
            colorBorderSecondary: string
            colorSuccess: string
            colorSuccessBg: string
            colorWarning: string
            colorWarningBg: string
            colorError: string
            colorErrorBg: string
            colorLink: string
            borderRadius: number
            fontFamily: string
          }
        }
      }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'theme.commit'
      params: Record<string, never>
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'theme.rollback'
      params: Record<string, never>
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'theme.set'
      params: {
        theme: {
          id: string
          name: string
          mode: 'light' | 'dark'
          tokens: {
            colorBg: string
            colorBgLayout: string
            colorBgContainer: string
            colorBgElevated: string
            colorPrimary: string
            colorPrimaryHover: string
            colorPrimaryBg: string
            colorText: string
            colorTextSecondary: string
            colorTextTertiary: string
            colorBorder: string
            colorBorderSecondary: string
            colorSuccess: string
            colorSuccessBg: string
            colorWarning: string
            colorWarningBg: string
            colorError: string
            colorErrorBg: string
            colorLink: string
            borderRadius: number
            fontFamily: string
          }
        }
      }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'dialog.open'
      params: { type: 'file' | 'folder'; multiple?: boolean; extensions?: string[] }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'dialog.confirm'
      params: { title: string; message: string; confirmLabel?: string; cancelLabel?: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'notification.show'
      params: { title: string; body: string }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'result.read.chunk'
      params: { readId: string; offset: number }
    }
  | {
      wireVersion: 3
      requestId: string
      session: string
      method: 'result.read.close'
      params: { readId: string }
    }
export type Response =
  | { wireVersion: 3; requestId: string; ok: true; result: Json }
  | {
      wireVersion: 3
      requestId: string
      ok: false
      error: {
        code:
          | 'INVALID_REQUEST'
          | 'INVALID_RESPONSE'
          | 'BUDGET_EXCEEDED'
          | 'SESSION_DENIED'
          | 'SESSION_EXPIRED'
          | 'SESSION_EXHAUSTED'
          | 'PERMISSION_DENIED'
          | 'REPLAY_DENIED'
          | 'BUSY'
          | 'TIMEOUT'
          | 'STORAGE_UNAVAILABLE'
          | 'STORAGE_CORRUPT'
          | 'INTERNAL_ERROR'
        message: string
      }
    }
export type TaskSnapshot = {
  taskId: string
  status: 'queued' | 'running' | 'paused' | 'succeeded' | 'failed' | 'cancelled' | 'interrupted'
  sequence: number
  cancelRequested: boolean
  resultRefs: string[]
  resourceKey: string
  [key: string]: Json
}
export type StorageOperation =
  { type: 'set'; key: string; value: Json } | { type: 'delete'; key: string }
export type StoragePage = { items: { key: string; value: Json }[]; nextCursor: string | null }
export type RendererAppearance = {
  mode: 'light' | 'dark'
  cssVars: {
    '--ob-mode': string
    '--ob-theme-id': string
    '--ob-color-primary-contrast': string
    '--ob-color-success-border': string
    '--ob-color-warning-border': string
    '--ob-color-error-border': string
    '--ob-color-bg': string
    '--ob-color-bg-layout': string
    '--ob-color-bg-container': string
    '--ob-color-bg-elevated': string
    '--ob-color-primary': string
    '--ob-color-primary-hover': string
    '--ob-color-primary-bg': string
    '--ob-color-text': string
    '--ob-color-text-secondary': string
    '--ob-color-text-tertiary': string
    '--ob-color-border': string
    '--ob-color-border-secondary': string
    '--ob-color-success': string
    '--ob-color-success-bg': string
    '--ob-color-warning': string
    '--ob-color-warning-bg': string
    '--ob-color-error': string
    '--ob-color-error-bg': string
    '--ob-color-link': string
    '--ob-radius': string
    '--ob-font-family': string
    '--ob-colorBg': string
    '--ob-colorBgLayout': string
    '--ob-colorBgContainer': string
    '--ob-colorBgElevated': string
    '--ob-colorPrimary': string
    '--ob-colorPrimaryHover': string
    '--ob-colorPrimaryBg': string
    '--ob-colorText': string
    '--ob-colorTextSecondary': string
    '--ob-colorTextTertiary': string
    '--ob-colorBorder': string
    '--ob-colorBorderSecondary': string
    '--ob-colorSuccess': string
    '--ob-colorSuccessBg': string
    '--ob-colorWarning': string
    '--ob-colorWarningBg': string
    '--ob-colorError': string
    '--ob-colorErrorBg': string
    '--ob-colorLink': string
    '--ob-borderRadius': string
    '--ob-fontFamily': string
  }
}
export declare function validateAppearance(raw: string): RendererAppearance
export declare function validateFilesDropped(raw: string): string[]
export interface RendererContext {
  root: HTMLElement
  session: string
  exchange: (request: Request) => Promise<Response>
  appearance: RendererAppearance
  onFilesDropped(listener: (paths: string[]) => void): () => void
  onAppearanceChanged(listener: (appearance: RendererAppearance) => void): () => void
}
export type Manifest = {
  id: string
  version: string
  displayName: string
  manifestVersion: 5
  sdkApiVersion: 5
  wireVersion: 3
  dataSchemaVersion: 1
  renderer: 'dist/renderer.js'
  backend?: 'dist/main.js'
  permissions: (
    | 'storage:read'
    | 'storage:write'
    | 'browser:downloads'
    | 'trusted:document-engine'
    | 'trusted:unienv'
    | 'trusted:archive-extractor'
    | 'tasks:read'
    | 'tasks:control'
    | 'theme:read'
    | 'theme:write'
    | 'dialog'
    | 'notification'
  )[]
  description?: string
  author?: string
  icon?: string
  category?: string
  config?: { [key: string]: Json }
}
export declare function validateManifest(raw: string): Manifest
export declare function validateRequest(raw: string): Request
export declare function validateResponse(raw: string, requestId: string): Response
export declare function createClient(options: {
  session: string
  exchange: (request: Request) => Promise<Response>
}): {
  ping(): Promise<Json>
  storage: {
    get(key: string): Promise<Json>
    set(key: string, value: Json): Promise<Json>
    delete(key: string): Promise<Json>
    batch(operations: StorageOperation[]): Promise<Json>
    transact(operations: StorageOperation[]): Promise<Json>
    keys(prefix?: string, options?: { after?: string; limit?: number }): Promise<StorageKeysPage>
    list(prefix?: string, options?: { after?: string; limit?: number }): Promise<StoragePage>
  }
  dialog: {
    open(options: {
      type: 'file' | 'folder'
      multiple?: boolean
      extensions?: string[]
    }): Promise<string[] | null>
    confirm(options: {
      title: string
      message: string
      confirmLabel?: string
      cancelLabel?: string
    }): Promise<boolean>
  }
  notify(title: string, body?: string): Promise<boolean>
  theme: {
    get(): Promise<Json>
    list(): Promise<Json[]>
    preview(theme: Json): Promise<boolean>
    commit(): Promise<boolean>
    rollback(): Promise<boolean>
    set(theme: Json): Promise<boolean>
  }
  tasks: {
    get(taskId: string): Promise<TaskSnapshot | null>
    list(options?: {
      limit?: number
      after?: string
    }): Promise<{ items: TaskSnapshot[]; nextCursor: string | null }>
    cancel(taskId: string): Promise<{ accepted: boolean; task: TaskSnapshot | null }>
  }
  config: { get(): Promise<Json>; patch(values: { [key: string]: Json }): Promise<Json> }
  document: { call(payload: { type: string; [key: string]: Json }): Promise<Json> }
  environment: { call(payload: { type: string; [key: string]: Json }): Promise<Json> }
  archive: { call(payload: { type: string; [key: string]: Json }): Promise<Json> }
  backend: { call(method: string, args?: Json[]): Promise<Json> }
}
export declare const contractSha256: string
export declare const contract: Readonly<Record<string, unknown>>

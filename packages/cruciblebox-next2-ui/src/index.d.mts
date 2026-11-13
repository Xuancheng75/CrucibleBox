export * from '@cruciblebox/next2-api'
import type {
  Json,
  FileReference,
  createClient as createApiClient,
  RendererContext,
  TaskSnapshot,
  Target,
  StorageKeysResult,
  StorageListResult,
  StorageUsage,
  ThemeAsset,
  ThemeAssetImportParams,
  ThemeBundledAssetImportParams,
  ThemeAssetListResult
} from '@cruciblebox/next2-api'
type Operations = ({ type: 'set'; key: string; value: Json } | { type: 'delete'; key: string })[]
export type StorageOperation = Operations[number]
type Page = { limit?: number; after?: string }
export declare function renderStatus(
  element: HTMLElement,
  state: 'empty' | 'loading' | 'success' | 'error',
  text: string
): void
export declare function createClient(
  context: Pick<RendererContext, 'session' | 'exchange' | 'enableThemeStyle'>
): ReturnType<typeof createApiClient> & {
  ping(): Promise<Json>
  config: { get(): Promise<Json>; patch(values: Record<string, Json>): Promise<Json> }
  storage: {
    usage(): Promise<StorageUsage>
    get(key: string): Promise<Json>
    set(key: string, value: Json): Promise<Json>
    delete(key: string): Promise<Json>
    batch(operations: Operations): Promise<Json>
    transact(operations: Operations): Promise<Json>
    keys(prefix?: string, page?: Page): Promise<StorageKeysResult>
    list(prefix?: string, page?: Page): Promise<StorageListResult>
  }
  files: ReturnType<typeof createApiClient>['files'] & {
    choose(options: {
      type: 'file' | 'folder'
      multiple?: boolean
      extensions?: string[]
    }): Promise<FileReference[] | null>
  }
  theme: {
    assets: {
      import(options: ThemeAssetImportParams): Promise<ThemeAsset>
      importBundled(options: ThemeBundledAssetImportParams): Promise<ThemeAsset>
      get(id: string): Promise<ThemeAsset>
      list(options?: Page): Promise<ThemeAssetListResult>
    }
    get(): Promise<Json>
    list(): Promise<Json>
    preview(theme: Json): Promise<boolean>
    set(theme: Json): Promise<boolean>
    commit(): Promise<boolean>
    rollback(): Promise<boolean>
  }
  dialog: {
    open(options: Json): Promise<string[] | null>
    confirm(options: Json): Promise<boolean>
  }
  notify(title: string, body?: string): Promise<boolean>
  tasks: {
    start(
      target: Target,
      method: string,
      params?: Json,
      options?: { timeoutMs?: number }
    ): Promise<{ taskId: string }>
    get(taskId: string): Promise<TaskSnapshot | null>
    cancel(taskId: string): Promise<{ accepted: boolean; task: TaskSnapshot | null }>
    list(page?: Page): Promise<{ items: TaskSnapshot[]; nextCursor: string | null }>
  }
}

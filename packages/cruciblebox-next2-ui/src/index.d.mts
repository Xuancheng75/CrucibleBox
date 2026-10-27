export * from '@cruciblebox/next2-api'
import type {
  Json,
  RendererContext,
  TaskSnapshot,
  Target,
  StorageKeysResult,
  StorageListResult
} from '@cruciblebox/next2-api'
type Operations = ({ type: 'set'; key: string; value: Json } | { type: 'delete'; key: string })[]
export type StorageOperation = Operations[number]
type Page = { limit?: number; after?: string }
export declare function createClient(context: Pick<RendererContext, 'session' | 'exchange'>): {
  platform(method: string, params?: Json): Promise<Json>
  backend(method: string, params?: Json): Promise<Json>
  service(id: string, method: string, params?: Json): Promise<Json>
  ping(): Promise<Json>
  config: { get(): Promise<Json>; patch(values: Record<string, Json>): Promise<Json> }
  storage: {
    get(key: string): Promise<Json>
    set(key: string, value: Json): Promise<Json>
    delete(key: string): Promise<Json>
    batch(operations: Operations): Promise<Json>
    transact(operations: Operations): Promise<Json>
    keys(prefix?: string, page?: Page): Promise<StorageKeysResult>
    list(prefix?: string, page?: Page): Promise<StorageListResult>
  }
  theme: {
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
    start(target: Target, method: string, params?: Json): Promise<{ taskId: string }>
    get(taskId: string): Promise<TaskSnapshot | null>
    cancel(taskId: string): Promise<{ accepted: boolean; task: TaskSnapshot | null }>
    list(page?: Page): Promise<{ items: TaskSnapshot[]; nextCursor: string | null }>
  }
}

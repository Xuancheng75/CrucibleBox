import type { createClient, Json, StorageOperation } from '../vendor/next-api/src/index.mjs'
export interface DiaryStorage {
  get<T = unknown>(key: string): Promise<T | null>
  set(key: string, value: unknown): Promise<void>
  delete(key: string): Promise<void>
  batch(
    operations: ({ type: 'set'; key: string; value: unknown } | { type: 'delete'; key: string })[]
  ): Promise<void>
  list<T = unknown>(prefix?: string): Promise<{ key: string; value: T }[]>
}
export interface DiaryContext {
  storage: DiaryStorage
  logger: { info(message: string): void; error(message: string, error?: unknown): void }
}
export interface DiaryDomain {
  activate(context: DiaryContext): void
  deactivate(): void
  onMessage(message: unknown): Promise<unknown>
}
export interface DiaryApi {
  execute(message: unknown): Promise<unknown>
}
export interface DiaryRenderProps {
  api: DiaryApi
}
export function createDiaryStorage(client: ReturnType<typeof createClient>): DiaryStorage {
  return {
    async get<T>(key: string) {
      return (await client.storage.get(key)) as T | null
    },
    async set(key, value) {
      await client.storage.set(key, value as Json)
    },
    async delete(key) {
      await client.storage.delete(key)
    },
    async batch(operations) {
      await client.storage.transact(operations as StorageOperation[])
    },
    async list<T>(prefix = '') {
      const entries: { key: string; value: T }[] = []
      let after: string | undefined
      do {
        const page = await client.storage.keys(prefix, { limit: 100, ...(after ? { after } : {}) })
        for (const key of page.items) {
          const value = await client.storage.get(key)
          if (value !== null) entries.push({ key, value: value as T })
        }
        after = page.nextCursor ?? undefined
      } while (after)
      return entries
    }
  }
}

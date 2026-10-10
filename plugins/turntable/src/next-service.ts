import type { createClient, Json, StorageOperation } from '../vendor/next-api/src/index.mjs'
export interface DecisionStorage {
  get<T = unknown>(key: string): Promise<T | null>
  set(key: string, value: unknown): Promise<void>
  batch(
    operations: ({ type: 'set'; key: string; value: unknown } | { type: 'delete'; key: string })[]
  ): Promise<void>
}
export interface DecisionContext {
  storage: DecisionStorage
  logger: { info(message: string): void; error(message: string, error?: unknown): void }
}
export interface DecisionDomain {
  activate(context: DecisionContext): void
  deactivate(): void
  onMessage(message: unknown): Promise<unknown>
}
export interface DecisionRenderProps {
  config: Record<string, unknown>
  api: {
    execute(message: unknown): Promise<unknown>
    notify(title: string, body?: string): Promise<boolean>
  }
}
export function createDecisionStorage(client: ReturnType<typeof createClient>): DecisionStorage {
  return {
    async get<T>(key: string) {
      return (await client.storage.get(key)) as T | null
    },
    async set(key, value) {
      await client.storage.set(key, value as Json)
    },
    async batch(operations) {
      await client.storage.transact(operations as StorageOperation[])
    }
  }
}

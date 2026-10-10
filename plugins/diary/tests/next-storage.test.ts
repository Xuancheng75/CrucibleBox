import { expect, test } from 'vitest'
import { createClient } from '../vendor/next-api/src/index.mjs'
import { createDiaryStorage } from '../src/next-service'
test('Next diary paginates existing keys through the real SDK and uses atomic writes', async () => {
  const keys = Array.from({ length: 103 }, (_, i) => 'entry:' + String(i).padStart(3, '0'))
  const calls: string[] = []
  const client = createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => {
      calls.push(r.method)
      let result: unknown = null
      if (r.method === 'storage.keys') {
        expect(r.params.limit).toBe(100)
        const selected = keys.filter((k) => !r.params.after || k > r.params.after).slice(0, 100)
        result = { items: selected, nextCursor: selected.length === 100 ? selected.at(-1) : null }
      } else if (r.method === 'storage.get') result = { content: '原文🌏', key: r.params.key }
      else if (r.method === 'storage.write.begin') result = { transactionId: 'b'.repeat(64) }
      else if (r.method === 'storage.write.chunk')
        result = { receivedBytes: r.params.offset + atob(r.params.data).length }
      return { wireVersion: 3, requestId: r.requestId, ok: true, result } as never
    }
  })
  const storage = createDiaryStorage(client)
  const rows = await storage.list('entry:')
  expect(rows.map((r) => r.key)).toEqual(keys)
  expect(calls.filter((c) => c === 'storage.keys')).toHaveLength(2)
  await storage.batch([
    { type: 'set', key: 'entry:2026-10-08', value: { content: '正文' } },
    { type: 'delete', key: 'draft:2026-10-08' }
  ])
  expect(calls.slice(-3)).toEqual([
    'storage.write.begin',
    'storage.write.chunk',
    'storage.write.commit'
  ])
})

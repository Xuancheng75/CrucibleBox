import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createClient } from '../src/index.mjs'
test('storage pagination and atomic operations use exact bounded Next shapes', async () => {
  const seen = []
  const client = createClient({
    session: 'a'.repeat(64),
    exchange: async (request) => {
      seen.push(request)
      return {
        wireVersion: 3,
        requestId: request.requestId,
        ok: true,
        result:
          request.method === 'storage.list'
            ? { items: [{ key: 'entry:1', value: { text: '中文' } }], nextCursor: 'entry:1' }
            : null
      }
    }
  })
  assert.equal(
    (await client.storage.list('entry:', { limit: 2, after: 'entry:0' })).items[0].value.text,
    '中文'
  )
  await client.storage.batch([
    { type: 'set', key: 'entry:1', value: 1 },
    { type: 'delete', key: 'draft:1' }
  ])
  await client.storage.delete('entry:1')
  assert.deepEqual(
    seen.map((x) => x.method),
    ['storage.list', 'storage.batch', 'storage.delete']
  )
  assert.deepEqual(seen[0].params, { prefix: 'entry:', limit: 2, after: 'entry:0' })
  await assert.rejects(client.storage.list('', { limit: 1.5 }), /INVALID_REQUEST/)
  await assert.rejects(
    client.storage.batch([{ type: 'delete', key: 'a', value: 1 }]),
    /INVALID_REQUEST/
  )
  assert.equal(seen.length, 3)
})
test('malformed storage page response is rejected rather than accepted as typed output', async () => {
  const client = createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => ({
      wireVersion: 3,
      requestId: r.requestId,
      ok: true,
      result: { items: [], nextCursor: 123 }
    })
  })
  await assert.rejects(client.storage.list(), /INVALID_RESPONSE/)
})

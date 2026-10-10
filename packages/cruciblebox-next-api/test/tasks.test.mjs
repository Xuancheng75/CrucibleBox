import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createClient } from '../src/index.mjs'
const snapshot = (id) => ({
  taskId: id,
  status: 'succeeded',
  sequence: 3,
  cancelRequested: false,
  resultRefs: [],
  resourceKey: 'test'
})
const client = (result) =>
  createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => ({ wireVersion: 3, requestId: r.requestId, ok: true, result })
  })
test('task responses correlate IDs and reject malformed or unordered pages', async () => {
  assert.equal((await client(snapshot('a')).tasks.get('a')).status, 'succeeded')
  await assert.rejects(client(snapshot('b')).tasks.get('a'), /INVALID_RESPONSE/)
  await assert.rejects(
    client({ accepted: true, task: snapshot('b') }).tasks.cancel('a'),
    /INVALID_RESPONSE/
  )
  for (const page of [
    { items: [snapshot('b'), snapshot('a')], nextCursor: null },
    { items: [snapshot('a'), snapshot('a')], nextCursor: null },
    { items: [snapshot('b')], nextCursor: 'other' },
    { items: [], nextCursor: 'a' },
    { items: [snapshot('a')], nextCursor: null }
  ])
    await assert.rejects(client(page).tasks.list({ after: 'a' }), /INVALID_RESPONSE/)
  assert.deepEqual(
    await client({ items: [snapshot('b')], nextCursor: 'b' }).tasks.list({ after: 'a' }),
    { items: [snapshot('b')], nextCursor: 'b' }
  )
})

import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createClient } from '@cruciblebox/next-api'
test('renderer-only ping is capability-free', async () => {
  let captured
  const client = createClient({
    session: 'A'.repeat(32),
    exchange: async (request) => {
      captured = request
      return { wireVersion: 3, requestId: request.requestId, ok: true, result: 'pong' }
    }
  })
  assert.equal(await client.ping(), 'pong')
  assert.equal(captured.method, 'runtime.ping')
  assert.deepEqual(captured.params, {})
})

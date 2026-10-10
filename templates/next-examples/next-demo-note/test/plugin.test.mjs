import { test } from 'node:test'
import assert from 'node:assert/strict'
import { activate } from '../src/main.mjs'
test('backend stores an explicit versioned key without choosing an owner', async () => {
  let value
  const plugin = activate({
    session: 'A'.repeat(32),
    exchange: async (request) => {
      assert.equal(Object.hasOwn(request, 'owner'), false)
      assert.equal(request.params.key, 'note.v1')
      if (request.method === 'storage.set') {
        value = request.params.value
        return { wireVersion: 3, requestId: request.requestId, ok: true, result: null }
      }
      return { wireVersion: 3, requestId: request.requestId, ok: true, result: value ?? null }
    }
  })
  assert.equal(await plugin.load(), null)
  await plugin.save('中文 preserved')
  assert.deepEqual(await plugin.load(), { schema: 1, text: '中文 preserved' })
})

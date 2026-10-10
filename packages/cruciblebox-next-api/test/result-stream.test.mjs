import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createClient } from '../src/index.mjs'
test('large service result streams exact Unicode without invoking its operation twice', async () => {
  const expected = { text: '中文🙂'.repeat(30000) },
    bytes = Buffer.from(JSON.stringify(expected)),
    seen = []
  const c = createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => {
      seen.push(r.method)
      const result =
        r.method === 'document.call'
          ? { $nextResult: { readId: 'b'.repeat(64), byteLength: bytes.length } }
          : r.method === 'result.read.chunk'
            ? {
                offset: r.params.offset,
                data: bytes.subarray(r.params.offset, r.params.offset + 24576).toString('base64')
              }
            : null
      return { wireVersion: 3, requestId: r.requestId, ok: true, result }
    }
  })
  assert.deepEqual(await c.document.call({ type: 'document.jobs.get', taskId: 'large' }), expected)
  assert.equal(seen.filter((method) => method === 'document.call').length, 1)
  assert.equal(seen.at(-1), 'result.read.close')
})
test('invalid result chunk closes snapshot and never returns partial output', async () => {
  const seen = []
  const c = createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => {
      seen.push(r.method)
      return {
        wireVersion: 3,
        requestId: r.requestId,
        ok: true,
        result:
          r.method === 'document.call'
            ? { $nextResult: { readId: 'b'.repeat(64), byteLength: 60000 } }
            : r.method === 'result.read.chunk'
              ? { offset: 1, data: 'YQ==' }
              : null
      }
    }
  })
  await assert.rejects(c.document.call({ type: 'getStatus' }), /INVALID_RESPONSE/)
  assert.deepEqual(seen, ['document.call', 'result.read.chunk', 'result.read.close'])
})

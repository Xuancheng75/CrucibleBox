import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createClient, validateFilesDropped } from '../src/index.mjs'
test('native UI results fail closed and file events remain bounded', async () => {
  const c = createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => ({ wireVersion: 3, requestId: r.requestId, ok: true, result: 'invalid' })
  })
  await assert.rejects(c.dialog.open({ type: 'file' }), /INVALID_RESPONSE/)
  await assert.rejects(c.dialog.confirm({ title: '删除', message: '确定吗' }), /INVALID_RESPONSE/)
  await assert.rejects(c.notify('消息'), /INVALID_RESPONSE/)
  assert.deepEqual(validateFilesDropped(JSON.stringify(['C:/中文.txt'])), ['C:/中文.txt'])
  for (const value of [[], [1], Array(257).fill('a'), ['a'.repeat(2049)]])
    assert.throws(() => validateFilesDropped(JSON.stringify(value)))
})

import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createClient } from '../src/index.mjs'
function client(handler) {
  return createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => ({
      wireVersion: 3,
      requestId: r.requestId,
      ok: true,
      result: await handler(r)
    })
  })
}
test('large Unicode uses bounded chunks with exact original JSON and atomic commit', async () => {
  const raw = JSON.stringify('日记🌏'.repeat(131072))
  const chunks = []
  let committed = false
  const c = client((r) => {
    if (r.method === 'storage.write.begin') {
      assert.deepEqual(r.params.writes, [
        { key: 'diary.original', byteLength: Buffer.byteLength(raw) }
      ])
      return { transactionId: 'b'.repeat(64) }
    }
    if (r.method === 'storage.write.chunk') {
      const bytes = Buffer.from(r.params.data, 'base64')
      assert.ok(bytes.length <= 24576)
      assert.equal(
        r.params.offset,
        chunks.reduce((n, b) => n + b.length, 0)
      )
      chunks.push(bytes)
      return { receivedBytes: r.params.offset + bytes.length }
    }
    assert.equal(r.method, 'storage.write.commit')
    committed = true
    return null
  })
  await c.storage.set('diary.original', JSON.parse(raw))
  assert.equal(Buffer.concat(chunks).toString('utf8'), raw)
  assert.equal(committed, true)
})
test('large get falls back on byte budget and closes the completed snapshot', async () => {
  const raw = Buffer.from(JSON.stringify('中文'.repeat(40000)))
  let closes = 0
  const c = createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => {
      if (r.method === 'storage.get')
        return {
          wireVersion: 3,
          requestId: r.requestId,
          ok: false,
          error: { code: 'BUDGET_EXCEEDED', message: 'BUDGET_EXCEEDED' }
        }
      let result = null
      if (r.method === 'storage.read.begin')
        result = { found: true, readId: 'c'.repeat(64), byteLength: raw.length }
      if (r.method === 'storage.read.chunk')
        result = {
          offset: r.params.offset,
          data: raw.subarray(r.params.offset, r.params.offset + 24576).toString('base64')
        }
      if (r.method === 'storage.read.close') closes++
      return { wireVersion: 3, requestId: r.requestId, ok: true, result }
    }
  })
  assert.equal(await c.storage.get('diary.original'), JSON.parse(raw.toString()))
  assert.equal(closes, 1)
})
test('failed upload aborts without commit; lossy values and extra fields never reach host', async () => {
  const seen = []
  const c = client((r) => {
    seen.push(r.method)
    if (r.method === 'storage.write.begin') return { transactionId: 'd'.repeat(64) }
    if (r.method === 'storage.write.chunk') throw Error('injected')
    return null
  })
  await assert.rejects(c.storage.transact([{ type: 'set', key: 'x', value: 'large' }]), /injected/)
  assert.deepEqual(seen, ['storage.write.begin', 'storage.write.chunk', 'storage.write.abort'])
  const before = seen.length
  for (const op of [
    { type: 'delete', key: 'x', value: 1 },
    { type: 'set', key: 'x', value: NaN },
    { type: 'set', key: 'x', value: { missing: undefined } },
    { type: 'set', key: 'x', value: '\ud800' }
  ])
    await assert.rejects(c.storage.transact([op]), /INVALID_REQUEST/)
  assert.equal(seen.length, before)
})

test('incorrect upload acknowledgement aborts before commit', async () => {
  const seen = []
  const c = client((r) => {
    seen.push(r.method)
    if (r.method === 'storage.write.begin') return { transactionId: 'd'.repeat(64) }
    if (r.method === 'storage.write.chunk') return { receivedBytes: 0 }
    return null
  })
  await assert.rejects(
    c.storage.transact([{ type: 'set', key: 'x', value: 'hello' }]),
    /INVALID_RESPONSE/
  )
  assert.deepEqual(seen, ['storage.write.begin', 'storage.write.chunk', 'storage.write.abort'])
})

test('malformed download offset closes the snapshot and returns no partial value', async () => {
  const seen = []
  const c = createClient({
    session: 'a'.repeat(64),
    exchange: async (r) => {
      seen.push(r.method)
      if (r.method === 'storage.get')
        return {
          wireVersion: 3,
          requestId: r.requestId,
          ok: false,
          error: { code: 'BUDGET_EXCEEDED', message: 'BUDGET_EXCEEDED' }
        }
      let result = null
      if (r.method === 'storage.read.begin')
        result = { found: true, readId: 'c'.repeat(64), byteLength: 3 }
      if (r.method === 'storage.read.chunk') result = { offset: 1, data: 'ImEi' }
      return { wireVersion: 3, requestId: r.requestId, ok: true, result }
    }
  })
  await assert.rejects(c.storage.get('x'), /INVALID_RESPONSE/)
  assert.deepEqual(seen, [
    'storage.get',
    'storage.read.begin',
    'storage.read.chunk',
    'storage.read.close'
  ])
})

test('noncanonical base64 and malformed UTF-8 never return decoded storage data', async () => {
  for (const data of [
    'MB==',
    'bnVsbB==',
    'bnVsbA=',
    'b=nV',
    'bnVsbA===',
    'bnVs\nbA==',
    '____',
    'IsCwIg==',
    'Iu2ggCI='
  ]) {
    let closes = 0
    const c = createClient({
      session: 'a'.repeat(64),
      exchange: async (r) => {
        if (r.method === 'storage.get')
          return {
            wireVersion: 3,
            requestId: r.requestId,
            ok: false,
            error: { code: 'BUDGET_EXCEEDED', message: 'BUDGET_EXCEEDED' }
          }
        let result = null
        if (r.method === 'storage.read.begin')
          result = {
            found: true,
            readId: 'c'.repeat(64),
            byteLength: Buffer.from(data, 'base64').length || 1
          }
        if (r.method === 'storage.read.chunk') result = { offset: 0, data }
        if (r.method === 'storage.read.close') closes++
        return { wireVersion: 3, requestId: r.requestId, ok: true, result }
      }
    })
    await assert.rejects(c.storage.get('x'), /INVALID_RESPONSE/)
    assert.equal(closes, 1, data)
  }
})

import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { validateRequest, validateResponse, validateManifest, createClient } from '../src/index.mjs'
const fixtures = JSON.parse(
  readFileSync(new URL('../../../contracts/next/fixtures.json', import.meta.url), 'utf8')
)
for (const fixture of fixtures)
  test(fixture.name, () => {
    const raw = fixture.raw ?? JSON.stringify(fixture.request)
    if (fixture.valid) assert.doesNotThrow(() => validateRequest(raw))
    else assert.throws(() => validateRequest(raw))
  })
test('client keeps owner out of request and releases capacity on failure', async () => {
  const client = createClient({
    session: 'A'.repeat(32),
    exchange: async (request) => {
      assert.equal(Object.hasOwn(request, 'owner'), false)
      throw new Error('executor failed')
    }
  })
  for (let i = 0; i < 40; i++) await assert.rejects(client.ping(), /executor failed/)
})

test('client bounds inflight exchanges and accepts retry after settlement', async () => {
  const releases = []
  const client = createClient({
    session: 'A'.repeat(32),
    exchange: (request) =>
      new Promise((resolve) =>
        releases.push((result) =>
          resolve({ wireVersion: 3, requestId: request.requestId, ok: true, result })
        )
      )
  })
  const pending = Array.from({ length: 32 }, () => client.ping())
  await assert.rejects(client.ping(), /BUSY/)
  releases.forEach((resolve) => resolve(null))
  await Promise.all(pending)
  const retry = client.ping()
  releases.at(-1)(null)
  await retry
})

const responseFixtures = JSON.parse(
  readFileSync(new URL('../../../contracts/next/response-fixtures.json', import.meta.url), 'utf8')
)
for (const fixture of responseFixtures)
  test('response: ' + fixture.name, () => {
    const raw = fixture.raw ?? JSON.stringify(fixture.response)
    if (fixture.valid) assert.doesNotThrow(() => validateResponse(raw, fixture.requestId))
    else assert.throws(() => validateResponse(raw, fixture.requestId), /INVALID_RESPONSE/)
  })
test('client rejects mismatched response and preserves structured error code', async () => {
  const wrong = createClient({
    session: 'A'.repeat(32),
    exchange: async () => ({ wireVersion: 3, requestId: 'other', ok: true, result: null })
  })
  await assert.rejects(wrong.ping(), /INVALID_RESPONSE/)
  const denied = createClient({
    session: 'A'.repeat(32),
    exchange: async (request) => ({
      wireVersion: 3,
      requestId: request.requestId,
      ok: false,
      error: { code: 'PERMISSION_DENIED', message: 'denied' }
    })
  })
  await assert.rejects(denied.ping(), (error) => error.code === 'PERMISSION_DENIED')
})

const manifestFixtures = JSON.parse(
  readFileSync(new URL('../../../contracts/next/manifest-fixtures.json', import.meta.url), 'utf8')
)
for (const fixture of manifestFixtures)
  test('manifest: ' + fixture.name, () => {
    const raw = JSON.stringify(fixture.manifest)
    if (fixture.valid) assert.doesNotThrow(() => validateManifest(raw))
    else assert.throws(() => validateManifest(raw))
  })

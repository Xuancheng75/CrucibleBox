import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  validateManifest,
  validateRequest,
  validateResponse,
  validateResourceManifest,
  validateResourceCatalog,
  validateBackendFrame,
  createClient
} from '../src/index.mjs'
const fixtures = JSON.parse(
  readFileSync(new URL('../../../contracts/next2/fixtures.json', import.meta.url), 'utf8')
)
const validators = {
  manifest: validateManifest,
  request: validateRequest,
  resource: validateResourceManifest,
  catalog: validateResourceCatalog,
  response: (raw) => validateResponse(raw, 'r-1'),
  frame: validateBackendFrame
}
test('executor pause confirmation is optional and strictly boolean', () => {
  const request = {
    wireVersion: 4,
    requestId: 'pause-1',
    session: 'a'.repeat(64),
    target: { kind: 'platform' },
    method: 'tasks.controlState',
    params: { taskId: 'task-1', executionPaused: true }
  }
  assert.doesNotThrow(() => validateRequest(JSON.stringify(request)))
  assert.throws(() =>
    validateRequest(
      JSON.stringify({ ...request, params: { ...request.params, executionPaused: 'true' } })
    )
  )
})
for (const fixture of fixtures.cases) {
  test(fixture.name, () => {
    if (fixture.valid) assert.doesNotThrow(() => validators[fixture.schema](fixture.raw))
    else assert.throws(() => validators[fixture.schema](fixture.raw))
  })
}
test('client correlates responses and preserves domain errors', async () => {
  const requests = []
  const client = createClient({
    session: 'a'.repeat(64),
    exchange: async (request) => {
      requests.push(request)
      return { wireVersion: 4, requestId: request.requestId, ok: true, result: request.params }
    }
  })
  assert.deepEqual(await client.backend('new-method', { value: 42 }), { value: 42 })
  await client.service('transform', 'recognize', { ref: 'file-1' })
  assert.deepEqual(requests[1].target, { kind: 'service', id: 'transform' })
  const failed = createClient({
    session: 'a'.repeat(64),
    exchange: async (request) => ({
      wireVersion: 4,
      requestId: request.requestId,
      ok: false,
      error: { code: 'MODEL_MISSING', message: 'No model' }
    })
  })
  await assert.rejects(failed.backend('translate'), { code: 'MODEL_MISSING' })
})
test('client rejects wrong correlation and oversized frames', async () => {
  const client = createClient({
    session: 'a'.repeat(64),
    exchange: async () => ({ wireVersion: 4, requestId: 'other', ok: true, result: null })
  })
  await assert.rejects(client.backend('hello'), /INVALID_RESPONSE/)
  assert.throws(
    () => validateRequest(JSON.stringify({ ...fixtures.request, params: 'x'.repeat(65536) })),
    /BUDGET/
  )
})

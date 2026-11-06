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
    wireVersion: 5,
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
      return { wireVersion: 5, requestId: request.requestId, ok: true, result: request.params }
    }
  })
  assert.deepEqual(await client.backend('new-method', { value: 42 }), { value: 42 })
  await client.service('transform', 'recognize', { ref: 'file-1' })
  assert.deepEqual(requests[1].target, { kind: 'service', id: 'transform' })
  const failed = createClient({
    session: 'a'.repeat(64),
    exchange: async (request) => ({
      wireVersion: 5,
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
    exchange: async () => ({ wireVersion: 5, requestId: 'other', ok: true, result: null })
  })
  await assert.rejects(client.backend('hello'), /INVALID_RESPONSE/)
  assert.throws(
    () => validateRequest(JSON.stringify({ ...fixtures.request, params: 'x'.repeat(65536) })),
    /BUDGET/
  )
})
test('capability discovery uses a declared binding response', async () => {
  const binding = {
    providerId: 'sample-provider',
    providerVersion: '1.0.0',
    lifecycle: 'on-demand',
    export: { id: 'transform', version: '1.2.0', methods: ['recognize'], interchangeable: true }
  }
  const client = createClient({
    session: 'a'.repeat(64),
    exchange: async (request) => {
      assert.deepEqual(request.target, { kind: 'platform' })
      assert.equal(request.method, 'capabilities.resolve')
      assert.deepEqual(request.params, { id: 'transform' })
      return { wireVersion: 5, requestId: request.requestId, ok: true, result: binding }
    }
  })
  assert.deepEqual(await client.resolveCapability('transform'), binding)
  await assert.rejects(client.resolveCapability('invalid/id'), /INVALID_PLATFORM_CALL/)
})

test('wire cancellation addresses exactly one request or task', () => {
  const base = { kind: 'cancel', wireVersion: 5 }
  assert.doesNotThrow(() => validateBackendFrame(JSON.stringify({ ...base, taskId: 'task' })))
  assert.doesNotThrow(() => validateBackendFrame(JSON.stringify({ ...base, requestId: 'request' })))
  assert.throws(() => validateBackendFrame(JSON.stringify(base)), /INVALID_BACKEND_CANCEL/)
  assert.throws(
    () => validateBackendFrame(JSON.stringify({ ...base, taskId: 'task', requestId: 'request' })),
    /INVALID_BACKEND_CANCEL/
  )
})

test('file chunk iteration closes the stream when the consumer exits early', async () => {
  const commands = []
  const streamId = 'b'.repeat(64)
  const client = createClient({
    session: 'a'.repeat(64),
    exchange: async (request) => {
      commands.push(request.method)
      const result = {
        'files.stream.open': { streamId, bytes: 1 },
        'files.stream.read': { data: 'YQ==', offset: 0, nextOffset: 1, eof: true },
        'files.stream.close': true
      }[request.method]
      return { wireVersion: 5, requestId: request.requestId, ok: true, result }
    }
  })
  for await (const chunk of client.readFileChunks('owned-file')) {
    assert.equal(chunk.data, 'YQ==')
    break
  }
  assert.deepEqual(commands, ['files.stream.open', 'files.stream.read', 'files.stream.close'])
})

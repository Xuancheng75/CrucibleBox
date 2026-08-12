import { describe, expect, it } from 'vitest'
import {
  createPluginBackendRpcErrorResponse,
  createPluginBackendRpcRequest,
  createPluginBackendRpcResponse,
  validatePluginBackendRpcEnvelope
} from '../shared/plugin-backend-rpc'
import { createPluginWorkerEnvironment } from '../plugin-system/PluginSandbox'

const token = 'a'.repeat(43)

describe('plugin backend RPC v2', () => {
  it('creates and validates typed requests and responses', () => {
    const request = createPluginBackendRpcRequest(token, 'request-1', 'db.query', {
      sql: 'SELECT value FROM entries WHERE id = ?',
      params: [1]
    })
    expect(validatePluginBackendRpcEnvelope(request)).toEqual(request)
    expect(createPluginBackendRpcResponse(token, 'request-1', [{ value: 'ok' }])).toMatchObject({
      v: 2,
      kind: 'response',
      ok: true
    })
    expect(
      createPluginBackendRpcErrorResponse(token, 'request-1', {
        code: 'NOT_ALLOWED',
        message: 'denied'
      })
    ).toMatchObject({ ok: false, error: { code: 'NOT_ALLOWED' } })
  })

  it.each([
    { ...createPluginBackendRpcRequest(token, 'a', 'dialog.open', { type: 'file' }), v: 1 },
    {
      ...createPluginBackendRpcRequest(token, 'a', 'dialog.open', { type: 'file' }),
      token: 'short'
    },
    {
      ...createPluginBackendRpcRequest(token, 'a', 'dialog.open', { type: 'file' }),
      method: 'process.spawn'
    },
    {
      ...createPluginBackendRpcRequest(token, 'a', 'dialog.open', { type: 'file' }),
      params: { type: 'file', executable: 'calc.exe' }
    }
  ])('rejects malformed or capability-injecting messages', (message) => {
    expect(() => validatePluginBackendRpcEnvelope(message)).toThrow()
  })

  it('rejects oversized and non-JSON payloads before dispatch', () => {
    expect(() =>
      createPluginBackendRpcRequest(token, 'a', 'plugin.message', {
        message: 'x'.repeat(300 * 1024)
      })
    ).toThrow()
    expect(() =>
      validatePluginBackendRpcEnvelope({
        ...createPluginBackendRpcRequest(token, 'a', 'plugin.message', {}),
        params: { message: { callback: () => undefined } }
      })
    ).toThrow()
  })

  it('validates namespaced storage requests', () => {
    expect(
      createPluginBackendRpcRequest(token, 'storage-1', 'storage.set', {
        key: 'entry:2026-08-10',
        value: { title: 'hello' }
      })
    ).toMatchObject({ method: 'storage.set' })
    expect(
      createPluginBackendRpcRequest(token, 'storage-2', 'storage.list', { prefix: '' })
    ).toMatchObject({ method: 'storage.list' })
    expect(
      createPluginBackendRpcRequest(token, 'storage-batch', 'storage.batch', {
        mutations: [
          { type: 'set', key: 'entry:2026-08-10', value: { title: 'saved' } },
          { type: 'delete', key: 'draft:2026-08-10' }
        ]
      })
    ).toMatchObject({ method: 'storage.batch' })
    expect(() =>
      createPluginBackendRpcRequest(token, 'storage-3', 'storage.get', { key: 'bad\nkey' })
    ).toThrow()
    expect(() =>
      createPluginBackendRpcRequest(token, 'storage-4', 'storage.set', {
        key: 'key',
        value: undefined
      })
    ).toThrow()
    expect(() =>
      createPluginBackendRpcRequest(token, 'storage-5', 'storage.batch', {
        mutations: [{ type: 'delete', key: 'draft', value: 'not allowed' }] as never
      })
    ).toThrow()
  })

  it('passes only non-secret operating-system variables to workers', () => {
    expect(
      createPluginWorkerEnvironment({
        SystemRoot: 'C:\\Windows',
        TEMP: 'C:\\Temp',
        PATH: 'C:\\secret-bin',
        AWS_SECRET_ACCESS_KEY: 'secret',
        HOME: 'C:\\Users\\private'
      })
    ).toEqual({
      SystemRoot: 'C:\\Windows',
      TEMP: 'C:\\Temp',
      OPENBOX_BACKEND_RUNTIME: 'utility-process-v2'
    })
  })
})

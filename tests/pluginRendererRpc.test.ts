import { describe, expect, it } from 'vitest'
import {
  PLUGIN_RENDERER_RPC_BUDGET,
  PluginRendererRpcPendingRequests,
  PluginRendererRpcValidationError,
  createPluginRendererRpcEvent,
  createPluginRendererRpcFailureResponse,
  createPluginRendererRpcInit,
  createPluginRendererRpcReady,
  createPluginRendererRpcRequest,
  createPluginRendererRpcSuccessResponse,
  inspectPluginRendererRpcPayload,
  parsePluginRendererRpcEnvelope,
  validatePluginRendererRpcEnvelope,
  validatePluginRendererRpcResponse
} from '../shared/plugin-renderer-rpc'
import type { ToolboxTheme } from '../shared/types/theme.types'

const token = 'a-secure-frame-token-1234'

const theme: ToolboxTheme = {
  id: 'dark',
  name: 'Dark',
  mode: 'dark',
  tokens: {
    colorBg: '#000',
    colorBgLayout: '#000',
    colorBgContainer: '#111',
    colorBgElevated: '#222',
    colorPrimary: '#1677ff',
    colorPrimaryHover: '#4096ff',
    colorPrimaryBg: '#102a44',
    colorText: '#fff',
    colorTextSecondary: '#ccc',
    colorTextTertiary: '#999',
    colorBorder: '#555',
    colorBorderSecondary: '#333',
    colorSuccess: '#52c41a',
    colorSuccessBg: '#162312',
    colorWarning: '#faad14',
    colorWarningBg: '#2b2111',
    colorError: '#ff4d4f',
    colorErrorBg: '#2a1215',
    colorLink: '#1677ff',
    borderRadius: 6,
    fontFamily: 'system-ui'
  }
}

describe('plugin renderer RPC v1', () => {
  it('creates and validates the fixed handshake envelopes', () => {
    expect(createPluginRendererRpcInit(token)).toEqual({ v: 1, kind: 'init', token })
    expect(createPluginRendererRpcReady(token)).toEqual({ v: 1, kind: 'ready', token })
    expect(() => validatePluginRendererRpcEnvelope({ v: 2, kind: 'ready', token })).toThrow(
      'unsupported renderer RPC version'
    )
  })

  it('validates every v1 request method and its specific result', () => {
    const cases = [
      ['backend.send', { message: { ping: true } }, { value: { pong: true } }],
      ['notification.show', { title: 'Done', body: 'Ready' }, { shown: true }],
      ['config.update', { config: { enabled: true } }, { accepted: true }],
      ['theme.get', {}, { theme }],
      ['theme.list', {}, { themes: [theme] as ToolboxTheme[] }],
      ['theme.preview', { theme }, { applied: true }],
      ['theme.commit', {}, { committed: true }],
      ['theme.rollback', {}, { restored: true }],
      ['theme.set', { theme }, { applied: true }],
      ['dialog.confirm', { title: 'Delete', message: 'Continue?' }, { confirmed: false }],
      [
        'dialog.open',
        { type: 'file', multiple: true, extensions: ['pdf'] as string[] },
        { paths: ['C:/doc.pdf'] as string[] }
      ],
      ['layout.resize', { height: 640 }, { applied: true }]
    ] as const

    for (const [method, params, result] of cases) {
      const request = createPluginRendererRpcRequest(token, `req:${method}`, method, params)
      expect(validatePluginRendererRpcEnvelope(request)).toEqual(request)
      const response = createPluginRendererRpcSuccessResponse(
        token,
        request.requestId,
        method,
        result
      )
      expect(validatePluginRendererRpcResponse(response, method)).toEqual(response)
    }
  })

  it('validates every host event and forbids extra identity fields', () => {
    const events = [
      createPluginRendererRpcEvent(token, 'state.initialize', { config: { count: 1 }, theme }),
      createPluginRendererRpcEvent(token, 'state.configChanged', { config: { count: 2 } }),
      createPluginRendererRpcEvent(token, 'theme.changed', { theme }),
      createPluginRendererRpcEvent(token, 'backend.message', { message: ['ready'] }),
      createPluginRendererRpcEvent(token, 'host.filesDropped', { paths: ['C:/Docs/report.pdf'] }),
      createPluginRendererRpcEvent(token, 'host.dispose', {})
    ]
    for (const event of events) expect(validatePluginRendererRpcEnvelope(event)).toEqual(event)

    expect(() =>
      validatePluginRendererRpcEnvelope({
        v: 1,
        kind: 'request',
        token,
        requestId: 'req-identity',
        method: 'theme.get',
        params: { pluginId: 'victim' }
      })
    ).toThrow('unexpected field "pluginId"')
    expect(() => validatePluginRendererRpcEnvelope({ ...events[0], pluginId: 'victim' })).toThrow(
      'unexpected field "pluginId"'
    )
  })

  it('rejects unknown and malformed request fields with structured issues', () => {
    const parsed = parsePluginRendererRpcEnvelope({
      v: 1,
      kind: 'request',
      token,
      requestId: 'req-1',
      method: 'host.destroy',
      params: {}
    })
    expect(parsed).toEqual({
      ok: false,
      issue: {
        code: 'UNKNOWN_METHOD',
        message: 'unknown renderer RPC method',
        path: '$.method'
      }
    })

    expect(() =>
      createPluginRendererRpcRequest(token, 'req-2', 'layout.resize', { height: 99 })
    ).toThrow('height must be an integer')
    expect(() =>
      createPluginRendererRpcRequest(token, 'req-3', 'notification.show', {
        title: 'ok',
        body: 'ok',
        extra: true
      } as never)
    ).toThrow('unexpected field "extra"')
  })

  it('requires method-aware validation for successful response results', () => {
    const response = {
      v: 1,
      kind: 'response',
      token,
      requestId: 'req-1',
      ok: true,
      result: { confirmed: 'yes' }
    }
    expect(validatePluginRendererRpcEnvelope(response)).toEqual(response)
    expect(() => validatePluginRendererRpcResponse(response, 'dialog.confirm')).toThrow(
      'confirmed must be boolean'
    )

    expect(
      createPluginRendererRpcFailureResponse(token, 'req-1', {
        code: 'NOT_ALLOWED',
        message: 'permission denied',
        retryable: false,
        details: { method: 'theme.set' }
      })
    ).toMatchObject({ ok: false, error: { code: 'NOT_ALLOWED' } })
  })

  it('rejects getters, custom prototypes, sparse arrays, cycles and non-JSON values', () => {
    const getter = { v: 1, kind: 'ready', token } as Record<string, unknown>
    Object.defineProperty(getter, 'surprise', { enumerable: true, get: () => 'boom' })
    expect(() => validatePluginRendererRpcEnvelope(getter)).toThrow('accessors')

    expect(() => inspectPluginRendererRpcPayload(new Date())).toThrow('plain prototype')
    expect(() => inspectPluginRendererRpcPayload(new Array(2))).toThrow('sparse arrays')
    expect(() => inspectPluginRendererRpcPayload({ value: undefined })).toThrow('JSON-compatible')
    const cycle: { self?: unknown } = {}
    cycle.self = cycle
    expect(() => inspectPluginRendererRpcPayload(cycle)).toThrow('cyclic')
  })

  it('enforces byte, depth, node, array and string budgets without serialization', () => {
    expect(() =>
      inspectPluginRendererRpcPayload('x'.repeat(11), {
        ...PLUGIN_RENDERER_RPC_BUDGET,
        maxStringBytes: 10
      })
    ).toThrow('string exceeds byte budget')
    expect(() =>
      inspectPluginRendererRpcPayload([1, 2, 3], {
        ...PLUGIN_RENDERER_RPC_BUDGET,
        maxArrayLength: 2
      })
    ).toThrow('array exceeds item budget')
    expect(() =>
      inspectPluginRendererRpcPayload(
        { one: 1, two: 2 },
        {
          ...PLUGIN_RENDERER_RPC_BUDGET,
          maxObjectKeys: 1
        }
      )
    ).toThrow('object exceeds key budget')
    expect(() =>
      inspectPluginRendererRpcPayload(
        { child: { leaf: true } },
        {
          ...PLUGIN_RENDERER_RPC_BUDGET,
          maxDepth: 1
        }
      )
    ).toThrow('depth budget')
    expect(() =>
      inspectPluginRendererRpcPayload([1, 2], {
        ...PLUGIN_RENDERER_RPC_BUDGET,
        maxNodes: 2
      })
    ).toThrow('node budget')
    expect(() =>
      inspectPluginRendererRpcPayload('é', {
        ...PLUGIN_RENDERER_RPC_BUDGET,
        maxSerializedBytes: 3
      })
    ).toThrow('serialized byte budget')
  })

  it('bounds pending concurrency and rejects duplicate request ids', () => {
    const pending = new PluginRendererRpcPendingRequests(2)
    pending.add('one')
    pending.add('two')
    expect(pending.size).toBe(2)
    expect(() => pending.add('three')).toThrow('pending request limit reached')
    expect(() => pending.add('one')).toThrow('already pending')
    expect(pending.delete('one')).toBe(true)
    pending.add('three')
    pending.clear()
    expect(pending.size).toBe(0)
  })

  it('uses validation errors with stable machine-readable codes', () => {
    try {
      createPluginRendererRpcReady('short')
      expect.unreachable()
    } catch (error) {
      expect(error).toBeInstanceOf(PluginRendererRpcValidationError)
      expect((error as PluginRendererRpcValidationError).issue).toEqual({
        code: 'INVALID_TOKEN',
        message: 'expected a string with 16-128 characters',
        path: '$.token'
      })
    }
  })
})

import { describe, expect, it } from 'vitest'
import {
  createPluginBackendRpcRequest,
  validatePluginBackendRpcEnvelope
} from '../shared/plugin-backend-rpc'

describe('backend configuration RPC', () => {
  it('accepts JSON objects and rejects malformed config before dispatch', () => {
    const request = createPluginBackendRpcRequest(
      'a'.repeat(43),
      'config-1',
      'lifecycle.configure',
      {
        config: { maxItems: 20, monitor: false, label: '用户配置' }
      }
    )
    expect(validatePluginBackendRpcEnvelope(request)).toEqual(request)
    for (const params of [{ config: [] }, { config: null }, { config: {}, extra: true }]) {
      expect(() => validatePluginBackendRpcEnvelope({ ...request, params })).toThrow()
    }
  })
})

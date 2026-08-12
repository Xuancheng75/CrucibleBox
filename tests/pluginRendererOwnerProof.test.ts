import { describe, expect, it } from 'vitest'
import { PluginRendererRequestOwnerProof } from '../plugin-system/PluginRendererRequestOwnerProof'

describe('plugin renderer request owner proof', () => {
  it('round-trips a positive WebContents id', () => {
    const proof = new PluginRendererRequestOwnerProof(Buffer.alloc(32, 1))
    expect(proof.verify(proof.create(42))).toBe(42)
  })

  it('rejects forged, malformed and cross-process proofs', () => {
    const first = new PluginRendererRequestOwnerProof(Buffer.alloc(32, 1))
    const second = new PluginRendererRequestOwnerProof(Buffer.alloc(32, 2))
    const valid = first.create(7)
    expect(second.verify(valid)).toBeUndefined()
    expect(first.verify(`${valid.slice(0, -1)}0`)).toBeUndefined()
    expect(first.verify('7.not-a-signature')).toBeUndefined()
    expect(first.verify(undefined)).toBeUndefined()
  })

  it('rejects invalid WebContents ids', () => {
    const proof = new PluginRendererRequestOwnerProof(Buffer.alloc(32, 1))
    expect(() => proof.create(0)).toThrow('positive safe integer')
    expect(() => proof.create(Number.MAX_SAFE_INTEGER + 1)).toThrow('positive safe integer')
  })
})

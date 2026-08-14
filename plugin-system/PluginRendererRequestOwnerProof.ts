// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { createHmac, randomBytes, timingSafeEqual } from 'node:crypto'

export const PLUGIN_RENDERER_OWNER_PROOF_HEADER = 'x-openbox-renderer-owner'

const PROOF_PATTERN = /^(\d{1,10})\.([a-f0-9]{64})$/

export class PluginRendererRequestOwnerProof {
  private readonly secret: Buffer

  constructor(secret: Buffer = randomBytes(32)) {
    if (secret.byteLength < 32) throw new Error('Renderer owner proof secret is too short')
    this.secret = Buffer.from(secret)
  }

  create(webContentsId: number): string {
    if (!Number.isSafeInteger(webContentsId) || webContentsId <= 0) {
      throw new Error('webContentsId must be a positive safe integer')
    }
    return `${webContentsId}.${this.sign(webContentsId)}`
  }

  verify(proof: string | null | undefined): number | undefined {
    if (!proof) return undefined
    const match = PROOF_PATTERN.exec(proof)
    if (!match) return undefined
    const webContentsId = Number(match[1])
    if (!Number.isSafeInteger(webContentsId) || webContentsId <= 0) return undefined
    const expected = Buffer.from(this.sign(webContentsId), 'hex')
    const actual = Buffer.from(match[2], 'hex')
    return actual.byteLength === expected.byteLength && timingSafeEqual(actual, expected)
      ? webContentsId
      : undefined
  }

  private sign(webContentsId: number): string {
    return createHmac('sha256', this.secret).update(String(webContentsId)).digest('hex')
  }
}

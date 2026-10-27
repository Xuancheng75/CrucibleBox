import {
  createClient,
  validateManifest,
  type Request,
  type Manifest,
  type NativeBackend
} from '../src/index.mjs'
const client = createClient({
  session: 'a'.repeat(64),
  exchange: async (request: Request) => ({
    wireVersion: 4,
    requestId: request.requestId,
    ok: true,
    result: null
  })
})
void client.backend('any.future.business.method', { value: 42 })
const manifest: Manifest = validateManifest('{}')
const native: NativeBackend = {
  kind: 'native',
  entry: 'bin/backend.exe',
  os: 'windows',
  arch: 'x86_64',
  trust: 'trusted-local-code'
}
void manifest
void native
const forged: Request = {
  wireVersion: 4,
  requestId: 'r',
  session: 'a'.repeat(64),
  // @ts-expect-error The renderer does not choose a service provider or owner.
  target: { kind: 'service', id: 'transform', providerId: 'other' },
  method: 'read',
  params: null
}
// @ts-expect-error Native code requires its explicit trust declaration.
const missingTrust: NativeBackend = {
  kind: 'native',
  entry: 'backend.exe',
  os: 'windows',
  arch: 'x86_64'
}
void forged
void missingTrust

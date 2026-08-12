import { createHash, createPrivateKey, createPublicKey, sign, verify } from 'node:crypto'

export function canonicalJson(value) {
  if (value === null || typeof value === 'boolean' || typeof value === 'string') {
    return JSON.stringify(value)
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new TypeError('Canonical JSON only supports finite numbers')
    return JSON.stringify(value)
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`
  if (typeof value === 'object') {
    const entries = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
    return `{${entries.join(',')}}`
  }
  throw new TypeError(`Canonical JSON does not support ${typeof value}`)
}

export function sha256(content) {
  return createHash('sha256').update(content).digest('hex')
}

export function signPluginArtifactManifest(manifest, privateKey) {
  const key = privateKey?.type === 'private' ? privateKey : createPrivateKey(privateKey)
  if (key.asymmetricKeyType !== 'ed25519') throw new Error('Plugin signing key must be Ed25519')
  return sign(null, Buffer.from(canonicalJson(manifest)), key).toString('base64')
}

export function verifyPluginArtifactManifest(manifest, signature, publicKey) {
  const key = publicKey?.type === 'public' ? publicKey : createPublicKey(publicKey)
  if (key.asymmetricKeyType !== 'ed25519')
    throw new Error('Plugin verification key must be Ed25519')
  return verify(null, Buffer.from(canonicalJson(manifest)), key, Buffer.from(signature, 'base64'))
}

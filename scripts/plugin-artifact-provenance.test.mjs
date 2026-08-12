import { generateKeyPairSync } from 'node:crypto'
import test from 'node:test'
import assert from 'node:assert/strict'
import {
  canonicalJson,
  signPluginArtifactManifest,
  verifyPluginArtifactManifest
} from './plugin-artifact-provenance.mjs'

test('canonical JSON is stable across object insertion order', () => {
  assert.equal(canonicalJson({ z: 1, a: { d: 2, b: 1 } }), '{"a":{"b":1,"d":2},"z":1}')
})

test('Ed25519 plugin manifest signatures reject tampering and the wrong key', () => {
  const signer = generateKeyPairSync('ed25519')
  const stranger = generateKeyPairSync('ed25519')
  const manifest = { schemaVersion: 1, plugins: [{ id: 'diary', sha256: 'abc' }] }
  const signature = signPluginArtifactManifest(manifest, signer.privateKey)

  assert.equal(verifyPluginArtifactManifest(manifest, signature, signer.publicKey), true)
  assert.equal(
    verifyPluginArtifactManifest(
      { ...manifest, plugins: [{ id: 'diary', sha256: 'changed' }] },
      signature,
      signer.publicKey
    ),
    false
  )
  assert.equal(verifyPluginArtifactManifest(manifest, signature, stranger.publicKey), false)
})

test('plugin signing rejects non-Ed25519 keys', () => {
  const rsa = generateKeyPairSync('rsa', { modulusLength: 2048 })
  assert.throws(() => signPluginArtifactManifest({ schemaVersion: 1 }, rsa.privateKey), /Ed25519/)
})

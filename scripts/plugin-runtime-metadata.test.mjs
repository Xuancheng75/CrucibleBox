import { test } from 'node:test'
import assert from 'node:assert/strict'
import { artifactRuntimeMetadata } from './plugin-runtime-metadata.mjs'
const manifest = {
  id: 'example',
  version: '1.0.0',
  displayName: 'Example',
  manifestVersion: 5,
  sdkApiVersion: 5,
  wireVersion: 3,
  dataSchemaVersion: 1,
  renderer: 'dist/renderer.js',
  permissions: []
}
test('Next renderer-only package never advertises a backend or legacy API', () => {
  assert.deepEqual(artifactRuntimeMetadata(manifest), {
    manifestVersion: 5,
    backend: false,
    backendApiVersion: null,
    rendererApiVersion: 5,
    sdkApiVersion: 5,
    wireVersion: 3,
    dataSchemaVersion: 1
  })
  assert.equal(
    artifactRuntimeMetadata({ ...manifest, backend: 'dist/main.js' }).backendApiVersion,
    5
  )
})
test('malformed Next fields fail closed instead of falling back to legacy metadata', () => {
  assert.throws(() => artifactRuntimeMetadata({ ...manifest, wireVersion: 2 }))
  assert.throws(() => artifactRuntimeMetadata({ ...manifest, backend: false }))
  assert.equal(
    artifactRuntimeMetadata({ manifestVersion: 2, backend: false, rendererApiVersion: 3 })
      .rendererApiVersion,
    3
  )
})

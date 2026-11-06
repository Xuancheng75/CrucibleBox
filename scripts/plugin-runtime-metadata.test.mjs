import { test } from 'node:test'
import assert from 'node:assert/strict'
import { artifactRuntimeMetadata } from './plugin-runtime-metadata.mjs'
const manifest = {
  manifestVersion: 7,
  sdkApiVersion: 7,
  id: 'sample-independent',
  version: '1.0.0',
  displayName: 'Sample',
  description: 'Fixture',
  publisher: { id: 'local', displayName: 'Local' },
  minHostVersion: '2.2.0-beta.1',
  sdk: { min: 7, maxExclusive: 8 },
  renderer: 'dist/renderer.js',
  permissions: [],
  background: false,
  contributes: { commands: [] },
  exports: [],
  dependencies: [],
  resources: [],
  files: [{ path: 'dist/renderer.js', bytes: 1, sha256: '0'.repeat(64) }]
}
test('API 7 renderer-only metadata uses wire 5 and never invents a backend', () => {
  assert.deepEqual(artifactRuntimeMetadata(manifest), {
    manifestVersion: 7,
    backend: false,
    backendApiVersion: null,
    rendererApiVersion: 7,
    sdkApiVersion: 7,
    wireVersion: 5,
    dataSchemaVersion: 1
  })
})
test('distribution rejects old and malformed manifests', () => {
  for (const version of [1, 2, 3, 4, 5, 6])
    assert.throws(
      () => artifactRuntimeMetadata({ ...manifest, manifestVersion: version }),
      /API_VERSION_UNSUPPORTED/
    )
  assert.throws(() => artifactRuntimeMetadata({ ...manifest, backend: false }))
})

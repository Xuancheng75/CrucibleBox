import { test } from 'node:test'
import assert from 'node:assert/strict'
import { artifactRuntimeMetadata } from './plugin-runtime-metadata.mjs'
const manifest = {
  manifestVersion: 6,
  sdkApiVersion: 6,
  id: 'sample-independent',
  version: '1.0.0',
  displayName: 'Sample',
  description: 'Fixture',
  publisher: { id: 'local', displayName: 'Local' },
  minHostVersion: '2.1.0-beta.4',
  sdk: { min: 6, maxExclusive: 7 },
  renderer: 'dist/renderer.js',
  permissions: [],
  background: false,
  contributes: { commands: [] },
  exports: [],
  dependencies: [],
  resources: [],
  files: [{ path: 'dist/renderer.js', bytes: 1, sha256: '0'.repeat(64) }]
}
test('API6 renderer-only metadata uses wire4 and never invents a backend', () => {
  assert.deepEqual(artifactRuntimeMetadata(manifest), {
    manifestVersion: 6,
    backend: false,
    backendApiVersion: null,
    rendererApiVersion: 6,
    sdkApiVersion: 6,
    wireVersion: 4,
    dataSchemaVersion: 1
  })
})
test('distribution rejects old and malformed manifests', () => {
  for (const version of [1, 2, 3, 4, 5])
    assert.throws(
      () => artifactRuntimeMetadata({ ...manifest, manifestVersion: version }),
      /API_VERSION_UNSUPPORTED/
    )
  assert.throws(() => artifactRuntimeMetadata({ ...manifest, backend: false }))
})

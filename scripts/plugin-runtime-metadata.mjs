import { validateManifest as validateNext2Manifest } from '../packages/cruciblebox-next2-api/src/index.mjs'
export function artifactRuntimeMetadata(manifest) {
  if (manifest.manifestVersion === 6) {
    const checked = validateNext2Manifest(JSON.stringify(manifest))
    const backend = checked.backend !== undefined
    return {
      manifestVersion: 6,
      backend,
      backendApiVersion: backend ? 6 : null,
      rendererApiVersion: 6,
      sdkApiVersion: 6,
      wireVersion: 4,
      dataSchemaVersion: 1
    }
  }
  throw new Error('API_VERSION_UNSUPPORTED: only Manifest/API 6 packages can be distributed')
}

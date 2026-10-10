import { validateManifest } from '../packages/cruciblebox-next-api/src/index.mjs'
export function artifactRuntimeMetadata(manifest) {
  if (manifest.manifestVersion === 5) {
    const checked = validateManifest(JSON.stringify(manifest))
    const backend = typeof checked.backend === 'string'
    return {
      manifestVersion: 5,
      backend,
      backendApiVersion: backend ? checked.sdkApiVersion : null,
      rendererApiVersion: checked.sdkApiVersion,
      sdkApiVersion: checked.sdkApiVersion,
      wireVersion: checked.wireVersion,
      dataSchemaVersion: checked.dataSchemaVersion
    }
  }
  return {
    manifestVersion: manifest.manifestVersion ?? 1,
    backend: manifest.backend !== false,
    backendApiVersion: manifest.backend === false ? null : (manifest.backendApiVersion ?? 1),
    rendererApiVersion: manifest.rendererApiVersion ?? 1
  }
}

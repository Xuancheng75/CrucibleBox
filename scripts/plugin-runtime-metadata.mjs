import { contract, validateManifest } from '../packages/cruciblebox-next2-api/src/index.mjs'
export function artifactRuntimeMetadata(manifest) {
  const { manifestVersion, sdkApiVersion, wireVersion, dataSchemaVersion } = contract.versions
  if (manifest.manifestVersion !== manifestVersion)
    throw new Error(`API_VERSION_UNSUPPORTED: requires Manifest ${manifestVersion}`)
  const checked = validateManifest(JSON.stringify(manifest))
  const backend = checked.backend !== undefined
  return {
    manifestVersion,
    backend,
    backendApiVersion: backend ? sdkApiVersion : null,
    rendererApiVersion: checked.renderer === undefined ? null : sdkApiVersion,
    sdkApiVersion,
    wireVersion,
    dataSchemaVersion
  }
}

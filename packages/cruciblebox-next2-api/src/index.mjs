import { contract } from './generated.mjs'
import { parse, matches, unique, validRange, validateInventory } from './schema.mjs'
export { contract } from './generated.mjs'
export { compatible } from './schema.mjs'

export function validateManifest(raw) {
  const manifest = parse(raw, 'Manifest', contract.budget.manifestBytes)
  validateInventory(manifest.files)
  if (manifest.files.reduce((sum, file) => sum + file.bytes, 0) > contract.budget.packageBytes)
    throw Error('BUDGET_EXCEEDED')
  for (const [name, list] of [
    ['exports', manifest.exports],
    ['dependencies', manifest.dependencies],
    ['resources', manifest.resources]
  ]) {
    unique(list, (item) => item.id)
    for (const item of list) {
      if (name === 'exports') unique(item.methods, (method) => method)
      else validRange(item.version)
    }
  }
  unique(manifest.contributes.commands, (command) => command.id)
  const files = new Set(manifest.files.map((file) => file.path))
  for (const entry of [
    manifest.renderer,
    manifest.backend?.entry,
    manifest.contributes.navigation?.icon,
    ...manifest.resources.map((resource) => resource.catalog)
  ].filter(Boolean))
    if (!files.has(entry)) throw Error('UNDECLARED_ENTRY')
  if (manifest.backend?.kind === 'native' && !manifest.backend.entry.endsWith('.exe'))
    throw Error('INVALID_NATIVE_ENTRY')
  if (manifest.backend?.kind === 'js' && !manifest.backend.entry.endsWith('.js'))
    throw Error('INVALID_JS_ENTRY')
  if (!manifest.renderer.endsWith('.js')) throw Error('INVALID_RENDERER_ENTRY')
  if (!manifest.backend && (manifest.background || manifest.exports.length))
    throw Error('BACKEND_REQUIRED')
  if (manifest.dependencies.some((item) => item.providerId === manifest.id))
    throw Error('SELF_DEPENDENCY')
  return manifest
}
export function validateResourceManifest(raw) {
  const manifest = parse(raw, 'ResourceManifest', contract.budget.resourceManifestBytes)
  validateInventory(manifest.files)
  validRange(manifest.pluginVersion)
  if (manifest.files.reduce((sum, entry) => sum + entry.bytes, 0) > contract.budget.resourceBytes)
    throw Error('BUDGET_EXCEEDED')
  return manifest
}
export function validateResourceCatalog(raw) {
  const catalog = parse(raw, 'ResourceCatalog', contract.budget.resourceManifestBytes)
  unique(catalog.releases, (release) => release.id)
  for (const release of catalog.releases) {
    validateResourceManifest(JSON.stringify(release.manifest))
    unique(release.downloads, (file) => file.path)
    const files = new Set(release.manifest.files.map((file) => file.path))
    if (
      files.size !== release.downloads.length ||
      release.downloads.some((file) => !files.has(file.path))
    )
      throw Error('RESOURCE_DOWNLOAD_INVENTORY_MISMATCH')
  }
  return catalog
}
export function validateRequest(raw) {
  const request = parse(raw, 'Request')
  if (request.target.kind === 'platform') {
    const method = contract.methods[request.method]
    if (!method || !matches(method.params, request.params)) throw Error('INVALID_PLATFORM_CALL')
    if (request.method === 'tasks.start' && request.params.target.kind === 'platform')
      throw Error('INVALID_TASK_TARGET')
  }
  return request
}
export function validateResponse(raw, requestId) {
  const response = parse(raw, 'Response')
  if (response.requestId !== requestId) throw Error('INVALID_RESPONSE')
  return response
}
export function validateBackendFrame(raw) {
  const frame = parse(raw, 'BackendFrame')
  if (frame.request) validateRequest(JSON.stringify(frame.request))
  if (frame.response) validateResponse(JSON.stringify(frame.response), frame.response.requestId)
  return frame
}
export function createClient({ session, exchange }) {
  let sequence = 0
  const call = async (target, method, params = {}) => {
    const request = validateRequest(
      JSON.stringify({
        wireVersion: 4,
        requestId: `sdk-${++sequence}`,
        session,
        target,
        method,
        params
      })
    )
    const response = validateResponse(JSON.stringify(await exchange(request)), request.requestId)
    if (!response.ok)
      throw Object.assign(new Error(response.error.message), { code: response.error.code })
    if (target.kind === 'platform' && !matches(contract.methods[method].result, response.result))
      throw Error('INVALID_RESPONSE')
    return response.result
  }
  return {
    platform: (method, params) => call({ kind: 'platform' }, method, params),
    backend: (method, params) => call({ kind: 'backend' }, method, params),
    service: (id, method, params) => call({ kind: 'service', id }, method, params)
  }
}

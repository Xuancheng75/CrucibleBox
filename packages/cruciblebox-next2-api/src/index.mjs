import semver from 'semver'
import { contract } from './generated.mjs'
import {
  parse,
  matches,
  unique,
  validRange,
  validateInventory,
  validateBudget,
  normalize
} from './schema.mjs'
export { contract } from './generated.mjs'
export { compatible } from './schema.mjs'

export function validateThemeDesign(raw) {
  const design = parse(raw, 'ThemeDesign')
  const assets = new Set(design.assets ?? [])
  unique(design.layers ?? [], (layer) => layer.id)
  unique(design.fonts ?? [], (font) => font.family.toLowerCase())
  for (const layer of design.layers ?? []) {
    if (layer.type === 'image' && (!layer.assetId || !assets.has(layer.assetId)))
      throw Error('THEME_ASSET_UNDECLARED')
    if (layer.type === 'text' && typeof layer.text !== 'string')
      throw Error('THEME_LAYER_TEXT_REQUIRED')
    if (layer.type === 'shape' && layer.shape === 'polygon' && !layer.points)
      throw Error('THEME_POLYGON_POINTS_REQUIRED')
  }
  for (const font of design.fonts ?? [])
    if (!assets.has(font.assetId)) throw Error('THEME_ASSET_UNDECLARED')
  return design
}

function validateCapabilitySchema(rule) {
  for (const [min, max] of [
    ['minimum', 'maximum'],
    ['minLength', 'maxLength'],
    ['minItems', 'maxItems']
  ]) {
    if (rule[min] > rule[max]) throw Error('INVALID_CAPABILITY_SCHEMA')
  }
  if (rule.properties) {
    if (rule.required?.some((key) => !Object.hasOwn(rule.properties, key)))
      throw Error('INVALID_CAPABILITY_SCHEMA')
    for (const child of Object.values(rule.properties)) validateCapabilitySchema(child)
  }
  if (rule.items) validateCapabilitySchema(rule.items)
}

export function validateManifest(raw) {
  const manifest = parse(raw, 'Manifest', contract.budget.manifestBytes)
  if (manifest.id === contract.capabilities.builtins.providerId) throw Error('RESERVED_PROVIDER_ID')
  const features =
    manifest.tools !== undefined ||
    manifest.permissions.some((permission) =>
      ['process:run', 'network:download'].includes(permission)
    ) ||
    manifest.contributes.commands.some(
      (command) => command.placements !== undefined || command.keywords !== undefined
    )
  const minimum =
    manifest.runtimeCapabilities !== undefined ||
    manifest.runtimeRequirements !== undefined ||
    manifest.permissions.some((permission) =>
      ['runtime:execute', 'native:publish', 'files:write'].includes(permission)
    ) ||
    manifest.dependencies.some((item) => item.id.startsWith('host-'))
      ? contract.versions.completionHostVersion
      : features
        ? contract.versions.featuresHostVersion
        : contract.versions.minHostVersion
  if (semver.lt(manifest.minHostVersion, minimum)) throw Error('HOST_VERSION_UNDERSTATED')
  if (manifest.dataMigration) validRange(manifest.dataMigration.fromPluginVersion)
  const tools = manifest.tools ?? []
  unique(tools, (tool) => tool.id)
  const runtimes = manifest.runtimeCapabilities ?? []
  unique(runtimes, (runtime) => runtime.capabilityId)
  const requirements = manifest.runtimeRequirements ?? []
  unique(requirements, (runtime) => runtime.capabilityId)
  for (const requirement of requirements) {
    validRange(requirement.version)
    if (!manifest.dependencies.some((dependency) => dependency.id === requirement.capabilityId))
      throw Error('RUNTIME_DEPENDENCY_UNDECLARED')
  }

  if (
    runtimes.some(
      (runtime) =>
        !tools.some((tool) => tool.id === runtime.toolId) ||
        !manifest.exports.some(
          (item) =>
            item.id === runtime.capabilityId &&
            item.methods.some(
              (method) => (typeof method === 'string' ? method : method.name) === 'run'
            )
        )
    )
  )
    throw Error('INVALID_RUNTIME_CAPABILITY')
  if (
    tools.length &&
    (!manifest.permissions.includes('process:run') ||
      tools.some(
        (tool) =>
          !tool.entry.toLowerCase().endsWith('.exe') ||
          !manifest.resources.some((resource) => resource.id === tool.resourceId)
      ))
  )
    throw Error('INVALID_PROCESS_TOOL_DECLARATION')
  if (
    manifest.backgroundIntervalMs !== undefined &&
    (!manifest.background || manifest.backend?.kind !== 'js')
  )
    throw Error('BACKGROUND_INTERVAL_REQUIRES_JS_SERVICE')
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
      if (name === 'exports') {
        const events = item.events ?? []
        if (!item.methods.length && !events.length) throw Error('EMPTY_CAPABILITY')
        unique(events, (event) => event.name)
        for (const event of events) validateCapabilitySchema(event.payload)
        if (
          new TextEncoder().encode(JSON.stringify(item)).length >
          contract.budget.capabilityDescriptorBytes
        )
          throw Error('CAPABILITY_DESCRIPTOR_BUDGET_EXCEEDED')
        validateBudget(
          item,
          contract.budget.capabilityDescriptorNodes,
          'CAPABILITY_DESCRIPTOR_BUDGET_EXCEEDED'
        )
        unique(item.methods, (method) => (typeof method === 'string' ? method : method.name))
        for (const method of item.methods) {
          if (typeof method === 'string') continue
          validateCapabilitySchema(method.params)
          validateCapabilitySchema(method.result)
        }
      } else validRange(item.version)
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
  if (manifest.renderer && !manifest.renderer.endsWith('.js')) throw Error('INVALID_RENDERER_ENTRY')
  if (
    !manifest.renderer &&
    (!manifest.backend ||
      manifest.contributes.navigation ||
      manifest.contributes.fileHandlers?.length)
  )
    throw Error('RENDERER_REQUIRED')
  if (
    !manifest.backend &&
    (manifest.background ||
      manifest.exports.length ||
      manifest.contributes.commands.length ||
      tools.length)
  )
    throw Error('BACKEND_REQUIRED')
  if (manifest.dependencies.some((item) => item.providerId === manifest.id))
    throw Error('SELF_DEPENDENCY')
  return manifest
}
export function validateResourceManifest(raw) {
  const manifest = parse(raw, 'ResourceManifest', contract.budget.resourceManifestBytes)
  validateInventory(manifest.files)
  if (manifest.runtimeIdentity && (manifest.kind !== 'runtime' || !manifest.containsNativeCode))
    throw Error('INVALID_RUNTIME_RESOURCE')
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
    normalize(method.params, request.params)
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
  if (frame.kind === 'cancel' && (frame.taskId === undefined) === (frame.requestId === undefined))
    throw Error('INVALID_BACKEND_CANCEL')
  if (frame.request) validateRequest(JSON.stringify(frame.request))
  if (frame.response) validateResponse(JSON.stringify(frame.response), frame.response.requestId)
  return frame
}
export function createClient({ session, exchange }) {
  let sequence = 0
  const call = async (target, method, params = {}, signal) => {
    if (signal?.aborted) throw Object.assign(Error('CANCELLED'), { code: 'CANCELLED' })
    const request = validateRequest(
      JSON.stringify({
        wireVersion: contract.versions.wireVersion,
        requestId: `sdk-${++sequence}`,
        session,
        target,
        method,
        params
      })
    )
    const response = validateResponse(
      JSON.stringify(await exchange(request, signal)),
      request.requestId
    )
    if (!response.ok)
      throw Object.assign(new Error(response.error.message), { code: response.error.code })
    if (target.kind === 'platform' && !matches(contract.methods[method].result, response.result))
      throw Error('INVALID_RESPONSE')
    return response.result
  }
  const platform = (method, params) => call({ kind: 'platform' }, method, params)
  return {
    platform,
    resolveCapability: (id) => platform('capabilities.resolve', { id }),
    capabilities: {
      resolve: (id) => platform('capabilities.resolve', { id }),
      describe: (id) => platform('capabilities.describe', { id }),
      list: (options = {}) => platform('capabilities.list', options)
    },
    writeLog: (entry) => platform('logging.write', entry),
    files: {
      info: (ref) => platform('files.info', { ref }),
      list: (ref) => platform('files.list', { ref }),
      writeResult: (options) => platform('files.writeResult', options),
      child: (ref, path) => platform('files.child', { ref, path }),
      createDirectory: (ref, name) => platform('files.directory.create', { ref, name }),
      walk: (ref, options = {}) => platform('files.directory.walk', { ...options, ref }),
      copy: (options) => platform('files.copy', options),
      move: (options) => platform('files.move', options),
      remove: (ref, options = {}) => platform('files.remove', { ...options, ref }),
      temporary: (options) => platform('files.temporary', options),
      releaseTemporary: (ref) => platform('files.temporary.release', { ref }),
      write: {
        open: (options) => platform('files.write.open', options),
        append: (streamId, data) => platform('files.write.append', { streamId, data }),
        commit: (streamId) => platform('files.write.commit', { streamId }),
        abort: (streamId) => platform('files.write.abort', { streamId })
      },
      watch: {
        open: (ref, options = {}) => platform('files.watch', { ...options, ref }),
        poll: (watchId) => platform('files.watch.poll', { watchId }),
        close: (watchId) => platform('files.watch.close', { watchId })
      }
    },
    network: { download: (options) => platform('network.download', options) },
    process: { run: (options) => platform('process.run', options) },
    runtime: {
      list: () => platform('runtime.list', {}),
      resolve: (id) => platform('runtime.resolve', { id })
    },
    backend: (method, params, options) =>
      call({ kind: 'backend' }, method, params, options?.signal),
    service: (id, method, params, options) =>
      call({ kind: 'service', id }, method, params, options?.signal),
    events: {
      subscribe: (topic) => platform('events.subscribe', topic),
      publish: (event) => platform('events.publish', event),
      poll: (subscriptionId) => platform('events.poll', { subscriptionId }),
      unsubscribe: (subscriptionId) => platform('events.unsubscribe', { subscriptionId })
    },
    async *readFileChunks(ref, options) {
      const checkCancelled = () => {
        if (options?.signal?.aborted) throw Object.assign(Error('CANCELLED'), { code: 'CANCELLED' })
      }
      checkCancelled()
      const { streamId, bytes } = await platform('files.stream.open', { ref })
      try {
        let offset = 0
        for (;;) {
          checkCancelled()
          const length = contract.budget.fileStreamChunkBytes
          const chunk = await platform('files.stream.read', { streamId, offset, length })
          const next = Math.min(offset + length, bytes)
          if (
            chunk.offset !== offset ||
            chunk.nextOffset !== next ||
            chunk.eof !== (next === bytes)
          )
            throw Error('INVALID_RESPONSE')
          checkCancelled()
          yield chunk
          if (chunk.eof) return
          offset = chunk.nextOffset
        }
      } finally {
        await platform('files.stream.close', { streamId })
      }
    }
  }
}

export function defineProvider(activate) {
  return (context) => activate({ ...context, api: createClient(context) })
}

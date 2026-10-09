import { contract } from './generated.mjs'
export { contract, contractSha256 } from './generated.mjs'
function canonicalNumeric(value) {
  return /^(0|[1-9][0-9]*)$/.test(value) && Number(value) <= Number.MAX_SAFE_INTEGER
}
function semver(value) {
  const match = value.match(/^([0-9]+)\.([0-9]+)\.([0-9]+)(?:-([A-Za-z0-9.-]+))?$/)
  return (
    !!match &&
    match.slice(1, 4).every(canonicalNumeric) &&
    (!match[4] ||
      match[4].split('.').every((id) => !!id && (!/^[0-9]+$/.test(id) || canonicalNumeric(id))))
  )
}
function shape(schema, value) {
  if (schema.oneOf && schema.oneOf.filter((child) => shape(child, value)).length !== 1) return false
  if (schema.type === 'boolean') return typeof value === 'boolean'
  if (schema.type === 'null') return value === null
  if (schema.type === 'integer' || schema.type === 'number')
    return (
      typeof value === 'number' &&
      Number.isFinite(value) &&
      (schema.type !== 'integer' || Number.isSafeInteger(value)) &&
      value >= (schema.minimum ?? -Infinity) &&
      value <= (schema.maximum ?? Infinity)
    )
  if ('const' in schema && value !== schema.const) return false
  if (schema.enum && !schema.enum.includes(value)) return false
  if (schema.type === 'object') {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return false
    if ((schema.required ?? []).some((key) => !Object.hasOwn(value, key))) return false
    if (
      schema.additionalProperties === false &&
      Object.keys(value).some((key) => !Object.hasOwn(schema.properties ?? {}, key))
    )
      return false
    return Object.entries(schema.properties ?? {}).every(
      ([key, child]) => !Object.hasOwn(value, key) || shape(child, value[key])
    )
  }
  if (schema.type === 'array') {
    return (
      Array.isArray(value) &&
      value.length >= (schema.minItems ?? 0) &&
      value.length <= (schema.maxItems ?? Infinity) &&
      (!schema.uniqueItems || new Set(value).size === value.length) &&
      value.every((item) => shape(schema.items, item))
    )
  }
  if (schema.type === 'string') {
    if (typeof value !== 'string') return false
    let points = value.length
    for (let i = 0; i < value.length; i++) {
      const code = value.charCodeAt(i)
      if (code >= 0xd800 && code <= 0xdbff) {
        points--
        i++
      }
    }
    if (points < (schema.minLength ?? 0) || points > (schema.maxLength ?? Infinity)) return false
    if (schema.pattern && !new RegExp(schema.pattern).test(value)) return false
    if (schema.format && (schema.format !== 'semver-without-build' || !semver(value))) return false
  }
  return true
}
function unicodeLength(text) {
  let bytes = 0
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i)
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = text.charCodeAt(++i)
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw new Error('INVALID_REQUEST')
      bytes += 4
    } else if (code >= 0xdc00 && code <= 0xdfff) throw new Error('INVALID_REQUEST')
    else bytes += code < 128 ? 1 : code < 2048 ? 2 : 3
  }
  return bytes
}
function validatePayload(raw, maxBytes = contract.budget.bytes) {
  if (typeof raw !== 'string' || unicodeLength(raw) > maxBytes) throw new Error('INVALID_REQUEST')
  const request = JSON.parse(raw)
  let nodes = 0
  function visit(value, depth) {
    if (++nodes > contract.budget.nodes || depth > contract.budget.depth)
      throw new Error('BUDGET_EXCEEDED')
    if (
      typeof value === 'number' &&
      (!Number.isFinite(value) || Math.abs(value) > Number.MAX_SAFE_INTEGER)
    )
      throw new Error('INVALID_REQUEST')
    if (typeof value === 'string') unicodeLength(value)
    if (value && typeof value === 'object') {
      for (const key of Object.keys(value)) unicodeLength(key)
      for (const child of Object.values(value)) visit(child, depth + 1)
    }
  }
  visit(request, 1)
  return request
}
export function validateRequest(raw) {
  const request = validatePayload(raw)
  if (!shape(contract.envelope, request)) throw new Error('INVALID_REQUEST')
  const method = contract.methods[request.method]
  if (!method || !shape(method.params, request.params)) throw new Error('INVALID_REQUEST')
  return request
}
export function validateManifest(raw) {
  const manifest = validatePayload(raw)
  if (!shape(contract.manifest, manifest)) throw new Error('INVALID_MANIFEST')
  return manifest
}
export function validateResponse(raw, requestId) {
  let response
  try {
    response = validatePayload(raw)
  } catch {
    throw new Error('INVALID_RESPONSE')
  }
  if (
    !response ||
    response.requestId !== requestId ||
    !(shape(contract.responses.success, response) || shape(contract.responses.failure, response))
  )
    throw new Error('INVALID_RESPONSE')
  return response
}

function encodeUtf8(text) {
  const bytes = new Uint8Array(unicodeLength(text))
  let offset = 0
  for (const char of text) {
    const point = char.codePointAt(0)
    if (point < 128) bytes[offset++] = point
    else if (point < 2048) {
      bytes[offset++] = 192 | (point >> 6)
      bytes[offset++] = 128 | (point & 63)
    } else if (point < 65536) {
      bytes[offset++] = 224 | (point >> 12)
      bytes[offset++] = 128 | ((point >> 6) & 63)
      bytes[offset++] = 128 | (point & 63)
    } else {
      bytes[offset++] = 240 | (point >> 18)
      bytes[offset++] = 128 | ((point >> 12) & 63)
      bytes[offset++] = 128 | ((point >> 6) & 63)
      bytes[offset++] = 128 | (point & 63)
    }
  }
  return bytes
}
function decodeUtf8(bytes) {
  const parts = [],
    chars = []
  for (let i = 0; i < bytes.length;) {
    const lead = bytes[i++]
    let point = lead
    if (lead >= 128) {
      const count =
        lead >= 194 && lead <= 223
          ? 1
          : lead >= 224 && lead <= 239
            ? 2
            : lead >= 240 && lead <= 244
              ? 3
              : -1
      if (count < 0 || i + count > bytes.length) throw new Error('INVALID_RESPONSE')
      point = lead & (count === 1 ? 31 : count === 2 ? 15 : 7)
      for (let j = 0; j < count; j++) {
        const next = bytes[i++]
        if ((next & 192) !== 128) throw new Error('INVALID_RESPONSE')
        point = (point << 6) | (next & 63)
      }
      if (
        point < (count === 1 ? 128 : count === 2 ? 2048 : 65536) ||
        point > 0x10ffff ||
        (point >= 0xd800 && point <= 0xdfff)
      )
        throw new Error('INVALID_RESPONSE')
    }
    chars.push(String.fromCodePoint(point))
    if (chars.length === 4096) {
      parts.push(chars.join(''))
      chars.length = 0
    }
  }
  parts.push(chars.join(''))
  return parts.join('')
}
const base64Alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
function encodeBase64(bytes) {
  const parts = []
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i],
      b = bytes[i + 1] ?? 0,
      c = bytes[i + 2] ?? 0
    parts.push(
      base64Alphabet[a >> 2] +
        base64Alphabet[((a & 3) << 4) | (b >> 4)] +
        (i + 1 < bytes.length ? base64Alphabet[((b & 15) << 2) | (c >> 6)] : '=') +
        (i + 2 < bytes.length ? base64Alphabet[c & 63] : '=')
    )
  }
  return parts.join('')
}
function decodeBase64(value) {
  if (typeof value !== 'string' || value.length % 4) throw new Error('INVALID_RESPONSE')
  const padding = value.endsWith('==') ? 2 : value.endsWith('=') ? 1 : 0
  const length = value.length - padding
  const bytes = new Uint8Array((value.length / 4) * 3 - padding)
  let offset = 0,
    bits = 0,
    buffer = 0,
    last = 0
  for (let i = 0; i < length; i++) {
    const code = value.charCodeAt(i)
    const digit =
      code >= 65 && code <= 90
        ? code - 65
        : code >= 97 && code <= 122
          ? code - 71
          : code >= 48 && code <= 57
            ? code + 4
            : code === 43
              ? 62
              : code === 47
                ? 63
                : -1
    if (digit < 0) throw new Error('INVALID_RESPONSE')
    last = digit
    buffer = (buffer << 6) | digit
    bits += 6
    if (bits >= 8) {
      bits -= 8
      bytes[offset++] = (buffer >> bits) & 255
    }
  }
  // Reject misplaced padding and nonzero unused bits without re-encoding every chunk.
  if (offset !== bytes.length || (padding === 2 && last & 15) || (padding === 1 && last & 3))
    throw new Error('INVALID_RESPONSE')
  return bytes
}

export function createClient({ session, exchange }) {
  let counter = 0
  let inflight = 0
  async function exchangeCall(method, params) {
    if (inflight >= contract.budget.inflight) throw new Error('BUSY')
    const request = {
      wireVersion: contract.versions.wireVersion,
      requestId: `next-${++counter}`,
      session,
      method,
      params
    }
    validateRequest(JSON.stringify(request))
    inflight++
    try {
      const response = validateResponse(JSON.stringify(await exchange(request)), request.requestId)
      if (!response.ok) {
        const error = new Error(response.error.message)
        error.code = response.error.code
        throw error
      }
      if (
        contract.methods[method].result &&
        !shape(contract.methods[method].result, response.result)
      )
        throw new Error('INVALID_RESPONSE')
      return response.result
    } finally {
      inflight--
    }
  }
  async function call(method, params) {
    const result = await exchangeCall(method, params)
    if (
      !['document.call', 'environment.call', 'archive.call'].includes(method) ||
      !result ||
      typeof result !== 'object' ||
      !Object.hasOwn(result, '$nextResult')
    )
      return result
    const opened = result.$nextResult
    if (
      Object.keys(result).length !== 1 ||
      !opened ||
      typeof opened !== 'object' ||
      Object.keys(opened).length !== 2 ||
      typeof opened.readId !== 'string' ||
      !/^[a-f0-9]{64}$/.test(opened.readId)
    )
      throw new Error('INVALID_RESPONSE')
    const readId = opened.readId
    try {
      if (
        !Number.isSafeInteger(opened.byteLength) ||
        opened.byteLength <= contract.resultTransport.inlineBytes ||
        opened.byteLength > contract.resultTransport.maxBytes
      )
        throw new Error('INVALID_RESPONSE')
      const bytes = new Uint8Array(opened.byteLength)
      let offset = 0
      while (offset < bytes.length) {
        const page = await exchangeCall('result.read.chunk', { readId, offset })
        if (
          !page ||
          typeof page !== 'object' ||
          Object.keys(page).length !== 2 ||
          page.offset !== offset ||
          typeof page.data !== 'string'
        )
          throw new Error('INVALID_RESPONSE')
        const chunk = decodeBase64(page.data)
        if (
          !chunk.length ||
          chunk.length > contract.budget.storageChunkBytes ||
          offset + chunk.length > bytes.length
        )
          throw new Error('INVALID_RESPONSE')
        bytes.set(chunk, offset)
        offset += chunk.length
      }
      return validatePayload(decodeUtf8(bytes), contract.resultTransport.maxBytes)
    } finally {
      await exchangeCall('result.read.close', { readId }).catch(() => {})
    }
  }
  function isSizeError(error) {
    return ['BUDGET_EXCEEDED', 'INVALID_REQUEST'].includes(error?.code ?? error?.message)
  }
  async function readLarge(key) {
    const opened = await call('storage.read.begin', { key })
    if (!opened.found) return null
    if (
      !opened.readId ||
      opened.byteLength <= 0 ||
      opened.byteLength > contract.budget.storageValueBytes
    )
      throw new Error('INVALID_RESPONSE')
    const readId = opened.readId
    try {
      const bytes = new Uint8Array(opened.byteLength)
      let offset = 0
      while (offset < bytes.length) {
        const page = await call('storage.read.chunk', { readId, offset })
        const chunk = decodeBase64(page.data)
        if (
          page.offset !== offset ||
          chunk.length === 0 ||
          chunk.length > contract.budget.storageChunkBytes ||
          offset + chunk.length > bytes.length
        )
          throw new Error('INVALID_RESPONSE')
        bytes.set(chunk, offset)
        offset += chunk.length
      }
      return JSON.parse(decodeUtf8(bytes))
    } finally {
      await call('storage.read.close', { readId }).catch(() => {})
    }
  }
  async function transact(operations) {
    if (
      !Array.isArray(operations) ||
      operations.length < 1 ||
      operations.length > contract.budget.storageTransactionOps
    )
      throw new Error('INVALID_REQUEST')
    const seen = new Set()
    const writes = []
    const encoded = []
    const deletes = []
    let total = 0
    for (const operation of operations) {
      if (!operation || (operation.type !== 'set' && operation.type !== 'delete'))
        throw new Error('INVALID_REQUEST')
      const allowed = operation.type === 'set' ? ['type', 'key', 'value'] : ['type', 'key']
      if (
        Object.keys(operation).some((key) => !allowed.includes(key)) ||
        allowed.some((key) => !Object.hasOwn(operation, key))
      )
        throw new Error('INVALID_REQUEST')
      const key = operation.key
      if (typeof key !== 'string' || seen.has(key)) throw new Error('INVALID_REQUEST')
      seen.add(key)
      if (operation.type === 'delete') {
        deletes.push(key)
        continue
      }
      const raw = JSON.stringify(operation.value, (_key, value) => {
        if (
          typeof value === 'undefined' ||
          typeof value === 'function' ||
          typeof value === 'symbol' ||
          typeof value === 'bigint' ||
          (typeof value === 'number' &&
            (!Number.isFinite(value) || Math.abs(value) > Number.MAX_SAFE_INTEGER))
        )
          throw new Error('INVALID_REQUEST')
        return value
      })
      validatePayload(raw, contract.budget.storageValueBytes)
      if (raw === undefined) throw new Error('INVALID_REQUEST')
      const bytes = encodeUtf8(raw)
      if (bytes.length < 1 || bytes.length > contract.budget.storageValueBytes)
        throw new Error('BUDGET_EXCEEDED')
      total += bytes.length
      if (total > contract.budget.storageTransactionBytes) throw new Error('BUDGET_EXCEEDED')
      writes.push({ key, byteLength: bytes.length })
      encoded.push({ key, bytes })
    }
    let transactionId
    try {
      const opened = await call('storage.write.begin', { writes, deletes })
      transactionId = opened.transactionId
      for (const entry of encoded) {
        for (
          let offset = 0;
          offset < entry.bytes.length;
          offset += contract.budget.storageChunkBytes
        ) {
          const data = encodeBase64(
            entry.bytes.subarray(
              offset,
              Math.min(offset + contract.budget.storageChunkBytes, entry.bytes.length)
            )
          )
          const received = await call('storage.write.chunk', {
            transactionId,
            key: entry.key,
            offset,
            data
          })
          if (
            received.receivedBytes !==
            Math.min(offset + contract.budget.storageChunkBytes, entry.bytes.length)
          )
            throw new Error('INVALID_RESPONSE')
        }
      }
      return await call('storage.write.commit', { transactionId })
    } catch (error) {
      if (transactionId) await call('storage.write.abort', { transactionId }).catch(() => {})
      throw error
    }
  }
  return {
    ping: () => call('runtime.ping', {}),
    backend: { call: (method, args = []) => call('backend.call', { method, args }) },
    theme: Object.fromEntries(
      ['get', 'list', 'preview', 'commit', 'rollback', 'set'].map((operation) => [
        operation,
        async (theme) => {
          const result = await call(
            'theme.' + operation,
            operation === 'preview' || operation === 'set' ? { theme } : {}
          )
          if (operation === 'get' && !shape(contract.theme, result))
            throw new Error('INVALID_RESPONSE')
          if (
            operation === 'list' &&
            (!Array.isArray(result) || !result.every((value) => shape(contract.theme, value)))
          )
            throw new Error('INVALID_RESPONSE')
          if (!['get', 'list'].includes(operation) && typeof result !== 'boolean')
            throw new Error('INVALID_RESPONSE')
          return result
        }
      ])
    ),
    dialog: {
      open: async (options) => {
        const result = await call('dialog.open', options)
        if (
          result !== null &&
          !(
            Array.isArray(result) &&
            result.length <= 256 &&
            result.every((path) => typeof path === 'string' && path.length <= 2048)
          )
        )
          throw new Error('INVALID_RESPONSE')
        return result
      },
      confirm: async (options) => {
        const result = await call('dialog.confirm', options)
        if (typeof result !== 'boolean') throw new Error('INVALID_RESPONSE')
        return result
      }
    },
    notify: async (title, body = '') => {
      const result = await call('notification.show', { title, body })
      if (typeof result !== 'boolean') throw new Error('INVALID_RESPONSE')
      return result
    },
    tasks: {
      get: async (taskId) => {
        const value = await call('tasks.get', { taskId })
        if (value !== null && (!shape(contract.taskSnapshot, value) || value.taskId !== taskId))
          throw new Error('INVALID_RESPONSE')
        return value
      },
      list: async (options = {}) => {
        const value = await call('tasks.list', {
          limit: options.limit ?? 20,
          ...(options.after === undefined ? {} : { after: options.after })
        })
        if (
          !value ||
          !Array.isArray(value.items) ||
          value.items.length > (options.limit ?? 20) ||
          !value.items.every(
            (item, index) =>
              shape(contract.taskSnapshot, item) &&
              item.taskId > (index ? value.items[index - 1].taskId : (options.after ?? ''))
          ) ||
          !(
            value.nextCursor === null ||
            (value.items.length > 0 && value.nextCursor === value.items.at(-1).taskId)
          )
        )
          throw new Error('INVALID_RESPONSE')
        return value
      },
      cancel: async (taskId) => {
        const value = await call('tasks.cancel', { taskId })
        if (
          !value ||
          typeof value.accepted !== 'boolean' ||
          !(
            value.task === null ||
            (shape(contract.taskSnapshot, value.task) && value.task.taskId === taskId)
          )
        )
          throw new Error('INVALID_RESPONSE')
        return value
      }
    },
    config: {
      get: () => call('config.get', {}),
      patch: (values) => call('config.patch', { values })
    },
    document: { call: (payload) => call('document.call', { payload }) },
    environment: { call: (payload) => call('environment.call', { payload }) },
    archive: { call: (payload) => call('archive.call', { payload }) },
    storage: {
      get: async (key) => {
        try {
          return await call('storage.get', { key })
        } catch (error) {
          if (!isSizeError(error)) throw error
          return readLarge(key)
        }
      },
      set: async (key, value) => {
        try {
          return await call('storage.set', { key, value })
        } catch (error) {
          if (!isSizeError(error)) throw error
          return transact([{ type: 'set', key, value }])
        }
      },
      delete: (key) => call('storage.delete', { key }),
      batch: async (operations) => {
        try {
          return await call('storage.batch', { operations })
        } catch (error) {
          if (!isSizeError(error)) throw error
          return transact(operations)
        }
      },
      transact,
      keys: (prefix = '', options = {}) =>
        call('storage.keys', {
          prefix,
          limit: options.limit ?? 100,
          ...(options.after === undefined ? {} : { after: options.after })
        }),
      list: (prefix = '', options = {}) =>
        call('storage.list', {
          prefix,
          limit: options.limit ?? 100,
          ...(options.after === undefined ? {} : { after: options.after })
        })
    }
  }
}

export function validateAppearance(raw) {
  const value = validatePayload(raw)
  if (!shape(contract.rendererAppearance, value)) throw new Error('INVALID_APPEARANCE')
  return value
}

export function validateFilesDropped(raw) {
  const value = validatePayload(raw)
  if (!shape(contract.rendererEvents.filesDropped, value)) throw new Error('INVALID_EVENT')
  return value
}

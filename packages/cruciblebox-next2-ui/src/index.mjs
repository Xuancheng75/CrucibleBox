import { createClient as connect, contract } from '@cruciblebox/next2-api'
export * from '@cruciblebox/next2-api'

const encoder = new TextEncoder()
const decoder = new TextDecoder('utf-8', { fatal: true })
const maxValue = contract.budget.storageValueBytes
const chunkSize = contract.budget.storageChunkBytes
const encode = (bytes) => btoa(String.fromCharCode(...bytes))
const decode = (value) => Uint8Array.from(atob(value), (char) => char.charCodeAt(0))
function bytes(value, maximum = maxValue) {
  const raw = JSON.stringify(value, (key, value) => {
    if (
      /[\ud800-\udfff]/u.test(key) ||
      (typeof value === 'string' && /[\ud800-\udfff]/u.test(value)) ||
      ['undefined', 'function', 'symbol', 'bigint'].includes(typeof value) ||
      (typeof value === 'number' &&
        (!Number.isFinite(value) || Math.abs(value) > Number.MAX_SAFE_INTEGER))
    )
      throw Error('INVALID_STORAGE_VALUE')
    return value
  })
  if (typeof raw !== 'string') throw Error('INVALID_STORAGE_VALUE')
  const result = encoder.encode(raw)
  if (result.length > maximum) throw Error('BUDGET_EXCEEDED')
  return result
}
export function createClient(context) {
  const api = connect(context)
  const call = api.platform
  async function readLarge(key) {
    const opened = await call('storage.read.begin', { key })
    if (!opened.found) return null
    const { readId, byteLength } = opened
    if (!readId || byteLength < 1) throw Error('INVALID_RESPONSE')
    try {
      const output = new Uint8Array(byteLength)
      let offset = 0
      while (offset < byteLength) {
        const page = await call('storage.read.chunk', { readId, offset })
        const chunk = decode(page.data)
        if (
          page.offset !== offset ||
          !chunk.length ||
          chunk.length > chunkSize ||
          offset + chunk.length > byteLength
        )
          throw Error('INVALID_RESPONSE')
        output.set(chunk, offset)
        offset += chunk.length
      }
      return JSON.parse(decoder.decode(output))
    } finally {
      await call('storage.read.close', { readId })
    }
  }
  async function transact(operations) {
    if (
      !Array.isArray(operations) ||
      !operations.length ||
      operations.length > contract.budget.storageTransactionOps
    )
      throw Error('INVALID_STORAGE_OPERATIONS')
    const keys = new Set(),
      writes = [],
      deletes = [],
      data = []
    for (const operation of operations) {
      if (!operation || keys.has(operation.key)) throw Error('INVALID_STORAGE_OPERATIONS')
      keys.add(operation.key)
      if (operation.type === 'delete') deletes.push(operation.key)
      else if (operation.type === 'set') {
        const value = bytes(operation.value)
        writes.push({ key: operation.key, byteLength: value.length })
        data.push({ key: operation.key, value })
      } else throw Error('INVALID_STORAGE_OPERATIONS')
    }
    if (
      data.reduce((sum, entry) => sum + entry.value.length, 0) >
      contract.budget.storageTransactionBytes
    )
      throw Error('BUDGET_EXCEEDED')
    const { transactionId } = await call('storage.write.begin', { writes, deletes })
    try {
      for (const { key, value } of data) {
        for (let offset = 0; offset < value.length; offset += chunkSize) {
          const chunk = value.subarray(offset, offset + chunkSize)
          const response = await call('storage.write.chunk', {
            transactionId,
            key,
            offset,
            data: encode(chunk)
          })
          if (response.receivedBytes !== offset + chunk.length) throw Error('INVALID_RESPONSE')
        }
      }
      return await call('storage.write.commit', { transactionId })
    } catch (error) {
      try {
        await call('storage.write.abort', { transactionId })
      } catch (abort) {
        throw new AggregateError([error, abort], 'Storage write and abort failed')
      }
      throw error
    }
  }
  const storage = {
    get: async (key) => {
      try {
        return await call('storage.get', { key })
      } catch (error) {
        if ((error.code ?? error.message) !== 'BUDGET_EXCEEDED') throw error
        return readLarge(key)
      }
    },
    set: (key, value) =>
      bytes(value).length <= 32768
        ? call('storage.set', { key, value })
        : transact([{ type: 'set', key, value }]),
    delete: (key) => call('storage.delete', { key }),
    batch: (operations) =>
      bytes(operations, contract.budget.storageTransactionBytes + 65536).length <= 32768
        ? call('storage.batch', { operations })
        : transact(operations),
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
  return {
    ...api,
    storage,
    ping: () => call('runtime.ping', {}),
    config: {
      get: () => call('config.get', {}),
      patch: (values) => call('config.patch', { values })
    },
    theme: Object.fromEntries(
      ['get', 'list', 'preview', 'commit', 'rollback', 'set'].map((method) => [
        method,
        (theme) => call('theme.' + method, ['preview', 'set'].includes(method) ? { theme } : {})
      ])
    ),
    dialog: {
      open: (options) => call('dialog.open', options),
      confirm: (options) => call('dialog.confirm', options)
    },
    notify: (title, body = '') => call('notification.show', { title, body }),
    tasks: {
      start: (target, method, params = {}) => call('tasks.start', { target, method, params }),
      get: (taskId) => call('tasks.get', { taskId }),
      cancel: (taskId) => call('tasks.cancel', { taskId }),
      list: (options = {}) =>
        call('tasks.list', {
          limit: options.limit ?? 20,
          ...(options.after === undefined ? {} : { after: options.after })
        })
    }
  }
}

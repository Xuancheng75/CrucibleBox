import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import { contract } from '../packages/cruciblebox-next2-api/src/generated.mjs'

function frame() {
  let now = 0
  let sequence = 0
  const timers = new Map()
  const root = {
    dataset: {
      apiVersion: '6',
      maxInflight: String(contract.budget.inflight),
      requestTimeout: String(contract.budget.requestTimeoutMs),
      interactiveTimeout: String(contract.budget.interactiveTimeoutMs),
      handshakeTimeout: String(contract.budget.handshakeTimeoutMs)
    }
  }
  const context = vm.createContext({
    document: { getElementById: () => root },
    parent: { postMessage() {} },
    addEventListener() {},
    removeEventListener() {},
    setTimeout(callback, delay) {
      const id = ++sequence
      timers.set(id, { callback, at: now + delay })
      return id
    },
    clearTimeout: (id) => timers.delete(id)
  })
  const source = readFileSync(
    new URL('../src-tauri/src/next_frame_runtime.js', import.meta.url),
    'utf8'
  )
  const end = source.lastIndexOf('})()')
  // Expose the actual exchange closure in this VM only; no product test hooks.
  const hook = `
    clearTimeout(handshakeTimer)
    connected = true
    port = { postMessage() {}, close() {} }
    globalThis.frameTest = { exchange, close }
  `
  vm.runInContext(source.slice(0, end) + hook + source.slice(end), context)
  return {
    ...context.frameTest,
    advance(milliseconds) {
      now += milliseconds
      for (const [id, timer] of timers) {
        if (timer.at <= now) {
          timers.delete(id)
          timer.callback()
        }
      }
    },
    pendingTimers: () => timers.size
  }
}
const request = (method, kind = 'platform') => ({ requestId: 'test-1', target: { kind }, method })

test('a human file choice remains pending after the ordinary request deadline', async () => {
  const page = frame()
  const response = page.exchange(request('files.choose'))
  let rejected = false
  response.catch(() => {
    rejected = true
  })
  page.advance(contract.budget.requestTimeoutMs + 1)
  await Promise.resolve()
  assert.equal(rejected, false)
  page.advance(contract.budget.interactiveTimeoutMs)
  await assert.rejects(response, /TIMEOUT/)
})

test('ordinary requests and similarly named backend calls retain the ordinary deadline', async () => {
  for (const call of [request('storage.get'), request('files.choose', 'backend')]) {
    const page = frame()
    const response = page.exchange(call)
    page.advance(contract.budget.requestTimeoutMs + 1)
    await assert.rejects(response, /TIMEOUT/)
  }
})

test('session disposal cancels an outstanding choice and releases its timer', async () => {
  const page = frame()
  const response = page.exchange(request('files.choose'))
  page.close('SESSION_REPLACED')
  await assert.rejects(response, /SESSION_REPLACED/)
  assert.equal(page.pendingTimers(), 0)
})

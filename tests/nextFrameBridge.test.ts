import { afterEach, expect, test, vi } from 'vitest'
import {
  createNextDispatcher,
  isNextReady,
  transport,
  nextSandbox,
  scheduleNextLease,
  type NextSession
} from '../tauri-frontend/src/features/plugins/runtime/NextFrameBridge'
const session: NextSession = {
  token: 'a'.repeat(64),
  handshakeToken: 'b'.repeat(64),
  origin: 'null',
  rendererApiVersion: 5,
  indexUrl: 'http://cruciblebox-plugin.localhost/test/index.html',
  expiresAt: Date.now() + 60000
}
const request = (id = 'r1') => ({
  wireVersion: 3,
  requestId: id,
  session: session.token,
  method: 'runtime.ping',
  params: {}
})
const reply = (id = 'r1') => ({ wireVersion: 3, requestId: id, ok: true, result: 'pong' })
afterEach(() => vi.useRealTimers())
test('ready binds opaque origin, exact iframe, nonce and expiry', () => {
  const source = {}
  const event = {
    source,
    origin: 'null',
    data: { kind: 'next-ready', nonce: session.handshakeToken, wireVersion: 3 }
  }
  expect(isNextReady(event, source, session)).toBe(true)
  expect(isNextReady(event, {}, session)).toBe(false)
  expect(
    isNextReady({ ...event, origin: 'http://cruciblebox-plugin.localhost' }, source, session)
  ).toBe(false)
  expect(isNextReady({ ...event, data: { ...event.data, nonce: 'wrong' } }, source, session)).toBe(
    false
  )
  expect(isNextReady(event, source, { ...session, expiresAt: 1 })).toBe(false)
})
test('invalid methods and foreign sessions never invoke native host', () => {
  const invoke = vi.fn(),
    send = vi.fn()
  const bridge = createNextDispatcher(session, invoke, send)
  bridge.dispatch({ ...request(), method: 'host.sql' })
  bridge.dispatch({ ...request(), session: 'c'.repeat(64) })
  expect(invoke).not.toHaveBeenCalled()
  expect(send).toHaveBeenCalledWith(
    expect.objectContaining({ error: { code: 'SESSION_DENIED', message: 'SESSION_DENIED' } })
  )
  bridge.dispose()
})
test('correlated success crosses native bridge; malformed response fails closed', async () => {
  const send = vi.fn()
  const bridge = createNextDispatcher(session, async () => reply(), send)
  bridge.dispatch(request())
  await vi.waitFor(() => expect(send).toHaveBeenCalledWith(reply()))
  bridge.dispose()
  const bad = createNextDispatcher(session, async () => reply('other'), send)
  bad.dispatch(request('bad'))
  await vi.waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({
        requestId: 'bad',
        error: { code: 'INVALID_RESPONSE', message: 'INVALID_RESPONSE' }
      })
    )
  )
  bad.dispose()
})
test('timeout sends once and ignores late completion', async () => {
  vi.useFakeTimers()
  let resolve!: (value: unknown) => void
  const send = vi.fn()
  const bridge = createNextDispatcher(
    session,
    () =>
      new Promise((r) => {
        resolve = r
      }),
    send
  )
  bridge.dispatch(request())
  await Promise.resolve()
  await vi.advanceTimersByTimeAsync(transport.requestTimeoutMs)
  expect(send).toHaveBeenCalledTimes(1)
  expect(send.mock.calls[0][0].error.code).toBe('TIMEOUT')
  resolve(reply())
  await Promise.resolve()
  await Promise.resolve()
  expect(send).toHaveBeenCalledTimes(1)
  bridge.dispose()
})
test('dispose suppresses late completion and capacity stays bounded', async () => {
  let resolve!: (value: unknown) => void
  const send = vi.fn()
  const invoke = vi.fn(
    () =>
      new Promise((r) => {
        resolve = r
      })
  )
  const bridge = createNextDispatcher(session, invoke, send)
  bridge.dispatch(request())
  await Promise.resolve()
  bridge.dispose()
  resolve(reply())
  await Promise.resolve()
  await Promise.resolve()
  expect(send).not.toHaveBeenCalled()
  const bounded = createNextDispatcher(session, async () => new Promise(() => {}), send)
  for (let i = 0; i < 33; i++) bounded.dispatch(request(`r-${i}`))
  expect(send).toHaveBeenCalledWith(
    expect.objectContaining({ error: { code: 'BUSY', message: 'BUSY' } })
  )
  bounded.dispose()
})

test('duplicate pending request cannot cause two final replies', async () => {
  let resolve!: (value: unknown) => void
  const send = vi.fn(),
    invoke = vi.fn(
      () =>
        new Promise((r) => {
          resolve = r
        })
    )
  const bridge = createNextDispatcher(session, invoke, send)
  bridge.dispatch(request())
  bridge.dispatch(request())
  await Promise.resolve()
  expect(invoke).toHaveBeenCalledTimes(1)
  expect(send).not.toHaveBeenCalled()
  resolve(reply())
  await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1))
  bridge.dispose()
})

test('only host-issued download capability adds downloads and never same-origin or modals', () => {
  expect(nextSandbox(session)).toBe('allow-scripts')
  expect(nextSandbox({ ...session, permissions: ['storage:write', 'browser:clipboard'] })).toBe(
    'allow-scripts'
  )
  expect(nextSandbox({ ...session, permissions: ['browser:downloads'] })).toBe(
    'allow-scripts allow-downloads'
  )
})

test('browser capability lifetime ends at lease and cleanup cancels the disposal timer', () => {
  vi.useFakeTimers()
  const close = vi.fn()
  scheduleNextLease({ ...session, expiresAt: Date.now() + 500 }, close)
  vi.advanceTimersByTime(499)
  expect(close).not.toHaveBeenCalled()
  vi.advanceTimersByTime(1)
  expect(close).toHaveBeenCalledOnce()
  const cancel = scheduleNextLease({ ...session, expiresAt: Date.now() + 500 }, close)
  cancel()
  vi.advanceTimersByTime(500)
  expect(close).toHaveBeenCalledOnce()
})

test('same-session reads execute once in order while another frame remains independent', async () => {
  let release!: (value: unknown) => void
  const invoke = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve
        })
    )
    .mockImplementation(async (raw) => reply(JSON.parse(raw).requestId))
  const send = vi.fn()
  const bridge = createNextDispatcher(session, invoke, send)
  bridge.dispatch(request('theme-read'))
  bridge.dispatch(request('config-read'))
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(1))
  const otherSend = vi.fn()
  const other = createNextDispatcher(session, async () => reply('independent'), otherSend)
  other.dispatch(request('independent'))
  await vi.waitFor(() => expect(otherSend).toHaveBeenCalledWith(reply('independent')))
  expect(invoke).toHaveBeenCalledTimes(1)
  release(reply('theme-read'))
  await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(2))
  expect(invoke.mock.calls.map(([raw]) => JSON.parse(raw).requestId)).toEqual([
    'theme-read',
    'config-read'
  ])
  bridge.dispose()
  other.dispose()
})

test('expired queued mutations never execute after the running operation returns', async () => {
  vi.useFakeTimers()
  let release!: (value: unknown) => void
  const invoke = vi.fn().mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        release = resolve
      })
  )
  const send = vi.fn()
  const bridge = createNextDispatcher(session, invoke, send)
  bridge.dispatch(request('slow'))
  bridge.dispatch({
    ...request('queued-write'),
    method: 'storage.set',
    params: { key: 'note', value: 'never write' }
  })
  await Promise.resolve()
  await vi.advanceTimersByTimeAsync(transport.requestTimeoutMs)
  release(reply('slow'))
  await vi.advanceTimersByTimeAsync(0)
  expect(invoke).toHaveBeenCalledTimes(1)
  expect(send).toHaveBeenCalledTimes(2)
  expect(send.mock.calls.every(([response]) => response.error.code === 'TIMEOUT')).toBe(true)
  bridge.dispose()
})

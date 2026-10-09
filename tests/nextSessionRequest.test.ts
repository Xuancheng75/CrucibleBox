import { afterEach, expect, test, vi } from 'vitest'
import { requestNextSession } from '../tauri-frontend/src/features/plugins/runtime/NextSessionRequest'
afterEach(() => vi.useRealTimers())
test('retries BUSY and preserves the legacy denial for routing', async () => {
  vi.useFakeTimers()
  const request = vi
    .fn()
    .mockRejectedValueOnce('BUSY')
    .mockRejectedValueOnce('BUSY')
    .mockRejectedValue('SESSION_DENIED')
  const outcome = requestNextSession(request, new AbortController().signal).catch(
    (reason) => reason
  )
  await vi.advanceTimersByTimeAsync(200)
  expect(await outcome).toBe('SESSION_DENIED')
  expect(request).toHaveBeenCalledTimes(3)
})
test('caps contention retries and does not retry permission errors', async () => {
  vi.useFakeTimers()
  const request = vi.fn().mockRejectedValue('BUSY')
  const outcome = requestNextSession(request, new AbortController().signal).catch(
    (reason) => reason
  )
  await vi.advanceTimersByTimeAsync(2000)
  expect(await outcome).toBe('BUSY')
  expect(request).toHaveBeenCalledTimes(21)
  const denied = vi.fn().mockRejectedValue('SESSION_EXPIRED')
  await expect(requestNextSession(denied, new AbortController().signal)).rejects.toBe(
    'SESSION_EXPIRED'
  )
  expect(denied).toHaveBeenCalledTimes(1)
})
test('abort cancels pending retries while a late issued token remains disposable', async () => {
  vi.useFakeTimers()
  const controller = new AbortController()
  const request = vi.fn().mockRejectedValue('BUSY')
  const outcome = requestNextSession(request, controller.signal).catch((reason) => reason)
  await vi.advanceTimersByTimeAsync(0)
  controller.abort()
  expect(await outcome).toMatchObject({ name: 'AbortError' })
  await vi.advanceTimersByTimeAsync(1000)
  expect(request).toHaveBeenCalledTimes(1)
  let issue!: (value: { token: string }) => void
  const second = new AbortController()
  const late = requestNextSession(
    () =>
      new Promise<{ token: string }>((resolve) => {
        issue = resolve
      }),
    second.signal
  )
  second.abort()
  issue({ token: 'dispose-me' })
  expect(await late).toEqual({ token: 'dispose-me' })
})

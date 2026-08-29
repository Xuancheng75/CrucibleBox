import { describe, expect, it, vi } from 'vitest'
import {
  UpdateCheckTimeoutError,
  formatUpdateError,
  isTransientUpdateError,
  retryUpdateCheck,
  withUpdateTimeout
} from '../tauri-frontend/src/utils/update-check'

describe('Tauri update check resilience', () => {
  it('times out a hanging request and clears the timer after success', async () => {
    await expect(withUpdateTimeout(() => Promise.resolve('ok'), 20)).resolves.toBe('ok')
    await expect(withUpdateTimeout(() => new Promise(() => {}), 5)).rejects.toBeInstanceOf(
      UpdateCheckTimeoutError
    )
  })

  it('retries transient network failures once', async () => {
    const check = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(new Error('error sending request for url'))
      .mockResolvedValueOnce('available')

    await expect(
      retryUpdateCheck(check, { timeoutMs: 20, maxAttempts: 2, retryDelayMs: 0 })
    ).resolves.toBe('available')
    expect(check).toHaveBeenCalledTimes(2)
  })

  it('does not retry non-transient manifest errors', async () => {
    const check = vi.fn<() => Promise<string>>().mockRejectedValue(new Error('invalid signature'))

    await expect(
      retryUpdateCheck(check, { timeoutMs: 20, maxAttempts: 2, retryDelayMs: 0 })
    ).rejects.toThrow('invalid signature')
    expect(check).toHaveBeenCalledTimes(1)
    expect(isTransientUpdateError(new Error('connection reset by peer'))).toBe(true)
    expect(isTransientUpdateError(new Error('invalid signature'))).toBe(false)
  })

  it('converts network failures to an actionable message', () => {
    expect(formatUpdateError(new Error('error sending request for url'))).toBe(
      '更新服务器暂时不可达，请检查网络后重试。'
    )
    expect(formatUpdateError(new Error('invalid signature'))).toBe('invalid signature')
  })
})

import { describe, expect, it } from 'vitest'
import { PluginCrashPolicy } from '../plugin-system/PluginCrashPolicy'

describe('PluginCrashPolicy', () => {
  it('applies bounded exponential backoff and then quarantines a crash loop', () => {
    const policy = new PluginCrashPolicy({
      baseDelayMs: 100,
      backoffFactor: 5,
      maxDelayMs: 300,
      quarantineThreshold: 4,
      windowMs: 1_000
    })

    expect(policy.record('plugin-a', 0)).toEqual({
      action: 'restart',
      crashesInWindow: 1,
      delayMs: 100
    })
    expect(policy.record('plugin-a', 10)).toEqual({
      action: 'restart',
      crashesInWindow: 2,
      delayMs: 300
    })
    expect(policy.record('plugin-a', 20)).toEqual({
      action: 'restart',
      crashesInWindow: 3,
      delayMs: 300
    })
    expect(policy.record('plugin-a', 30)).toEqual({
      action: 'quarantine',
      crashesInWindow: 4,
      delayMs: 0
    })
  })

  it('expires old crashes and supports an explicit operator reset', () => {
    const policy = new PluginCrashPolicy({ baseDelayMs: 10, windowMs: 100 })
    policy.record('plugin-a', 0)
    policy.record('plugin-a', 10)

    expect(policy.record('plugin-a', 200).crashesInWindow).toBe(1)
    policy.reset('plugin-a')
    expect(policy.record('plugin-a', 210).crashesInWindow).toBe(1)
  })
})

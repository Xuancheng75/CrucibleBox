export interface PluginCrashPolicyOptions {
  baseDelayMs?: number
  backoffFactor?: number
  maxDelayMs?: number
  quarantineThreshold?: number
  windowMs?: number
}

export interface PluginCrashDecision {
  action: 'restart' | 'quarantine'
  crashesInWindow: number
  delayMs: number
}

const DEFAULT_BASE_DELAY_MS = 1_000
const DEFAULT_BACKOFF_FACTOR = 5
const DEFAULT_MAX_DELAY_MS = 30_000
const DEFAULT_QUARANTINE_THRESHOLD = 3
const DEFAULT_WINDOW_MS = 5 * 60_000

function positiveInteger(value: number | undefined, fallback: number): number {
  return Number.isSafeInteger(value) && value! > 0 ? value! : fallback
}

export class PluginCrashPolicy {
  private readonly baseDelayMs: number
  private readonly backoffFactor: number
  private readonly crashHistory = new Map<string, number[]>()
  private readonly maxDelayMs: number
  private readonly quarantineThreshold: number
  private readonly windowMs: number

  constructor(options: PluginCrashPolicyOptions = {}) {
    this.baseDelayMs = positiveInteger(options.baseDelayMs, DEFAULT_BASE_DELAY_MS)
    this.backoffFactor = positiveInteger(options.backoffFactor, DEFAULT_BACKOFF_FACTOR)
    this.maxDelayMs = positiveInteger(options.maxDelayMs, DEFAULT_MAX_DELAY_MS)
    this.quarantineThreshold = positiveInteger(
      options.quarantineThreshold,
      DEFAULT_QUARANTINE_THRESHOLD
    )
    this.windowMs = positiveInteger(options.windowMs, DEFAULT_WINDOW_MS)
  }

  record(pluginId: string, now = Date.now()): PluginCrashDecision {
    const cutoff = now - this.windowMs
    const history = (this.crashHistory.get(pluginId) ?? []).filter(
      (timestamp) => timestamp >= cutoff
    )
    history.push(now)
    this.crashHistory.set(pluginId, history)

    if (history.length >= this.quarantineThreshold) {
      return { action: 'quarantine', crashesInWindow: history.length, delayMs: 0 }
    }

    const delayMs = Math.min(
      this.maxDelayMs,
      this.baseDelayMs * this.backoffFactor ** (history.length - 1)
    )
    return { action: 'restart', crashesInWindow: history.length, delayMs }
  }

  reset(pluginId: string): void {
    this.crashHistory.delete(pluginId)
  }
}

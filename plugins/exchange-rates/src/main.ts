import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'

interface RatesCache {
  base: string
  rates: Record<string, number>
  updatedAt: number
}

let ctx: PluginContext | null = null

const CACHE_KEY = 'rates:cache'
const CACHE_TTL = 30 * 60 * 1000

const plugin: PluginMain = {
  activate(pluginCtx: PluginContext) {
    ctx = pluginCtx
  },

  deactivate() {
    ctx = null
  },

  async onMessage(message: unknown) {
    if (!ctx) return { error: 'not activated' }
    const msg = message as { type: string; amount?: number; from?: string; to?: string }

    switch (msg.type) {
      case 'getRates': {
        const cached = await getCachedRates()
        if (cached && Date.now() - cached.updatedAt < CACHE_TTL) {
          return { rates: cached.rates, updatedAt: cached.updatedAt, base: cached.base, cached: true }
        }
        return await fetchAndCacheRates()
      }
      case 'refresh': {
        return await fetchAndCacheRates()
      }
      case 'convert': {
        const amount = msg.amount || 0
        const from = msg.from || 'USD'
        const to = msg.to || 'CNY'
        const cached = await getCachedRates()
        if (!cached) {
          const fresh = await fetchAndCacheRates()
          if ((fresh as { error?: string }).error) return fresh
          const rates = (fresh as RatesCache & { rates: Record<string, number> }).rates
          const rate = (rates[to] || 1) / (rates[from] || 1)
          return { result: amount * rate, rate }
        }
        const rate = (cached.rates[to] || 1) / (cached.rates[from] || 1)
        return { result: amount * rate, rate }
      }
      default:
        return { error: `unknown message: ${msg.type}` }
    }
  }
}

async function getCachedRates(): Promise<RatesCache | null> {
  if (!ctx) return null
  const raw = await ctx.storage.get<string>(CACHE_KEY)
  if (!raw) return null
  try {
    return JSON.parse(raw) as RatesCache
  } catch {
    return null
  }
}

async function fetchAndCacheRates(): Promise<unknown> {
  if (!ctx) return { error: 'not activated' }
  try {
    const resp = await ctx.api.fetch('https://open.er-api.com/v6/latest/USD')
    const text = await resp.text()
    const data = JSON.parse(text)
    if (data.result !== 'success' || !data.rates) {
      return { error: 'API returned error' }
    }
    const cache: RatesCache = {
      base: 'USD',
      rates: data.rates,
      updatedAt: Date.now()
    }
    await ctx.storage.set(CACHE_KEY, JSON.stringify(cache))
    return { rates: data.rates, updatedAt: cache.updatedAt, base: 'USD', cached: false }
  } catch (e) {
    const cached = await getCachedRates()
    if (cached) {
      return { rates: cached.rates, updatedAt: cached.updatedAt, base: cached.base, cached: true, stale: true }
    }
    return { error: (e as Error).message }
  }
}

export default plugin

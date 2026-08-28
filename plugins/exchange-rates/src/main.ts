import type {
  PluginContext,
  PluginFetchResponse,
  PluginMain
} from 'cruciblebox-plugin-api'

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
  const raw = await ctx.storage.get<RatesCache>(CACHE_KEY)
  if (!raw) return null
  return raw
}

async function fetchAndCacheRates(): Promise<unknown> {
  if (!ctx) return { error: 'not activated' }
  try {
    const resp = await ctx.api.fetch('https://open.er-api.com/v6/latest/USD')
    if (!resp.ok) return { error: `API returned HTTP ${resp.status}` }
    const data = JSON.parse(await readFetchBody(resp))
    if (data.result !== 'success' || !data.rates) {
      return { error: 'API returned error' }
    }
    const cache: RatesCache = {
      base: 'USD',
      rates: data.rates,
      updatedAt: Date.now()
    }
    await ctx.storage.set(CACHE_KEY, cache)
    return { rates: data.rates, updatedAt: cache.updatedAt, base: 'USD', cached: false }
  } catch (e) {
    const cached = await getCachedRates()
    if (cached) {
      return { rates: cached.rates, updatedAt: cached.updatedAt, base: cached.base, cached: true, stale: true }
    }
    return { error: (e as Error).message }
  }
}

async function readFetchBody(response: Response | PluginFetchResponse): Promise<string> {
  if (isPluginFetchResponse(response)) return response.body
  return response.text()
}

function isPluginFetchResponse(
  response: Response | PluginFetchResponse
): response is PluginFetchResponse {
  return typeof response.body === 'string'
}

export default plugin

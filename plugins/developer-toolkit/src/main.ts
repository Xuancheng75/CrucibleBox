import type { PluginContext, PluginFetchResponse, PluginMain } from 'cruciblebox-plugin-api'

let context: PluginContext | null = null

const plugin: PluginMain = {
  activate(ctx) {
    context = ctx
  },
  deactivate() {
    context = null
  },
  async onMessage(message) {
    if (!context) return { error: '插件尚未启动' }
    const request = message as {
      type?: string
      method?: string
      url?: string
      headers?: Record<string, string>
      body?: string
      state?: unknown
    }
    if (request.type === 'getState')
      return (
        (await context.storage.get('api-debugger-state')) ?? {
          collections: [],
          history: [],
          environment: {}
        }
      )
    if (request.type === 'saveState') {
      await context.storage.set('api-debugger-state', request.state)
      return { ok: true }
    }
    if (request.type !== 'request') return { error: '不支持的接口操作' }
    const started = Date.now()
    try {
      const response = await context.api.fetch(request.url ?? '', {
        method: request.method ?? 'GET',
        headers: request.headers ?? {},
        body: ['GET', 'HEAD'].includes(request.method ?? 'GET') ? undefined : request.body
      })
      if (typeof (response as PluginFetchResponse).body === 'string') {
        const result = response as PluginFetchResponse
        return {
          status: result.status,
          statusText: result.statusText,
          headers: result.headers,
          body: result.body,
          durationMs: Date.now() - started
        }
      }
      const result = response as Response
      return {
        status: result.status,
        statusText: result.statusText,
        headers: Object.fromEntries(result.headers),
        body: await result.text(),
        durationMs: Date.now() - started
      }
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error) }
    }
  }
}

export default plugin

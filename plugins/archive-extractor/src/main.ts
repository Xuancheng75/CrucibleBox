import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'

let context: PluginContext | null = null

const plugin: PluginMain = {
  async activate(nextContext: PluginContext): Promise<void> {
    context = nextContext
    await context.api.invokeTrustedService!('archive-extractor', 'activate')
  },

  async deactivate(): Promise<void> {
    const activeContext = context
    context = null
    if (activeContext) {
      await activeContext.api.invokeTrustedService!('archive-extractor', 'deactivate')
    }
  },

  async onMessage(message: unknown): Promise<unknown> {
    if (!context) throw new Error('Archive Extractor backend is not active')
    return context.api.invokeTrustedService!('archive-extractor', 'message', message)
  }
}

export default plugin

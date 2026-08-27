import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'

let ctx: PluginContext | null = null

const plugin: PluginMain = {
  activate(pluginCtx: PluginContext) {
    ctx = pluginCtx
  },

  deactivate() {
    ctx = null
  },

  async onMessage(message: unknown) {
    if (!ctx) return { error: 'not activated' }
    const msg = message as { type: string }

    if (msg.type === 'getSystemInfo') {
      const info = await ctx.api.getSystemInfo()
      return info
    }

    return { error: `unknown message: ${msg.type}` }
  }
}

export default plugin

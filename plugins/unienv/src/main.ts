import type { PluginContext } from 'cruciblebox-plugin-api'

let context: PluginContext | null = null

const plugin = {
  async activate(nextContext: PluginContext): Promise<void> {
    context = nextContext
    // invokeTrustedService 由 trusted:unienv 权限门控，运行时宿主固定提供
    await context.api.invokeTrustedService!('unienv', 'activate')
    context.logger.info('[UniEnv] trusted host service connected')
  },

  async deactivate(): Promise<void> {
    const activeContext = context
    context = null
    if (activeContext) {
      await activeContext.api.invokeTrustedService!('unienv', 'deactivate')
    }
  },

  async onMessage(message: unknown): Promise<unknown> {
    if (!context) throw new Error('UniEnv backend is not active')
    return await context.api.invokeTrustedService!('unienv', 'message', message)
  }
}

export default plugin

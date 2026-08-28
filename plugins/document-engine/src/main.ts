import type { PluginContext } from 'cruciblebox-plugin-api'

let context: PluginContext | null = null

const plugin = {
  async activate(nextContext: PluginContext): Promise<void> {
    context = nextContext
    // document-engine 为宿主固定摘要可信服务（trusted:document-engine 权限）
    // 运行时宿主通过 envelope_host 分发到 document_engine_service::dispatch
    await context.api.invokeTrustedService!('document-engine', 'activate')
    context.logger.info('[DocumentEngine] trusted host service connected')
  },

  async deactivate(): Promise<void> {
    const activeContext = context
    context = null
    if (activeContext) {
      await activeContext.api.invokeTrustedService!('document-engine', 'deactivate')
    }
  },

  async onMessage(message: unknown): Promise<unknown> {
    if (!context) throw new Error('Document Engine backend is not active')
    return await context.api.invokeTrustedService!('document-engine', 'message', message)
  }
}

export default plugin

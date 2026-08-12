import type { PluginContext, PluginMain } from 'openbox-plugin-api'

const plugin: PluginMain = {
  activate(ctx: PluginContext) {
    ctx.logger.info('插件已激活')

    ctx.api.notify('插件已启动', '插件已就绪')

    ctx.api.onEvent('app:ready', () => {
      ctx.logger.info('应用已就绪')
    })
  },

  deactivate() {
    console.log('插件已停用')
  },

  onMessage(message: unknown) {
    console.log('收到消息:', message)
    return { echo: message }
  }
}

export default plugin

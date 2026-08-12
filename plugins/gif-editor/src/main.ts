import type { PluginMain } from 'openbox-plugin-api'

const plugin: PluginMain = {
  activate(ctx) {
    ctx.logger.info('GIF 编辑器插件已激活')
  },

  deactivate() {},

  onMessage(message) {
    const msg = message as { type?: unknown } | null
    if (msg && msg.type === 'ping') {
      return { ok: true }
    }
    return null
  }
}

export default plugin

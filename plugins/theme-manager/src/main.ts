import type { PluginContext, PluginMain } from 'openbox-plugin-api'

let disposeThemeListener: (() => void) | null = null

const plugin: PluginMain = {
  activate(ctx: PluginContext) {
    ctx.logger.info('主题管理插件已激活')

    disposeThemeListener = ctx.api.onEvent('openbox:theme-changed', (data) => {
      const theme = data as { id?: string; name?: string }
      ctx.logger.info(`工具箱主题已切换：${theme?.name || theme?.id || '未知'}`)
    })
  },

  deactivate() {
    if (disposeThemeListener) {
      disposeThemeListener()
      disposeThemeListener = null
    }
    console.log('主题管理插件已停用')
  }
}

export default plugin
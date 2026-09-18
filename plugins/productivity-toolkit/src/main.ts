import type { PluginContext } from 'cruciblebox-plugin-api'
let context: PluginContext | undefined
const plugin = {
  async activate(next: PluginContext) {
    context = next
  },
  async deactivate() {
    context = undefined
  },
  async onMessage(message: unknown) {
    if (!context) throw new Error('笔记与效率尚未启动')
    const request = message as { type?: string; key?: string; value?: unknown }
    if (request.type === 'get' && request.key) return await context.storage.get(request.key)
    if (request.type === 'set' && request.key) {
      await context.storage.set(request.key, request.value)
      return { success: true }
    }
    if (request.type === 'remove' && request.key) {
      await context.storage.delete(request.key)
      return { success: true }
    }
    throw new Error('不支持的操作')
  }
}
export default plugin

import { execute as dbExecute, queryAll as dbQueryAll } from '@database/index'
import type { PluginLogEntry, PluginLogFilter } from '@shared/types/plugin.types'

/**
 * PluginLogService — 插件日志的 SQL 访问与写入。
 *
 * 从 PluginManager 抽出，SQL 文本逐字保留：
 * - getLogs：行映射（SQL 别名）、排序、limit 钳制 1..2000（默认 500）
 * - clearLogs：按插件清空或全清
 * - log：message 截断 4000 字符、INSERT、trim、降级与事件负载（trimmed 消息）
 * - trimPluginLogs：每插件最多 2000 行
 */

export interface PluginLogEvent {
  pluginId: string
  level: string
  message: string
}

export interface PluginLogServiceOptions {
  /** 日志写入后触发（用于向 EventBus 发 plugin:log） */
  emitLog?: (entry: PluginLogEvent) => void
}

const MAX_LOG_LENGTH = 4000
const MAX_LOG_ROWS_PER_PLUGIN = 2000

export class PluginLogService {
  constructor(private readonly options: PluginLogServiceOptions = {}) {}

  getLogs(filter: PluginLogFilter = {}): PluginLogEntry[] {
    const conditions: string[] = []
    const params: Array<string | number> = []
    if (filter.pluginId) {
      conditions.push('plugin_id = ?')
      params.push(filter.pluginId)
    }
    if (filter.level) {
      conditions.push('level = ?')
      params.push(filter.level)
    }
    const where = conditions.length ? ` WHERE ${conditions.join(' AND ')}` : ''
    const limit = Math.min(Math.max(filter.limit ?? 500, 1), MAX_LOG_ROWS_PER_PLUGIN)
    params.push(limit)
    const rows = dbQueryAll<{
      id: number
      pluginId: string
      level: string
      message: string
      timestamp: string
    }>(
      `SELECT id, plugin_id AS pluginId, level, message, timestamp
       FROM plugin_logs${where} ORDER BY timestamp DESC, id DESC LIMIT ?`,
      params
    )
    return rows.map((row) => ({
      id: row.id,
      pluginId: row.pluginId,
      level: row.level as PluginLogEntry['level'],
      message: row.message,
      timestamp: row.timestamp
    }))
  }

  clearLogs(pluginId?: string): void {
    if (pluginId) {
      dbExecute('DELETE FROM plugin_logs WHERE plugin_id = ?', [pluginId])
      return
    }
    dbExecute('DELETE FROM plugin_logs')
  }

  log(pluginId: string, level: string, message: string, _args: unknown[]): void {
    const trimmed =
      message.length > MAX_LOG_LENGTH ? `${message.slice(0, MAX_LOG_LENGTH)}…` : message
    try {
      dbExecute('INSERT INTO plugin_logs (plugin_id, level, message) VALUES (?, ?, ?)', [
        pluginId,
        level,
        trimmed
      ])
      this.trimPluginLogs(pluginId)
    } catch (err) {
      // db 已关闭/未初始化时降级为控制台输出，避免日志写入抛未捕获异常
      console.error('[plugin-log] drop:', (err as Error)?.message ?? err)
    }
    this.options.emitLog?.({ pluginId, level, message: trimmed })
  }

  /** Keep at most 2000 log rows per plugin to avoid unbounded growth. */
  private trimPluginLogs(pluginId: string): void {
    try {
      dbExecute(
        `DELETE FROM plugin_logs WHERE plugin_id = ? AND id NOT IN (
           SELECT id FROM plugin_logs WHERE plugin_id = ? ORDER BY id DESC LIMIT 2000
         )`,
        [pluginId, pluginId]
      )
    } catch {
      // ignore
    }
  }
}

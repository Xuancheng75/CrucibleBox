import type { PluginContext, PluginMain } from 'openbox-plugin-api'
import {
  normalizeDiaryText,
  parseDiaryDate,
  type DiaryDraft,
  type DiaryEntry,
  type DiaryMutationResult
} from './diary-domain'

interface DiaryMessage {
  type: string
  date?: string
  year?: number
  month?: number
  title?: string
  content?: string
}

let ctx: PluginContext | null = null
let mutationQueue: Promise<void> = Promise.resolve()

function storage() {
  if (!ctx) throw new Error('插件存储未初始化')
  return ctx.storage
}

async function runMutation<Result>(operation: () => Promise<Result>): Promise<Result> {
  const result = mutationQueue.then(operation, operation)
  mutationQueue = result.then(
    () => undefined,
    () => undefined
  )
  return await result
}

function invalidInput(message: string): DiaryMutationResult {
  return { ok: false, error: { code: 'INVALID_INPUT', message } }
}

function storageError(error: unknown): DiaryMutationResult {
  ctx?.logger.error('Diary storage mutation failed', error)
  return {
    ok: false,
    error: {
      code: 'STORAGE_ERROR',
      message: error instanceof Error ? error.message : 'Diary storage operation failed'
    }
  }
}

async function getEntryByDate(date: string): Promise<DiaryEntry | null> {
  return await storage().get<DiaryEntry>(`entry:${date}`)
}

async function getDraftByDate(date: string): Promise<DiaryDraft | null> {
  return await storage().get<DiaryDraft>(`draft:${date}`)
}

async function getEntriesInMonth(
  year: number,
  month: number
): Promise<{ entry_date: string; title: string }[]> {
  const m = String(month).padStart(2, '0')
  const prefix = `${year}-${m}`
  const entries = await storage().list<DiaryEntry>(`entry:${prefix}`)
  return entries
    .map(({ value }) => ({ entry_date: value.entry_date, title: value.title }))
    .sort((left, right) => left.entry_date.localeCompare(right.entry_date))
}

function formatDateHeader(dateStr: string): string {
  const date = parseDiaryDate(dateStr)
  if (!date) return dateStr
  const weekNames = ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六']
  return `${date.year}年${date.month}月${date.day}日 ${weekNames[date.weekday]}`
}

async function handleGetMonthEntries(msg: DiaryMessage) {
  const now = new Date()
  const year = msg.year ?? now.getFullYear()
  const month = msg.month ?? now.getMonth() + 1
  return { entries: await getEntriesInMonth(year, month) }
}

async function handleGetEntry(msg: DiaryMessage) {
  const date = parseDiaryDate(msg.date)
  if (!date) return { error: '缺少或无效的日期参数' }
  const [entry, draft] = await Promise.all([
    getEntryByDate(date.value),
    getDraftByDate(date.value)
  ])
  return { entry, draft }
}

async function handleSaveEntry(msg: DiaryMessage) {
  const date = parseDiaryDate(msg.date)
  const text = normalizeDiaryText(msg.title ?? '', msg.content ?? '')
  if (!date || !text) return invalidInput('Invalid diary date or content')
  const pluginStorage = storage()
  return await runMutation(async () => {
    const savedAt = new Date().toISOString()
    const deleted = text.title === '' && text.content === ''
    try {
      await pluginStorage.batch([
        deleted
          ? { type: 'delete', key: `entry:${date.value}` }
          : {
              type: 'set',
              key: `entry:${date.value}`,
              value: { entry_date: date.value, ...text } satisfies DiaryEntry
            },
        { type: 'delete', key: `draft:${date.value}` }
      ])
      return { ok: true, savedAt, deleted } satisfies DiaryMutationResult
    } catch (error) {
      return storageError(error)
    }
  })
}

async function handleSaveDraft(msg: DiaryMessage) {
  const date = parseDiaryDate(msg.date)
  const text = normalizeDiaryText(msg.title ?? '', msg.content ?? '')
  if (!date || !text) return invalidInput('Invalid diary draft')
  const pluginStorage = storage()
  return await runMutation(async () => {
    const savedAt = new Date().toISOString()
    try {
      await pluginStorage.set(`draft:${date.value}`, {
        date: date.value,
        ...text,
        updatedAt: savedAt
      } satisfies DiaryDraft)
      return { ok: true, savedAt, deleted: false } satisfies DiaryMutationResult
    } catch (error) {
      return storageError(error)
    }
  })
}

async function handleDiscardDraft(msg: DiaryMessage) {
  const date = parseDiaryDate(msg.date)
  if (!date) return invalidInput('Invalid diary date')
  const pluginStorage = storage()
  return await runMutation(async () => {
    const savedAt = new Date().toISOString()
    try {
      await pluginStorage.delete(`draft:${date.value}`)
      return { ok: true, savedAt, deleted: true } satisfies DiaryMutationResult
    } catch (error) {
      return storageError(error)
    }
  })
}

async function handleDeleteEntry(msg: DiaryMessage) {
  const date = parseDiaryDate(msg.date)
  if (!date) return invalidInput('Invalid diary date')
  const pluginStorage = storage()
  return await runMutation(async () => {
    const savedAt = new Date().toISOString()
    try {
      await pluginStorage.batch([
        { type: 'delete', key: `entry:${date.value}` },
        { type: 'delete', key: `draft:${date.value}` }
      ])
      return { ok: true, savedAt, deleted: true } satisfies DiaryMutationResult
    } catch (error) {
      return storageError(error)
    }
  })
}

async function handleExportSingle(msg: DiaryMessage) {
  const date = parseDiaryDate(msg.date)
  if (!date) return { error: '缺少或无效的日期参数' }
  const entry = await getEntryByDate(date.value)
  if (!entry) return { error: '该日期没有日记' }
  const header = formatDateHeader(date.value)
  return { content: `# ${header}\n\n## ${entry.title}\n\n${entry.content}` }
}

async function handleExportMonth(msg: DiaryMessage) {
  const now = new Date()
  const year = msg.year ?? now.getFullYear()
  const month = msg.month ?? now.getMonth() + 1
  const rows = await getEntriesInMonth(year, month)
  if (rows.length === 0) return { error: '该月没有日记' }

  const parts: string[] = []
  for (const row of rows) {
    const entry = await getEntryByDate(row.entry_date)
    if (!entry) continue
    parts.push(`# ${formatDateHeader(row.entry_date)}\n\n## ${entry.title}\n\n${entry.content}`)
  }
  return { content: parts.join('\n\n---\n\n') }
}

const plugin: PluginMain = {
  activate(pluginCtx: PluginContext) {
    ctx = pluginCtx
    mutationQueue = Promise.resolve()
    ctx.logger.info('日记插件已激活')
  },

  deactivate() {
    ctx = null
  },

  async onMessage(message: unknown) {
    const msg = message as DiaryMessage
    switch (msg.type) {
      case 'getMonthEntries':
        return await handleGetMonthEntries(msg)
      case 'getEntry':
        return await handleGetEntry(msg)
      case 'saveEntry':
        return await handleSaveEntry(msg)
      case 'saveDraft':
        return await handleSaveDraft(msg)
      case 'discardDraft':
        return await handleDiscardDraft(msg)
      case 'deleteEntry':
        return await handleDeleteEntry(msg)
      case 'exportSingle':
        return await handleExportSingle(msg)
      case 'exportMonth':
        return await handleExportMonth(msg)
      default:
        return { error: `未知消息类型: ${msg.type}` }
    }
  }
}

export default plugin

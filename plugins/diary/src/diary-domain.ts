export interface DiaryEntry {
  entry_date: string
  title: string
  content: string
}

export interface DiaryDraft {
  date: string
  title: string
  content: string
  updatedAt: string
}

export type DiaryMutationResult =
  | { ok: true; savedAt: string; deleted: boolean }
  | { ok: false; error: { code: 'INVALID_INPUT' | 'STORAGE_ERROR'; message: string } }

const DATE_PATTERN = /^(\d{4})-(\d{2})-(\d{2})$/
const MAX_TITLE_LENGTH = 512
const MAX_CONTENT_LENGTH = 512 * 1024

export function parseDiaryDate(value: unknown): {
  value: string
  year: number
  month: number
  day: number
  weekday: number
} | null {
  if (typeof value !== 'string') return null
  const match = DATE_PATTERN.exec(value)
  if (!match) return null
  const year = Number(match[1])
  const month = Number(match[2])
  const day = Number(match[3])
  if (year < 1970 || year > 9999 || month < 1 || month > 12 || day < 1 || day > 31) return null
  const utc = new Date(Date.UTC(year, month - 1, day))
  if (
    utc.getUTCFullYear() !== year ||
    utc.getUTCMonth() !== month - 1 ||
    utc.getUTCDate() !== day
  ) {
    return null
  }
  return { value, year, month, day, weekday: utc.getUTCDay() }
}

export function normalizeDiaryText(title: unknown, content: unknown): {
  title: string
  content: string
} | null {
  if (typeof title !== 'string' || typeof content !== 'string') return null
  if (title.length > MAX_TITLE_LENGTH || content.length > MAX_CONTENT_LENGTH) return null
  return { title, content }
}

export function shouldLeaveAfterSave(
  result: DiaryMutationResult,
  savedRevision: number,
  currentRevision: number
): boolean {
  return result.ok && savedRevision === currentRevision
}

export function isDiaryMutationResult(value: unknown): value is DiaryMutationResult {
  if (!value || typeof value !== 'object') return false
  const result = value as Record<string, unknown>
  if (result.ok === true) {
    return (
      typeof result.savedAt === 'string' &&
      typeof result.deleted === 'boolean' &&
      Object.keys(result).every((key) => ['ok', 'savedAt', 'deleted'].includes(key))
    )
  }
  if (result.ok !== false || !result.error || typeof result.error !== 'object') return false
  const error = result.error as Record<string, unknown>
  return (
    (error.code === 'INVALID_INPUT' || error.code === 'STORAGE_ERROR') &&
    typeof error.message === 'string'
  )
}

import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import {
  isDiaryMutationResult,
  type DiaryDraft,
  type DiaryEntry,
  type DiaryMutationResult
} from '../diary-domain'

let currentApi: PluginRenderProps['api'] | null = null

export function setApi(api: PluginRenderProps['api']) {
  currentApi = api
}

function api() {
  if (!currentApi) throw new Error('API 未初始化')
  return currentApi
}

export interface DiaryMonthEntry {
  entry_date: string
  title: string
}

export interface DiaryLoadResult {
  entry: DiaryEntry | null
  draft: DiaryDraft | null
}

function mutationResult(value: unknown): DiaryMutationResult {
  if (!isDiaryMutationResult(value)) {
    return {
      ok: false,
      error: { code: 'STORAGE_ERROR', message: 'Diary backend returned an invalid result' }
    }
  }
  return value
}

export async function getMonthEntries(year: number, month: number): Promise<DiaryMonthEntry[]> {
  const response = (await api().sendToBackend({
    type: 'getMonthEntries',
    year,
    month
  })) as { entries?: DiaryMonthEntry[] }
  return response.entries ?? []
}

export async function getEntry(date: string): Promise<DiaryLoadResult> {
  const response = (await api().sendToBackend({ type: 'getEntry', date })) as {
    entry?: DiaryEntry | null
    draft?: DiaryDraft | null
    error?: string
  }
  if (response.error) throw new Error(response.error)
  return { entry: response.entry ?? null, draft: response.draft ?? null }
}

export async function saveEntry(
  date: string,
  title: string,
  content: string
): Promise<DiaryMutationResult> {
  return mutationResult(
    await api().sendToBackend({ type: 'saveEntry', date, title, content })
  )
}

export async function saveDraft(
  date: string,
  title: string,
  content: string
): Promise<DiaryMutationResult> {
  return mutationResult(
    await api().sendToBackend({ type: 'saveDraft', date, title, content })
  )
}

export async function discardDraft(date: string): Promise<DiaryMutationResult> {
  return mutationResult(await api().sendToBackend({ type: 'discardDraft', date }))
}

export async function deleteEntry(date: string): Promise<DiaryMutationResult> {
  return mutationResult(await api().sendToBackend({ type: 'deleteEntry', date }))
}

export async function exportSingle(date: string): Promise<string> {
  const response = (await api().sendToBackend({ type: 'exportSingle', date })) as {
    content?: string
    error?: string
  }
  if (response.error) throw new Error(response.error)
  return response.content ?? ''
}

export async function exportMonth(year: number, month: number): Promise<string> {
  const response = (await api().sendToBackend({ type: 'exportMonth', year, month })) as {
    content?: string
    error?: string
  }
  if (response.error) throw new Error(response.error)
  return response.content ?? ''
}

import type { TaskSnapshot, TaskStatus } from './task-manager'

export const TASK_POLL_INTERVAL_MS = 1_000
export const TASK_POLL_TIMEOUT_MS = 90 * 60 * 1_000

const TERMINAL_STATUSES = new Set<TaskStatus>(['succeeded', 'failed', 'cancelled'])
const TASK_STATUSES = new Set<TaskStatus>(['queued', 'running', 'succeeded', 'failed', 'cancelled'])

export interface PollTaskOptions<TResult = unknown, TProgress = unknown> {
  taskId: string
  signal: AbortSignal
  fetchTask(taskId: string): Promise<unknown>
  onSnapshot(snapshot: TaskSnapshot<TResult, TProgress>): void
  intervalMs?: number
  timeoutMs?: number
  now?: () => number
}

export class TaskPollingAbortedError extends Error {
  constructor() {
    super('Task polling was aborted')
    this.name = 'TaskPollingAbortedError'
  }
}

export class TaskPollingTimeoutError extends Error {
  constructor(taskId: string) {
    super(`Timed out while polling task "${taskId}"`)
    this.name = 'TaskPollingTimeoutError'
  }
}

export function readStartedTaskId(response: unknown): string {
  const record = requireRecord(response, 'Invalid task start response')
  throwResponseError(record)
  if (record.success !== true || typeof record.taskId !== 'string' || !record.taskId) {
    throw new Error('Task start response did not contain a task id')
  }
  return record.taskId
}

export function assertTaskCancellationAccepted(response: unknown, taskId: string): void {
  const record = requireRecord(response, 'Invalid task cancellation response')
  throwResponseError(record)
  if (record.success !== true || record.taskId !== taskId) {
    throw new Error('Task cancellation was not accepted')
  }
}

export function readTaskSnapshot<TResult = unknown, TProgress = unknown>(
  response: unknown
): TaskSnapshot<TResult, TProgress> {
  const record = requireRecord(response, 'Invalid task snapshot response')
  throwResponseError(record)
  if (
    typeof record.taskId !== 'string' ||
    typeof record.resourceKey !== 'string' ||
    !TASK_STATUSES.has(record.status as TaskStatus) ||
    typeof record.createdAt !== 'number'
  ) {
    throw new Error('Invalid task snapshot response')
  }
  return record as unknown as TaskSnapshot<TResult, TProgress>
}

export function isTerminalTaskStatus(status: TaskStatus): boolean {
  return TERMINAL_STATUSES.has(status)
}

export function isTaskPollingAborted(error: unknown): boolean {
  return error instanceof TaskPollingAbortedError
}

export async function pollTask<TResult = unknown, TProgress = unknown>({
  taskId,
  signal,
  fetchTask,
  onSnapshot,
  intervalMs = TASK_POLL_INTERVAL_MS,
  timeoutMs = TASK_POLL_TIMEOUT_MS,
  now = Date.now
}: PollTaskOptions<TResult, TProgress>): Promise<TaskSnapshot<TResult, TProgress>> {
  if (intervalMs <= 0 || timeoutMs <= 0) {
    throw new RangeError('Task polling intervals must be positive')
  }

  const startedAt = now()
  while (!signal.aborted) {
    throwIfAborted(signal)
    const snapshot = readTaskSnapshot<TResult, TProgress>(await fetchTask(taskId))
    throwIfAborted(signal)
    if (snapshot.taskId !== taskId) {
      throw new Error(`Received snapshot for unexpected task "${snapshot.taskId}"`)
    }

    onSnapshot(snapshot)
    if (isTerminalTaskStatus(snapshot.status)) return snapshot

    const remainingMs = timeoutMs - (now() - startedAt)
    if (remainingMs <= 0) throw new TaskPollingTimeoutError(taskId)
    await abortableDelay(Math.min(intervalMs, remainingMs), signal)
  }
  throw new TaskPollingAbortedError()
}

function requireRecord(value: unknown, message: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(message)
  }
  return value as Record<string, unknown>
}

function throwResponseError(record: Record<string, unknown>): void {
  if (typeof record.error !== 'string') return
  const error = new Error(record.error)
  if (typeof record.code === 'string') {
    ;(error as Error & { code?: string }).code = record.code
  }
  throw error
}

function throwIfAborted(signal: AbortSignal): void {
  if (signal.aborted) throw new TaskPollingAbortedError()
}

function abortableDelay(delayMs: number, signal: AbortSignal): Promise<void> {
  throwIfAborted(signal)
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      signal.removeEventListener('abort', onAbort)
      resolve()
    }, delayMs)
    const onAbort = () => {
      clearTimeout(timeout)
      signal.removeEventListener('abort', onAbort)
      reject(new TaskPollingAbortedError())
    }
    signal.addEventListener('abort', onAbort, { once: true })
  })
}

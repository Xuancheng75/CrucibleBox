// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { randomUUID } from 'node:crypto'

export type TaskStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled'

export interface SerializedTaskError {
  name: string
  message: string
  stack?: string
  code?: string | number
}

export interface TaskSnapshot<TResult = unknown, TProgress = unknown> {
  taskId: string
  resourceKey: string
  status: TaskStatus
  createdAt: number
  startedAt?: number
  completedAt?: number
  progress?: TProgress
  result?: TResult
  error?: SerializedTaskError
}

export interface TaskContext<TProgress = unknown> {
  taskId: string
  resourceKey: string
  signal: AbortSignal
  updateProgress(value: TProgress): boolean
}

export type TaskExecutor<TResult, TProgress = unknown> = (
  context: TaskContext<TProgress>
) => TResult | Promise<TResult>

export interface StartTaskOptions {
  signal?: AbortSignal
}

export interface TaskHandle<TResult, TProgress = unknown> {
  taskId: string
  signal: AbortSignal
  completion: Promise<TaskSnapshot<TResult, TProgress>>
  cancel(reason?: unknown): boolean
}

export interface TaskManagerOptions {
  maxRetainedTasks?: number
  createTaskId?: () => string
  now?: () => number
}

interface InternalTask {
  controller: AbortController
  detachExternalAbort?: () => void
  execute: TaskExecutor<unknown>
  executionStarted: boolean
  resolveCompletion: (snapshot: TaskSnapshot<unknown>) => void
  snapshot: TaskSnapshot<unknown>
  settled: boolean
}

const DEFAULT_MAX_RETAINED_TASKS = 100

export class DuplicateResourceTaskError extends Error {
  readonly resourceKey: string
  readonly taskId: string

  constructor(resourceKey: string, taskId: string) {
    super(`Resource "${resourceKey}" is already owned by task "${taskId}"`)
    this.name = 'DuplicateResourceTaskError'
    this.resourceKey = resourceKey
    this.taskId = taskId
  }
}

export class TaskManager {
  private readonly activeResources = new Map<string, string>()
  private readonly createTaskId: () => string
  private readonly maxRetainedTasks: number
  private readonly now: () => number
  private readonly tasks = new Map<string, InternalTask>()
  private readonly terminalTaskIds: string[] = []

  constructor(options: TaskManagerOptions = {}) {
    const maxRetainedTasks = options.maxRetainedTasks ?? DEFAULT_MAX_RETAINED_TASKS
    if (!Number.isSafeInteger(maxRetainedTasks) || maxRetainedTasks < 0) {
      throw new RangeError('maxRetainedTasks must be a non-negative safe integer')
    }

    this.maxRetainedTasks = maxRetainedTasks
    this.createTaskId = options.createTaskId ?? randomUUID
    this.now = options.now ?? Date.now
  }

  start<TResult, TProgress = unknown>(
    resourceKey: string,
    execute: TaskExecutor<TResult, TProgress>,
    options: StartTaskOptions = {}
  ): TaskHandle<TResult, TProgress> {
    const normalizedResourceKey = resourceKey.trim()
    if (!normalizedResourceKey) throw new TypeError('resourceKey must not be empty')

    const activeTaskId = this.activeResources.get(normalizedResourceKey)
    if (activeTaskId) {
      throw new DuplicateResourceTaskError(normalizedResourceKey, activeTaskId)
    }

    const taskId = this.createTaskId()
    if (!taskId || this.tasks.has(taskId)) {
      throw new Error(`Task id collision: "${taskId}"`)
    }

    const controller = new AbortController()
    let resolveCompletion!: (snapshot: TaskSnapshot<unknown>) => void
    const completion = new Promise<TaskSnapshot<unknown>>((resolve) => {
      resolveCompletion = resolve
    })
    const task: InternalTask = {
      controller,
      execute: (context) => execute(context as TaskContext<TProgress>),
      executionStarted: false,
      resolveCompletion,
      settled: false,
      snapshot: {
        taskId,
        resourceKey: normalizedResourceKey,
        status: 'queued',
        createdAt: this.now()
      }
    }

    this.tasks.set(taskId, task)
    this.activeResources.set(normalizedResourceKey, taskId)
    this.attachExternalSignal(task, options.signal)
    queueMicrotask(() => void this.executeTask(task))

    return {
      taskId,
      signal: controller.signal,
      completion: completion as Promise<TaskSnapshot<TResult, TProgress>>,
      cancel: (reason?: unknown) => this.cancel(taskId, reason)
    }
  }

  cancel(taskId: string, reason?: unknown): boolean {
    const task = this.tasks.get(taskId)
    if (!task || (task.snapshot.status !== 'queued' && task.snapshot.status !== 'running')) {
      return false
    }

    const cancelledBeforeStart = !task.executionStarted
    task.controller.abort(reason)
    this.completeTask(task, 'cancelled', undefined, serializeCancellation(reason))
    if (cancelledBeforeStart) this.releaseResource(task)
    return true
  }

  getTask<TResult = unknown, TProgress = unknown>(
    taskId: string
  ): TaskSnapshot<TResult, TProgress> | undefined {
    const task = this.tasks.get(taskId)
    return task ? (cloneSnapshot(task.snapshot) as TaskSnapshot<TResult, TProgress>) : undefined
  }

  listTasks(): TaskSnapshot[] {
    return [...this.tasks.values()].map((task) => cloneSnapshot(task.snapshot))
  }

  getActiveTaskId(resourceKey: string): string | undefined {
    return this.activeResources.get(resourceKey.trim())
  }

  private attachExternalSignal(task: InternalTask, signal?: AbortSignal): void {
    if (!signal) return
    const onAbort = () => this.cancel(task.snapshot.taskId, signal.reason)
    signal.addEventListener('abort', onAbort, { once: true })
    task.detachExternalAbort = () => signal.removeEventListener('abort', onAbort)
    if (signal.aborted) onAbort()
  }

  private async executeTask(task: InternalTask): Promise<void> {
    if (task.snapshot.status !== 'queued') {
      this.releaseResource(task)
      return
    }

    task.executionStarted = true
    task.snapshot = {
      ...task.snapshot,
      status: 'running',
      startedAt: this.now()
    }

    try {
      const result = await task.execute({
        taskId: task.snapshot.taskId,
        resourceKey: task.snapshot.resourceKey,
        signal: task.controller.signal,
        updateProgress: (value) => this.updateProgress(task, value)
      })
      if (task.snapshot.status === 'running') {
        this.completeTask(task, 'succeeded', result)
      }
    } catch (error) {
      if (task.snapshot.status === 'running') {
        this.completeTask(task, 'failed', undefined, serializeTaskError(error))
      }
    } finally {
      this.releaseResource(task)
    }
  }

  private completeTask(
    task: InternalTask,
    status: 'succeeded' | 'failed' | 'cancelled',
    result?: unknown,
    error?: SerializedTaskError
  ): void {
    if (task.settled) return
    task.settled = true
    task.detachExternalAbort?.()
    task.detachExternalAbort = undefined
    task.snapshot = {
      ...task.snapshot,
      status,
      completedAt: this.now(),
      ...(status === 'succeeded' ? { result } : {}),
      ...(error ? { error } : {})
    }
    const completedSnapshot = cloneSnapshot(task.snapshot)
    task.resolveCompletion(completedSnapshot)
    this.terminalTaskIds.push(task.snapshot.taskId)
    this.pruneTerminalTasks()
  }

  private updateProgress(task: InternalTask, value: unknown): boolean {
    if (task.snapshot.status !== 'queued' && task.snapshot.status !== 'running') {
      return false
    }
    task.snapshot = {
      ...task.snapshot,
      progress: toSerializableValue(value)
    }
    return true
  }

  private pruneTerminalTasks(): void {
    while (this.terminalTaskIds.length > this.maxRetainedTasks) {
      const taskId = this.terminalTaskIds.shift()
      if (taskId) this.tasks.delete(taskId)
    }
  }

  private releaseResource(task: InternalTask): void {
    if (this.activeResources.get(task.snapshot.resourceKey) === task.snapshot.taskId) {
      this.activeResources.delete(task.snapshot.resourceKey)
    }
  }
}

export function serializeTaskError(error: unknown): SerializedTaskError {
  if (error instanceof Error) {
    const code = readErrorCode(error)
    return {
      name: error.name || 'Error',
      message: error.message || error.name || 'Unknown task error',
      ...(error.stack ? { stack: error.stack } : {}),
      ...(code !== undefined ? { code } : {})
    }
  }

  if (isRecord(error)) {
    const name = typeof error.name === 'string' && error.name ? error.name : 'TaskError'
    const message =
      typeof error.message === 'string' && error.message ? error.message : safeDescribe(error)
    const stack = typeof error.stack === 'string' ? error.stack : undefined
    const code = readErrorCode(error)
    return {
      name,
      message,
      ...(stack ? { stack } : {}),
      ...(code !== undefined ? { code } : {})
    }
  }

  return {
    name: 'TaskError',
    message: typeof error === 'string' ? error : safeDescribe(error)
  }
}

function serializeCancellation(reason: unknown): SerializedTaskError {
  const serialized = reason === undefined ? undefined : serializeTaskError(reason)
  return {
    name: 'AbortError',
    message: serialized?.message || 'Task cancelled',
    ...(serialized?.stack ? { stack: serialized.stack } : {}),
    ...(serialized?.code !== undefined ? { code: serialized.code } : {})
  }
}

function cloneSnapshot<TResult, TProgress>(
  snapshot: TaskSnapshot<TResult, TProgress>
): TaskSnapshot<TResult, TProgress> {
  return {
    ...snapshot,
    ...(snapshot.progress !== undefined
      ? { progress: toSerializableValue(snapshot.progress) as TProgress }
      : {}),
    ...(snapshot.error ? { error: { ...snapshot.error } } : {})
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function readErrorCode(value: object): string | number | undefined {
  const code = (value as { code?: unknown }).code
  return typeof code === 'string' || typeof code === 'number' ? code : undefined
}

function safeDescribe(value: unknown): string {
  const seen = new WeakSet<object>()
  try {
    const serialized = JSON.stringify(value, (_key, current: unknown) => {
      if (typeof current === 'bigint') return `${current}n`
      if (typeof current === 'symbol' || typeof current === 'function') return String(current)
      if (isRecord(current)) {
        if (seen.has(current)) return '[Circular]'
        seen.add(current)
      }
      return current
    })
    return serialized ?? String(value)
  } catch {
    return String(value)
  }
}

function toSerializableValue(value: unknown): unknown {
  const description = safeDescribe(value)
  try {
    return JSON.parse(description) as unknown
  } catch {
    return description
  }
}

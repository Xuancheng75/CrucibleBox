import { describe, expect, it, vi } from 'vitest'
import {
  DuplicateResourceTaskError,
  TaskManager,
  type TaskSnapshot
} from '../../../plugin-system/trusted-services/unienv/task-manager'

interface Deferred<T> {
  promise: Promise<T>
  reject(reason?: unknown): void
  resolve(value: T): void
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((resolve_, reject_) => {
    resolve = resolve_
    reject = reject_
  })
  return { promise, reject, resolve }
}

function createManager(maxRetainedTasks = 100): TaskManager {
  let id = 0
  let time = 1_000
  return new TaskManager({
    maxRetainedTasks,
    createTaskId: () => `task-${++id}`,
    now: () => ++time
  })
}

async function flushMicrotasks(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

describe('TaskManager', () => {
  it('moves a successful task through queued/running/succeeded with a task id', async () => {
    const manager = createManager()
    const gate = deferred<{ version: string }>()
    const task = manager.start<{ version: string }, { percent: number }>(
      'python',
      ({ taskId, resourceKey, signal, updateProgress }) => {
        expect(taskId).toBe('task-1')
        expect(resourceKey).toBe('python')
        expect(signal.aborted).toBe(false)
        expect(updateProgress({ percent: 25 })).toBe(true)
        const mutableProgress = { percent: 100 }
        expect(updateProgress(mutableProgress)).toBe(true)
        mutableProgress.percent = 999
        return gate.promise
      }
    )

    expect(task.taskId).toBe('task-1')
    expect(manager.getTask(task.taskId)?.status).toBe('queued')
    await flushMicrotasks()
    expect(manager.getTask(task.taskId)?.status).toBe('running')

    gate.resolve({ version: '3.13.0' })
    const completed = await task.completion
    expect(completed).toMatchObject({
      taskId: 'task-1',
      resourceKey: 'python',
      status: 'succeeded',
      progress: { percent: 100 },
      result: { version: '3.13.0' }
    })
    expect(completed.startedAt).toBeGreaterThan(completed.createdAt)
    expect(completed.completedAt).toBeGreaterThan(completed.startedAt!)
    completed.progress!.percent = 0
    expect(
      manager.getTask<{ version: string }, { percent: number }>(task.taskId)?.progress
    ).toEqual({ percent: 100 })
  })

  it('serializes thrown values into a JSON-safe failed task', async () => {
    const manager = createManager()
    const failure: Record<string, unknown> = {
      name: 'InstallError',
      message: 'archive verification failed',
      code: 'E_CHECKSUM'
    }
    failure.context = failure

    const completed = await manager.start('node', () => {
      throw failure
    }).completion

    expect(completed.status).toBe('failed')
    expect(completed.result).toBeUndefined()
    expect(completed.error).toEqual({
      name: 'InstallError',
      message: 'archive verification failed',
      code: 'E_CHECKSUM'
    })
    expect(() => JSON.stringify(completed)).not.toThrow()
  })

  it('rejects a duplicate resource key until the owning task settles', async () => {
    const manager = createManager()
    const gate = deferred<string>()
    const first = manager.start('go', () => gate.promise)

    expect(() => manager.start('go', () => 'duplicate')).toThrowError(
      new DuplicateResourceTaskError('go', first.taskId)
    )

    gate.resolve('installed')
    await first.completion
    const replacement = manager.start('go', () => 'already installed')
    expect((await replacement.completion).status).toBe('succeeded')
  })

  it('uses an external AbortSignal to cancel a running task', async () => {
    const manager = createManager()
    const externalController = new AbortController()
    const abortObserved = deferred<void>()
    const task = manager.start(
      'java',
      ({ signal }) =>
        new Promise<never>((_resolve, reject) => {
          signal.addEventListener(
            'abort',
            () => {
              abortObserved.resolve()
              reject(signal.reason)
            },
            { once: true }
          )
        }),
      { signal: externalController.signal }
    )
    await flushMicrotasks()

    externalController.abort(new Error('cancelled by user'))
    const completed = await task.completion
    await abortObserved.promise

    expect(task.signal.aborted).toBe(true)
    expect(completed.status).toBe('cancelled')
    expect(completed.error).toMatchObject({ name: 'AbortError', message: 'cancelled by user' })
    expect(task.cancel()).toBe(false)
  })

  it('does not start work when its external signal is already aborted', async () => {
    const manager = createManager()
    const externalController = new AbortController()
    const execute = vi.fn(() => 'should not run')
    externalController.abort('cancel before queue runs')

    const task = manager.start('git', execute, { signal: externalController.signal })
    const completed = await task.completion
    await flushMicrotasks()

    expect(completed.status).toBe('cancelled')
    expect(execute).not.toHaveBeenCalled()
    expect(manager.getActiveTaskId('git')).toBeUndefined()
  })

  it('ignores a late result and holds the resource lock until execution really ends', async () => {
    const manager = createManager()
    const gate = deferred<string>()
    let updateProgress!: (value: { phase: string }) => boolean
    const task = manager.start<string, { phase: string }>('python', (context) => {
      updateProgress = context.updateProgress
      expect(updateProgress({ phase: 'running' })).toBe(true)
      return gate.promise
    })
    await flushMicrotasks()
    expect(manager.getTask(task.taskId)?.status).toBe('running')

    expect(task.cancel('user cancelled')).toBe(true)
    const cancelled = await task.completion
    expect(cancelled.status).toBe('cancelled')
    expect(cancelled.progress).toEqual({ phase: 'running' })
    expect(updateProgress({ phase: 'late update' })).toBe(false)
    expect(() => manager.start('python', () => 'too early')).toThrow(DuplicateResourceTaskError)

    gate.resolve('late success')
    await flushMicrotasks()

    expect(manager.getTask(task.taskId)).toMatchObject({
      status: 'cancelled',
      progress: { phase: 'running' },
      error: { name: 'AbortError', message: 'user cancelled' }
    })
    expect(manager.getTask(task.taskId)?.result).toBeUndefined()
    expect(manager.getActiveTaskId('python')).toBeUndefined()
    expect((await manager.start('python', () => 'fresh').completion).result).toBe('fresh')
  })

  it('retains only the configured number of terminal task snapshots', async () => {
    const manager = createManager(2)
    const completed: TaskSnapshot[] = []

    for (const resource of ['python', 'node', 'go']) {
      completed.push(await manager.start(resource, () => resource).completion)
    }

    expect(manager.getTask(completed[0].taskId)).toBeUndefined()
    expect(manager.listTasks().map((task) => task.taskId)).toEqual(['task-2', 'task-3'])
  })
})

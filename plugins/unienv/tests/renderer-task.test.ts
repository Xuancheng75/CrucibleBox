import { describe, expect, it, vi } from 'vitest'
import {
  assertTaskCancellationAccepted,
  pollTask,
  readStartedTaskId,
  TaskPollingAbortedError,
  TaskPollingTimeoutError
} from '../src/renderer-task'
import type { TaskSnapshot } from '../../../plugin-system/trusted-services/unienv/task-manager'

function snapshot(status: TaskSnapshot['status'], progress?: { percent: number }): TaskSnapshot {
  return {
    taskId: 'task-1',
    resourceKey: 'installation',
    status,
    createdAt: 1,
    ...(progress ? { progress } : {})
  }
}

describe('renderer task protocol', () => {
  it('reads the frozen start and cancellation responses', () => {
    expect(readStartedTaskId({ success: true, taskId: 'task-1', message: 'created' })).toBe(
      'task-1'
    )
    expect(() =>
      assertTaskCancellationAccepted({ success: true, taskId: 'task-1' }, 'task-1')
    ).not.toThrow()
  })

  it('surfaces backend error responses', () => {
    expect(() => readStartedTaskId({ error: 'already running', code: 'task-conflict' })).toThrow(
      'already running'
    )
    expect(() =>
      assertTaskCancellationAccepted(
        { error: 'task ended', code: 'task-not-cancellable' },
        'task-1'
      )
    ).toThrow('task ended')
  })
})

describe('pollTask', () => {
  it('publishes progress and resolves at the first terminal snapshot', async () => {
    vi.useFakeTimers()
    const fetchTask = vi
      .fn<() => Promise<TaskSnapshot>>()
      .mockResolvedValueOnce(snapshot('running', { percent: 25 }))
      .mockResolvedValueOnce(snapshot('succeeded', { percent: 100 }))
    const onSnapshot = vi.fn()
    const controller = new AbortController()

    const polling = pollTask({
      taskId: 'task-1',
      signal: controller.signal,
      fetchTask,
      onSnapshot
    })
    await vi.advanceTimersByTimeAsync(1_000)

    await expect(polling).resolves.toMatchObject({ status: 'succeeded' })
    expect(onSnapshot).toHaveBeenCalledTimes(2)
    expect(onSnapshot.mock.calls[0][0]).toMatchObject({ progress: { percent: 25 } })
    vi.useRealTimers()
  })

  it('clears its delay and stops publishing after cancellation', async () => {
    vi.useFakeTimers()
    const controller = new AbortController()
    const onSnapshot = vi.fn()
    const fetchTask = vi.fn().mockResolvedValue(snapshot('running'))
    const polling = pollTask({
      taskId: 'task-1',
      signal: controller.signal,
      fetchTask,
      onSnapshot
    })
    await vi.advanceTimersByTimeAsync(0)
    expect(onSnapshot).toHaveBeenCalledTimes(1)

    controller.abort()
    await expect(polling).rejects.toBeInstanceOf(TaskPollingAbortedError)
    await vi.advanceTimersByTimeAsync(5_000)
    expect(fetchTask).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(0)
    vi.useRealTimers()
  })

  it('does not publish an in-flight response after cancellation', async () => {
    let resolveFetch!: (value: TaskSnapshot) => void
    const fetchTask = vi.fn(
      () =>
        new Promise<TaskSnapshot>((resolve) => {
          resolveFetch = resolve
        })
    )
    const controller = new AbortController()
    const onSnapshot = vi.fn()
    const polling = pollTask({
      taskId: 'task-1',
      signal: controller.signal,
      fetchTask,
      onSnapshot
    })

    controller.abort()
    resolveFetch(snapshot('succeeded'))
    await expect(polling).rejects.toBeInstanceOf(TaskPollingAbortedError)
    expect(onSnapshot).not.toHaveBeenCalled()
  })

  it('stops at the configured polling deadline', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(0)
    const controller = new AbortController()
    const polling = pollTask({
      taskId: 'task-1',
      signal: controller.signal,
      fetchTask: vi.fn().mockResolvedValue(snapshot('running')),
      onSnapshot: vi.fn(),
      intervalMs: 1_000,
      timeoutMs: 2_500
    })
    const rejection = expect(polling).rejects.toBeInstanceOf(TaskPollingTimeoutError)

    await vi.advanceTimersByTimeAsync(2_500)
    await rejection
    vi.useRealTimers()
  })
})

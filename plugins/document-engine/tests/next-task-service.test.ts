import { describe, expect, it, vi } from 'vitest'
import { taskService } from '../src/next-task-service'
import type { createClient, TaskSnapshot } from '../vendor/next-api/src/index.mjs'
const state = (status: TaskSnapshot['status'], sequence: number): TaskSnapshot => ({
  taskId: 'one',
  resourceKey: 'work',
  status,
  sequence,
  cancelRequested: false,
  resultRefs: []
})
const client = (get: ReturnType<typeof vi.fn>, cancel = vi.fn(), list = vi.fn()) =>
  ({ tasks: { get, cancel, list } }) as unknown as ReturnType<typeof createClient>
describe('Next authoritative task consumers', () => {
  it('retains terminal state when late responses regress sequence and never attaches a running result', async () => {
    const get = vi
      .fn()
      .mockResolvedValueOnce(state('succeeded', 3))
      .mockResolvedValueOnce(state('succeeded', 3))
      .mockResolvedValue(state('running', 2))
    const domain = vi
      .fn()
      .mockResolvedValueOnce({ ...state('succeeded', 3), result: { path: 'output' } })
      .mockResolvedValue({ ...state('running', 2), result: { partial: true } })
    const call = taskService(client(get), 'document', domain)
    expect(await call({ type: 'document.jobs.get', taskId: 'one' })).toMatchObject({
      status: 'succeeded',
      result: { path: 'output' }
    })
    const late = await call({ type: 'document.jobs.get', taskId: 'one' })
    expect(late).toMatchObject({ status: 'succeeded', sequence: 3 })
    expect(late).not.toHaveProperty('result')
  })
  it('returns interrupted publication references without invoking an unavailable domain worker', async () => {
    const get = vi.fn().mockResolvedValue({
      ...state('interrupted', 4),
      resultRefs: ['C:/output/part'],
      createdAt: 1
    })
    const domain = vi.fn()
    const call = taskService(client(get), 'archive', domain)
    expect(await call({ type: 'getTask', taskId: 'one' })).toMatchObject({
      ok: true,
      data: { task: { status: 'interrupted', resultRefs: ['C:/output/part'] } }
    })
    expect(domain).not.toHaveBeenCalled()
  })
  it('uses owner-scoped cancellation and reports refusal without retrying the domain mutation', async () => {
    const cancel = vi.fn().mockResolvedValue({ accepted: false, task: null })
    const domain = vi.fn()
    const call = taskService(client(vi.fn(), cancel), 'environment', domain)
    expect(await call({ type: 'cancelTask', taskId: 'one' })).toMatchObject({
      success: false,
      taskId: 'one'
    })
    expect(cancel).toHaveBeenCalledExactlyOnceWith('one')
    expect(domain).not.toHaveBeenCalled()
  })
  it('consumes every task page and rejects a repeated cursor', async () => {
    const list = vi
      .fn()
      .mockResolvedValueOnce({ items: [state('running', 1)], nextCursor: 'one' })
      .mockResolvedValueOnce({ items: [], nextCursor: null })
    const call = taskService(client(vi.fn(), vi.fn(), list), 'document', vi.fn())
    expect(await call({ type: 'document.jobs.list' })).toMatchObject({ tasks: [{ taskId: 'one' }] })
    expect(list).toHaveBeenNthCalledWith(2, { limit: 20, after: 'one' })
    list.mockResolvedValue({ items: [], nextCursor: 'one' })
    await expect(call({ type: 'document.jobs.list' })).rejects.toThrow('INVALID_RESPONSE')
  })
  it('refuses an absent or foreign task before accessing domain details', async () => {
    const domain = vi.fn()
    const call = taskService(client(vi.fn().mockResolvedValue(null)), 'document', domain)
    await expect(call({ type: 'document.jobs.get', taskId: 'foreign' })).rejects.toThrow(
      'TASK_NOT_FOUND'
    )
    expect(domain).not.toHaveBeenCalled()
  })
  it('rejects terminal regression after the full snapshot cache has been evicted', async () => {
    const get = vi
      .fn()
      .mockImplementation(async (taskId: string) => ({ ...state('interrupted', 4), taskId }))
    const domain = vi.fn()
    const call = taskService(client(get), 'document', domain)
    for (let index = 0; index < 130; index++)
      await call({ type: 'document.jobs.get', taskId: `task-${index}` })
    get.mockResolvedValue({ ...state('running', 3), taskId: 'task-0' })
    await expect(call({ type: 'document.jobs.get', taskId: 'task-0' })).rejects.toThrow(
      'STALE_TASK_SNAPSHOT'
    )
    expect(domain).not.toHaveBeenCalled()
  })
})

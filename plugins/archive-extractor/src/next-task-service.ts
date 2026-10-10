import type { createClient, TaskSnapshot } from '../vendor/next-api/src/index.mjs'
type Client = ReturnType<typeof createClient>
type Message = { type: string; [key: string]: unknown }
// The owner-scoped runtime is authoritative; domain calls supply only full result details.
export function taskService(
  client: Client,
  kind: 'archive' | 'environment' | 'document',
  domain: (message: Message) => Promise<unknown>
) {
  const seen = new Map<string, TaskSnapshot>()
  const ledger = new Map<string, { sequence: number; status: TaskSnapshot['status'] }>()
  const observe = (value: TaskSnapshot) => {
    const previous = ledger.get(value.taskId)
    const stale =
      previous &&
      (value.sequence < previous.sequence ||
        (['succeeded', 'failed', 'cancelled', 'interrupted'].includes(previous.status) &&
          value.status !== previous.status))
    if (stale) {
      const cached = seen.get(value.taskId)
      if (cached) return cached
      throw Error('STALE_TASK_SNAPSHOT')
    }
    if (!previous && ledger.size >= 10000) throw Error('TASK_LIST_LIMIT')
    ledger.set(value.taskId, { sequence: value.sequence, status: value.status })
    seen.set(value.taskId, value)
    if (seen.size > 128) seen.delete(seen.keys().next().value!)
    return value
  }
  const read = async (taskId: string) => {
    const state = await client.tasks.get(taskId)
    if (!state) throw Error('TASK_NOT_FOUND')
    const snapshot = observe(state)
    if (snapshot.status === 'interrupted') return snapshot
    const response = await domain({
      type: kind === 'document' ? 'document.jobs.get' : 'getTask',
      taskId
    })
    const detail =
      kind === 'archive' ? (response as { data?: { task?: unknown } })?.data?.task : response
    if (!detail || typeof detail !== 'object' || Array.isArray(detail))
      throw Error('INVALID_RESPONSE')
    const current = await client.tasks.get(taskId)
    if (!current) throw Error('TASK_NOT_FOUND')
    const latest = observe(current)
    const record = detail as Record<string, unknown>
    if (record.taskId !== taskId) throw Error('INVALID_RESPONSE')
    // Never attach an earlier running result to a later terminal state.
    const { result, ...fields } = record
    return {
      ...fields,
      ...latest,
      ...(record.status === latest.status && result !== undefined ? { result } : {})
    }
  }
  return async (message: Message): Promise<unknown> => {
    const get = kind === 'document' ? 'document.jobs.get' : 'getTask'
    const cancel =
      kind === 'document' ? 'document.jobs.cancel' : kind === 'archive' ? 'cancel' : 'cancelTask'
    if (message.type === get) {
      if (typeof message.taskId !== 'string') throw Error('INVALID_REQUEST')
      const task = await read(message.taskId)
      return kind === 'archive' ? { ok: true, data: { task } } : task
    }
    if (message.type === cancel) {
      if (typeof message.taskId !== 'string') throw Error('INVALID_REQUEST')
      const result = await client.tasks.cancel(message.taskId)
      if (result.task) observe(result.task)
      const response = {
        success: result.accepted,
        taskId: message.taskId,
        ...(!result.accepted ? { error: 'Task cancellation was not accepted' } : {})
      }
      return kind === 'archive'
        ? { ok: result.accepted, data: response, message: response.error }
        : response
    }
    if (kind === 'document' && message.type === 'document.jobs.list') {
      const tasks: TaskSnapshot[] = []
      let after: string | undefined
      do {
        const page = await client.tasks.list({ limit: 20, ...(after ? { after } : {}) })
        tasks.push(...page.items.map(observe))
        if (
          page.nextCursor !== null &&
          (!page.nextCursor || (after !== undefined && page.nextCursor <= after))
        )
          throw Error('INVALID_RESPONSE')
        after = page.nextCursor ?? undefined
        if (tasks.length > 10000) throw Error('TASK_LIST_LIMIT')
      } while (after)
      return { tasks }
    }
    return domain(message)
  }
}

import { create } from 'zustand'
import { tauriApi, type HostTaskEventPayload, type HostTaskMutation } from '../api/tauriApi'

export type HostTaskStatus =
  | 'queued'
  | 'running'
  | 'paused'
  | 'waiting-user'
  | 'completed'
  | 'failed'
  | 'cancelled'

export interface HostTask {
  id: string
  title: string
  detail?: string
  source: 'host' | 'plugin' | 'marketplace' | 'update'
  status: HostTaskStatus
  progress?: number
  sequence?: number
  stage?: string
  owner?: string
  kind?: string
  resultRefs?: string[]
  createdAt: number
  updatedAt: number
  error?: string
}

interface TaskState {
  tasks: HostTask[]
  upsertTask: (task: Omit<HostTask, 'createdAt' | 'updatedAt'> & { createdAt?: number }) => void
  patchTask: (id: string, patch: Partial<Omit<HostTask, 'id' | 'createdAt'>>) => void
  mergeSnapshot: (task: HostTaskEventPayload) => void
  applyEvent: (task: HostTaskEventPayload) => void
  clearCompleted: () => void
  removeTask: (id: string) => void
}

const pendingWrites = new Map<string, Promise<void>>()

function persist(change: HostTaskMutation) {
  const previous = pendingWrites.get(change.id) ?? Promise.resolve()
  const next = previous
    .catch(() => undefined)
    .then(() => tauriApi.hostTasks.upsert(change))
    .then((snapshot) => useTaskStore.getState().mergeSnapshot(snapshot))
    .catch(async (error: unknown) => {
      console.error('[host-tasks] failed to persist task', change.id, error)
      try {
        const snapshot = (await tauriApi.hostTasks.list()).find((item) => item.id === change.id)
        useTaskStore.setState((state) => ({
          tasks: snapshot
            ? [toTask(snapshot), ...state.tasks.filter((item) => item.id !== change.id)]
            : state.tasks.filter((item) => item.id !== change.id)
        }))
      } catch (refreshError) {
        console.error('[host-tasks] failed to refresh task', change.id, refreshError)
      }
    })
    .finally(() => {
      if (pendingWrites.get(change.id) === next) pendingWrites.delete(change.id)
    })
  pendingWrites.set(change.id, next)
}

function toTask(snapshot: HostTaskEventPayload): HostTask {
  return {
    id: snapshot.id,
    title: snapshot.title ?? '后台任务',
    source: snapshot.source ?? 'host',
    status: snapshot.status ?? 'queued',
    detail: snapshot.detail,
    progress: snapshot.progress,
    sequence: snapshot.sequence,
    stage: snapshot.stage,
    owner: snapshot.owner,
    kind: snapshot.kind,
    resultRefs: snapshot.resultRefs,
    error: snapshot.error,
    createdAt: snapshot.createdAt ?? Date.now(),
    updatedAt: snapshot.updatedAt ?? Date.now()
  }
}

function merge(state: TaskState, task: HostTaskEventPayload): HostTask[] {
  const existing = state.tasks.find((item) => item.id === task.id)
  if (
    existing?.sequence !== undefined &&
    task.sequence !== undefined &&
    task.sequence <= existing.sequence
  ) {
    return state.tasks
  }
  if (existing?.sequence !== undefined && task.sequence === undefined) return state.tasks
  const now = Date.now()
  const next: HostTask = {
    id: task.id,
    title: task.title ?? existing?.title ?? '后台任务',
    source: task.source ?? existing?.source ?? 'host',
    status: task.status ?? existing?.status ?? 'queued',
    detail: task.detail ?? existing?.detail,
    progress: task.progress ?? existing?.progress,
    sequence: task.sequence ?? existing?.sequence,
    stage: task.stage ?? existing?.stage,
    owner: task.owner ?? existing?.owner,
    kind: task.kind ?? existing?.kind,
    resultRefs: task.resultRefs ?? existing?.resultRefs,
    error: task.error ?? existing?.error,
    createdAt: task.createdAt ?? existing?.createdAt ?? now,
    updatedAt: task.updatedAt ?? now
  }
  return [next, ...state.tasks.filter((item) => item.id !== task.id)].slice(0, 200)
}

export const useTaskStore = create<TaskState>((set) => ({
  tasks: [],
  upsertTask: (task) => {
    set((state) => ({ tasks: merge(state, task) }))
    persist(task)
  },
  patchTask: (id, patch) => {
    set((state) => ({
      tasks: state.tasks.map((task) =>
        task.id === id ? { ...task, ...patch, updatedAt: Date.now() } : task
      )
    }))
    persist({ id, ...patch })
  },
  mergeSnapshot: (task) => set((state) => ({ tasks: merge(state, task) })),
  applyEvent: (task) => {
    if (task.sequence !== undefined) {
      set((state) => ({ tasks: merge(state, task) }))
    } else {
      set((state) => ({ tasks: merge(state, task) }))
      persist(task)
    }
  },
  clearCompleted: () => {
    set((state) => ({
      tasks: state.tasks.filter(
        (task) => !['completed', 'cancelled', 'failed'].includes(task.status)
      )
    }))
    void Promise.allSettled([...pendingWrites.values()]).then(() =>
      tauriApi.hostTasks.removeTerminal()
    )
  },
  removeTask: (id) => {
    set((state) => ({ tasks: state.tasks.filter((task) => task.id !== id) }))
    void (pendingWrites.get(id) ?? Promise.resolve()).then(() =>
      tauriApi.hostTasks.removeTerminal(id)
    )
  }
}))

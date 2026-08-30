import { create } from 'zustand'

export type HostTaskStatus = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled'

export interface HostTask {
  id: string
  title: string
  detail?: string
  source: 'host' | 'plugin' | 'marketplace' | 'update'
  status: HostTaskStatus
  progress?: number
  createdAt: number
  updatedAt: number
  error?: string
}

interface TaskState {
  tasks: HostTask[]
  upsertTask: (task: Omit<HostTask, 'createdAt' | 'updatedAt'> & { createdAt?: number }) => void
  patchTask: (id: string, patch: Partial<Omit<HostTask, 'id' | 'createdAt'>>) => void
  clearCompleted: () => void
}

export const useTaskStore = create<TaskState>((set) => ({
  tasks: [],
  upsertTask: (task) =>
    set((state) => {
      const now = Date.now()
      const existing = state.tasks.find((item) => item.id === task.id)
      const next: HostTask = {
        ...existing,
        ...task,
        createdAt: existing?.createdAt ?? task.createdAt ?? now,
        updatedAt: now
      }
      return {
        tasks: [next, ...state.tasks.filter((item) => item.id !== task.id)].slice(0, 200)
      }
    }),
  patchTask: (id, patch) =>
    set((state) => ({
      tasks: state.tasks.map((task) =>
        task.id === id ? { ...task, ...patch, updatedAt: Date.now() } : task
      )
    })),
  clearCompleted: () =>
    set((state) => ({
      tasks: state.tasks.filter(
        (task) => task.status !== 'completed' && task.status !== 'cancelled'
      )
    }))
}))

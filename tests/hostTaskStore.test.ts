import { beforeEach, describe, expect, it } from 'vitest'
import { useTaskStore } from '../tauri-frontend/src/store/task.store'

describe('host task snapshot projection', () => {
  beforeEach(() => useTaskStore.setState({ tasks: [] }))

  it('keeps a newer event when the startup snapshot arrives later', () => {
    const store = useTaskStore.getState()
    store.mergeSnapshot({
      id: 'download-1',
      title: '下载插件',
      source: 'marketplace',
      status: 'running',
      progress: 80,
      sequence: 5,
      createdAt: 10,
      updatedAt: 50
    })
    store.mergeSnapshot({
      id: 'download-1',
      title: '下载插件',
      source: 'marketplace',
      status: 'running',
      progress: 20,
      sequence: 3,
      createdAt: 10,
      updatedAt: 30
    })
    expect(useTaskStore.getState().tasks[0]).toMatchObject({
      progress: 80,
      sequence: 5,
      updatedAt: 50
    })
  })
})

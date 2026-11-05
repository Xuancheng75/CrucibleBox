import { beforeEach, describe, expect, it, vi } from 'vitest'

const api = vi.hoisted(() => ({
  list: vi.fn(),
  enable: vi.fn(),
  disable: vi.fn(),
  uninstall: vi.fn(),
  reorder: vi.fn(),
  registerImportPath: vi.fn(),
  installPreview: vi.fn(),
  installCommit: vi.fn(),
  installDiscard: vi.fn()
}))
vi.mock('../tauri-frontend/src/api/tauriApi', () => ({ tauriApi: { plugin: api } }))

import { usePluginStore } from '../tauri-frontend/src/store/plugin.store'
import { useTaskStore } from '../tauri-frontend/src/store/task.store'
import { useAppStore } from '../tauri-frontend/src/store/app.store'
import { PluginLifecycleStatus, type PluginMeta } from '../shared/types/plugin.types'

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

describe('plugin lifecycle and reviewed install queue', () => {
  beforeEach(async () => {
    usePluginStore.getState().clearInstallQueue()
    await usePluginStore.getState().discardInstall()
    usePluginStore.setState(usePluginStore.getInitialState(), true)
    useAppStore.setState(useAppStore.getInitialState(), true)
    useTaskStore.setState({ tasks: [] })
    vi.resetAllMocks()
    api.list.mockResolvedValue([])
    api.registerImportPath.mockResolvedValue(undefined)
    api.installDiscard.mockResolvedValue(undefined)
    api.installPreview.mockImplementation(async ({ path }: { path: string }) => ({
      success: true,
      installToken: path
    }))
    api.installCommit.mockResolvedValue({ success: true })
  })

  it('retries the actual failed enable operation with the retained record ID', async () => {
    api.enable
      .mockRejectedValueOnce(new Error('进程启动失败'))
      .mockResolvedValueOnce({ success: true })
    expect(await usePluginStore.getState().enablePlugin('retained-record')).toBe(false)
    expect(usePluginStore.getState().failedOperation).toEqual({
      kind: 'enable',
      id: 'retained-record'
    })
    expect(await usePluginStore.getState().retryFailedOperation()).toBe(true)
    expect(api.enable.mock.calls).toEqual([['retained-record'], ['retained-record']])
    expect(api.list).not.toHaveBeenCalled()
    expect(usePluginStore.getState().error).toBeNull()
  })

  it('closes a cached renderer after successful disable', async () => {
    useAppStore.getState().setActivePluginId('retained-record')
    useAppStore.getState().setCurrentPage('pluginView')
    api.disable.mockResolvedValue({ success: true })
    expect(await usePluginStore.getState().disablePlugin('retained-record')).toBe(true)
    expect(useAppStore.getState()).toMatchObject({
      openedPluginIds: [],
      activePluginId: null,
      currentPage: 'home'
    })
  })

  it('closes only successfully disabled batch views and retries only failed records', async () => {
    const plugins = ['first', 'failed'].map((id) => ({ id, enabled: true }) as PluginMeta)
    usePluginStore.setState({ plugins })
    api.list.mockImplementation(async () => usePluginStore.getState().plugins)
    for (const plugin of plugins) useAppStore.getState().setActivePluginId(plugin.id)
    useAppStore.getState().setCurrentPage('pluginView')
    api.disable
      .mockResolvedValueOnce({ success: true })
      .mockResolvedValueOnce({ success: false, error: 'worker did not stop' })
      .mockResolvedValueOnce({ success: true })

    const result = await usePluginStore.getState().batchDisablePlugins(['first', 'failed'])
    expect(result).toEqual({
      succeeded: ['first'],
      failures: [{ id: 'failed', error: 'worker did not stop' }]
    })
    expect(useAppStore.getState()).toMatchObject({
      openedPluginIds: ['failed'],
      activePluginId: 'failed',
      currentPage: 'pluginView'
    })
    expect(usePluginStore.getState().failedOperation).toEqual({
      kind: 'batch-disable',
      ids: ['failed']
    })
    expect(await usePluginStore.getState().retryFailedOperation()).toBe(true)
    expect(api.disable.mock.calls).toEqual([['first'], ['failed'], ['failed']])
    expect(useAppStore.getState().openedPluginIds).toEqual([])
  })

  it.each(['enable', 'disable', 'uninstall', 'batch-disable', 'reorder'] as const)(
    'keeps downloaded packages queued during %s and resumes after it finishes',
    async (operation) => {
      const pending = deferred<{ success: boolean }>()
      usePluginStore.setState({ plugins: [{ id: 'record', enabled: true } as PluginMeta] })
      api.enable.mockReturnValue(pending.promise)
      api.disable.mockReturnValue(pending.promise)
      api.uninstall.mockReturnValue(pending.promise)
      api.reorder.mockReturnValue(pending.promise)
      const store = usePluginStore.getState()
      const result =
        operation === 'enable'
          ? store.enablePlugin('record')
          : operation === 'disable'
            ? store.disablePlugin('record')
            : operation === 'uninstall'
              ? store.uninstallPlugin('record')
              : operation === 'batch-disable'
                ? store.batchDisablePlugins(['record'])
                : store.reorderPlugins(['record'])
      store.enqueueInstalls([{ source: 'zip', path: 'downloaded.zip' }])
      expect(api.installPreview).not.toHaveBeenCalled()
      expect(usePluginStore.getState().installQueue).toEqual([
        { source: 'zip', path: 'downloaded.zip' }
      ])
      pending.resolve({ success: true })
      await result
      await vi.waitFor(() =>
        expect(usePluginStore.getState().installPreview?.installToken).toBe('downloaded.zip')
      )
      expect(usePluginStore.getState().installQueue).toEqual([])
      expect(api.installPreview).toHaveBeenCalledTimes(1)
    }
  )

  it('does not infer running status from the enabled preference when refreshing', async () => {
    api.list.mockResolvedValue([
      { id: 'saved-choice', name: 'example-tool', enabled: true },
      { id: 'confirmed-start', name: 'other-tool', enabled: true }
    ] as PluginMeta[])
    usePluginStore.getState().setPluginStatus('confirmed-start', PluginLifecycleStatus.Active)
    await usePluginStore.getState().fetchPlugins()
    expect(usePluginStore.getState().activePlugins).toEqual({
      'confirmed-start': PluginLifecycleStatus.Active
    })
  })

  it('does not publish enabled or active state while startup is pending or fails', async () => {
    const startup = deferred<{ success: boolean; error: string }>()
    api.enable.mockReturnValueOnce(startup.promise)
    usePluginStore.setState({ plugins: [{ id: 'record-id', enabled: false } as PluginMeta] })
    const result = usePluginStore.getState().enablePlugin('record-id')
    expect(usePluginStore.getState().plugins[0].enabled).toBe(false)
    expect(usePluginStore.getState().activePlugins['record-id']).toBeUndefined()
    startup.resolve({ success: false, error: 'startup failed' })
    expect(await result).toBe(false)
    expect(usePluginStore.getState().plugins[0].enabled).toBe(false)
    expect(usePluginStore.getState().activePlugins['record-id']).toBeUndefined()
  })

  it('continues with the next preview after a failed commit and retains only failed inputs for retry', async () => {
    api.installCommit
      .mockResolvedValueOnce({ success: false, error: '校验失败' })
      .mockResolvedValueOnce({ success: true })
    usePluginStore.getState().enqueueInstalls([
      { source: 'zip', path: 'first.zip', marketplaceSourceId: 0 },
      { source: 'zip', path: 'second.zip', marketplaceSourceId: 0 }
    ])
    await vi.waitFor(() =>
      expect(usePluginStore.getState().installPreview?.installToken).toBe('first.zip')
    )
    expect(await usePluginStore.getState().commitInstall()).toBe(false)
    await vi.waitFor(() =>
      expect(usePluginStore.getState().installPreview?.installToken).toBe('second.zip')
    )
    expect(await usePluginStore.getState().commitInstall()).toBe(true)
    expect(usePluginStore.getState()).toMatchObject({
      batchTotal: 2,
      batchSucceeded: 1,
      installPreview: null,
      queueProcessing: false
    })
    expect(usePluginStore.getState().batchFailures).toEqual(['first.zip：校验失败'])
    expect(usePluginStore.getState().failedInstalls).toEqual([
      expect.objectContaining({ path: 'first.zip', marketplaceSourceId: 0 })
    ])
  })

  it('keeps inputs appended while an earlier preview is still being prepared', async () => {
    const first = deferred<{ success: boolean; installToken: string }>()
    api.installPreview.mockImplementationOnce(() => first.promise)
    usePluginStore.getState().enqueueInstalls([{ source: 'zip', path: 'first.zip' }])
    await vi.waitFor(() => expect(api.installPreview).toHaveBeenCalledTimes(1))
    usePluginStore.getState().enqueueInstalls([{ source: 'zip', path: 'second.zip' }])
    first.resolve({ success: true, installToken: 'first.zip' })
    await vi.waitFor(() =>
      expect(usePluginStore.getState().installPreview?.installToken).toBe('first.zip')
    )
    expect(usePluginStore.getState().installQueue.map((request) => request.path)).toEqual([
      'second.zip'
    ])
    await usePluginStore.getState().commitInstall()
    await vi.waitFor(() =>
      expect(usePluginStore.getState().installPreview?.installToken).toBe('second.zip')
    )
    await usePluginStore.getState().commitInstall()
    expect(usePluginStore.getState().batchSucceeded).toBe(2)
  })

  it('discards a late preview after the user has cancelled its queue', async () => {
    const pending = deferred<{ success: boolean; installToken: string }>()
    api.installPreview.mockImplementationOnce(() => pending.promise)
    usePluginStore
      .getState()
      .enqueueInstalls([{ source: 'zip', path: 'cancelled.zip', taskId: 'cancelled-install' }])
    await vi.waitFor(() => expect(api.installPreview).toHaveBeenCalledTimes(1))
    usePluginStore.getState().clearInstallQueue()
    pending.resolve({ success: true, installToken: 'late-token' })
    await vi.waitFor(() => expect(api.installDiscard).toHaveBeenCalledWith('late-token'))
    expect(usePluginStore.getState()).toMatchObject({
      installPreview: null,
      installQueue: [],
      batchTotal: 0,
      batchFailures: [],
      loading: false
    })
    expect(
      useTaskStore.getState().tasks.find((task) => task.id === 'cancelled-install')?.status
    ).toBe('cancelled')
  })

  it('does not replace a preview that is already awaiting confirmation', async () => {
    expect(await usePluginStore.getState().installPlugin('zip', 'first.zip')).toBe(true)
    expect(await usePluginStore.getState().installPlugin('zip', 'second.zip')).toBe(false)
    expect(usePluginStore.getState().installPreview?.installToken).toBe('first.zip')
    expect(api.installPreview).toHaveBeenCalledTimes(1)
    await usePluginStore.getState().discardInstall()
  })
})

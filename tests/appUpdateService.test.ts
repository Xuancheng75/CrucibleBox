import { EventEmitter } from 'node:events'
import type { AppUpdater, UpdateCheckResult, UpdateInfo } from 'electron-updater'
import { AppUpdateService } from '../electron/AppUpdateService'
import type { AppUpdateState } from '../shared/types/ipc.types'

const updateInfo = {
  version: '2.0.0',
  files: [],
  path: 'CrucibleBox.exe',
  sha512: 'digest',
  releaseDate: '2026-08-11T00:00:00.000Z'
} satisfies UpdateInfo

class FakeUpdater extends EventEmitter {
  autoDownload = true
  autoInstallOnAppQuit = true
  disableWebInstaller = false
  fullChangelog = false
  allowPrerelease = false
  allowDowngrade = false
  channel: string | null = null
  checkResult: UpdateCheckResult | null = {
    isUpdateAvailable: true,
    updateInfo,
    versionInfo: updateInfo
  }
  checkCalls = 0
  downloadCalls = 0
  installCalls = 0
  checkBarrier: Promise<void> | null = null

  async checkForUpdates(): Promise<UpdateCheckResult | null> {
    this.checkCalls += 1
    if (this.checkBarrier) await this.checkBarrier
    return this.checkResult
  }

  async downloadUpdate(): Promise<string[]> {
    this.downloadCalls += 1
    this.emit('update-downloaded', updateInfo)
    return ['downloaded']
  }

  quitAndInstall(): void {
    this.installCalls += 1
  }
}

function createService(
  options: {
    packaged?: boolean
    configured?: boolean
    initialChannel?: string
    beforeInstall?: () => Promise<void>
    persistChannel?: (channel: 'stable' | 'beta') => void
  } = {}
) {
  const updater = new FakeUpdater()
  const broadcasts: AppUpdateState[] = []
  const persisted: string[] = []
  const service = new AppUpdateService({
    currentVersion: '1.5.22',
    packaged: options.packaged ?? true,
    configured: options.configured ?? true,
    updater: updater as unknown as AppUpdater,
    initialChannel: options.initialChannel,
    persistChannel: options.persistChannel ?? ((channel) => persisted.push(channel)),
    broadcast: (state) => broadcasts.push(state),
    beforeInstall: options.beforeInstall ?? (async () => {}),
    autoCheckDelayMs: null
  })
  service.start()
  return { broadcasts, persisted, service, updater }
}

describe('AppUpdateService', () => {
  it('keeps development builds disabled without touching the updater', async () => {
    const { service, updater } = createService({ packaged: false })

    expect(service.getState()).toMatchObject({ phase: 'disabled', channel: 'stable' })
    expect(updater.channel).toBeNull()
    await expect(service.check()).rejects.toThrow('disabled')
  })

  it('keeps packaged builds fully usable when online updates are not configured', async () => {
    const { service, updater } = createService({ configured: false })

    expect(service.getState()).toMatchObject({
      phase: 'disabled',
      message: 'Online updates are not configured; all offline features remain available'
    })
    expect(updater.channel).toBeNull()
    expect(service.setChannel('beta')).toMatchObject({ phase: 'disabled', channel: 'stable' })
    await expect(service.check()).rejects.toThrow('not configured')
    expect(updater.checkCalls).toBe(0)
  })

  it('configures explicit download and stable channel defaults', () => {
    const { service, updater } = createService()

    expect(updater).toMatchObject({
      autoDownload: false,
      autoInstallOnAppQuit: false,
      disableWebInstaller: true,
      fullChangelog: true,
      allowPrerelease: false,
      allowDowngrade: false,
      channel: 'latest'
    })
    service.dispose()
  })

  it('allows downgrade only for an explicit beta to stable transition', () => {
    const { persisted, service, updater } = createService({ initialChannel: 'beta' })

    expect(updater.channel).toBe('beta')
    expect(updater.allowPrerelease).toBe(true)
    const state = service.setChannel('stable')

    expect(state.rollbackEligible).toBe(true)
    expect(updater.channel).toBe('latest')
    expect(updater.allowDowngrade).toBe(true)
    expect(persisted).toEqual(['stable'])

    updater.emit('update-not-available', updateInfo)
    expect(service.getState().rollbackEligible).toBe(false)
    expect(updater.allowDowngrade).toBe(false)
  })

  it('checks, downloads and installs only through explicit actions', async () => {
    let prepared = false
    const { service, updater } = createService({
      beforeInstall: async () => {
        prepared = true
      }
    })

    await expect(service.download()).rejects.toThrow('No update')
    expect((await service.check()).phase).toBe('available')
    expect((await service.download()).phase).toBe('downloaded')
    await service.install()

    expect(prepared).toBe(true)
    expect(updater).toMatchObject({ checkCalls: 1, downloadCalls: 1, installCalls: 1 })
  })

  it('restores the channel and updater policy when persistence fails', () => {
    const { service, updater } = createService({
      persistChannel: () => {
        throw new Error('database write failed')
      }
    })

    expect(() => service.setChannel('beta')).toThrow('database write failed')
    expect(service.getState()).toMatchObject({ channel: 'stable', rollbackEligible: false })
    expect(updater).toMatchObject({
      channel: 'latest',
      allowPrerelease: false,
      allowDowngrade: false
    })
  })

  it('rejects concurrent update operations', async () => {
    let releaseCheck: (() => void) | undefined
    const { service, updater } = createService()
    updater.checkBarrier = new Promise<void>((resolve) => {
      releaseCheck = resolve
    })

    const checking = service.check()
    await Promise.resolve()
    await expect(service.check()).rejects.toThrow('already in progress')
    expect(() => service.setChannel('beta')).toThrow('during an update operation')
    releaseCheck?.()
    await checking
  })

  it('publishes bounded errors without retaining a busy lock', async () => {
    const { broadcasts, service, updater } = createService()
    updater.checkForUpdates = async () => {
      throw new Error('x'.repeat(800))
    }

    await expect(service.check()).rejects.toThrow()
    expect(service.getState()).toMatchObject({ phase: 'error' })
    expect(service.getState().message).toHaveLength(500)
    expect(broadcasts.at(-1)?.phase).toBe('error')
    service.setChannel('beta')
    expect(service.getState().phase).toBe('idle')
  })
})

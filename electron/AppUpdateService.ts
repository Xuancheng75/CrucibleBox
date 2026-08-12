import type { AppUpdater, ProgressInfo, UpdateCheckResult, UpdateInfo } from 'electron-updater'
import type { AppUpdateChannel, AppUpdateState } from '@shared/types/ipc.types'

export interface AppUpdateLogger {
  info(event: string, data?: unknown): void
  warn(event: string, data?: unknown): void
  error(event: string, data?: unknown): void
}

export interface AppUpdateServiceOptions {
  currentVersion: string
  packaged: boolean
  configured: boolean
  updater: AppUpdater
  initialChannel?: string | null
  persistChannel: (channel: AppUpdateChannel) => void
  broadcast: (state: AppUpdateState) => void
  beforeInstall: () => Promise<void>
  logger?: AppUpdateLogger
  autoCheckDelayMs?: number | null
}

function parseChannel(value: unknown): AppUpdateChannel {
  return value === 'beta' ? 'beta' : 'stable'
}

function boundedMessage(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error)
  return message.slice(0, 500)
}

export class AppUpdateService {
  private readonly options: AppUpdateServiceOptions
  private readonly updater: AppUpdater
  private state: AppUpdateState
  private operation: Promise<unknown> | null = null
  private autoCheckTimer: ReturnType<typeof setTimeout> | null = null
  private started = false

  constructor(options: AppUpdateServiceOptions) {
    this.options = options
    this.updater = options.updater
    this.state = {
      phase: this.isEnabled() ? 'idle' : 'disabled',
      channel: parseChannel(options.initialChannel),
      currentVersion: options.currentVersion,
      availableVersion: null,
      progressPercent: null,
      rollbackEligible: false,
      message: this.disabledMessage()
    }
  }

  start(): void {
    if (this.started) return
    this.started = true
    if (!this.isEnabled()) {
      this.publish()
      return
    }

    this.updater.autoDownload = false
    this.updater.autoInstallOnAppQuit = false
    this.updater.disableWebInstaller = true
    this.updater.fullChangelog = true
    this.applyChannel(false)
    this.updater.on('checking-for-update', this.onChecking)
    this.updater.on('update-available', this.onAvailable)
    this.updater.on('update-not-available', this.onNotAvailable)
    this.updater.on('download-progress', this.onProgress)
    this.updater.on('update-downloaded', this.onDownloaded)
    this.updater.on('error', this.onError)
    this.publish()

    const delay = this.options.autoCheckDelayMs
    if (delay !== null && delay !== undefined) {
      this.autoCheckTimer = setTimeout(
        () => {
          this.autoCheckTimer = null
          void this.check().catch(() => {})
        },
        Math.max(0, delay)
      )
      this.autoCheckTimer.unref()
    }
  }

  dispose(): void {
    if (this.autoCheckTimer) clearTimeout(this.autoCheckTimer)
    this.autoCheckTimer = null
    if (!this.started || !this.isEnabled()) return
    this.updater.off('checking-for-update', this.onChecking)
    this.updater.off('update-available', this.onAvailable)
    this.updater.off('update-not-available', this.onNotAvailable)
    this.updater.off('download-progress', this.onProgress)
    this.updater.off('update-downloaded', this.onDownloaded)
    this.updater.off('error', this.onError)
  }

  getState(): AppUpdateState {
    return { ...this.state }
  }

  setChannel(channel: unknown): AppUpdateState {
    if (!this.isEnabled()) return this.getState()
    if (channel !== 'stable' && channel !== 'beta') throw new Error('Unsupported update channel')
    if (this.operation) throw new Error('Cannot change update channel during an update operation')
    const previousState = this.getState()
    const previous = this.state.channel
    this.state = {
      ...this.state,
      phase: 'idle',
      channel,
      availableVersion: null,
      progressPercent: null,
      rollbackEligible: previous === 'beta' && channel === 'stable',
      message: null
    }
    try {
      this.applyChannel(this.state.rollbackEligible)
      this.options.persistChannel(channel)
    } catch (error) {
      this.state = previousState
      this.applyChannel(previousState.rollbackEligible)
      throw error
    }
    this.options.logger?.info('update-channel-changed', {
      channel,
      rollbackEligible: this.state.rollbackEligible
    })
    this.publish()
    return this.getState()
  }

  check(): Promise<AppUpdateState> {
    return this.runExclusive(async () => {
      this.assertEnabled()
      this.updateState({ phase: 'checking', message: null, progressPercent: null })
      const result = await this.updater.checkForUpdates()
      this.applyCheckResult(result)
      return this.getState()
    })
  }

  download(): Promise<AppUpdateState> {
    return this.runExclusive(async () => {
      this.assertEnabled()
      if (this.state.phase !== 'available') throw new Error('No update is available to download')
      this.updateState({ phase: 'downloading', progressPercent: 0, message: null })
      await this.updater.downloadUpdate()
      if (this.getState().phase === 'downloading') {
        this.updateState({ phase: 'downloaded', progressPercent: 100 })
      }
      return this.getState()
    })
  }

  install(): Promise<AppUpdateState> {
    return this.runExclusive(async () => {
      this.assertEnabled()
      if (this.state.phase !== 'downloaded') throw new Error('No downloaded update is ready')
      await this.options.beforeInstall()
      this.options.logger?.info('update-installing', {
        version: this.state.availableVersion,
        channel: this.state.channel
      })
      this.updater.quitAndInstall(false, true)
      return this.getState()
    })
  }

  private runExclusive<T>(operation: () => Promise<T>): Promise<T> {
    if (this.operation)
      return Promise.reject(new Error('An update operation is already in progress'))
    const pending = operation()
      .catch((error) => {
        this.onError(error instanceof Error ? error : new Error(String(error)))
        throw error
      })
      .finally(() => {
        if (this.operation === pending) this.operation = null
      })
    this.operation = pending
    return pending
  }

  private assertEnabled(): void {
    if (!this.options.packaged) throw new Error('Updates are disabled in development builds')
    if (!this.options.configured)
      throw new Error('Online updates are not configured for this build')
  }

  private isEnabled(): boolean {
    return this.options.packaged && this.options.configured
  }

  private disabledMessage(): string | null {
    if (!this.options.packaged) return 'Updates are available only in packaged builds'
    if (!this.options.configured)
      return 'Online updates are not configured; all offline features remain available'
    return null
  }

  private applyChannel(allowDowngrade: boolean): void {
    this.updater.channel = this.state.channel === 'stable' ? 'latest' : 'beta'
    this.updater.allowPrerelease = this.state.channel === 'beta'
    this.updater.allowDowngrade = allowDowngrade
  }

  private applyCheckResult(result: UpdateCheckResult | null): void {
    if (!result) {
      this.consumeRollbackAllowance()
      this.updateState({
        phase: 'not-available',
        rollbackEligible: false,
        message: 'Update service is unavailable'
      })
      return
    }
    if (result.isUpdateAvailable) {
      this.onAvailable(result.updateInfo)
    } else {
      this.onNotAvailable(result.updateInfo)
    }
  }

  private readonly onChecking = (): void => {
    this.updateState({ phase: 'checking', message: null })
  }

  private readonly onAvailable = (info: UpdateInfo): void => {
    this.consumeRollbackAllowance()
    this.updateState({
      phase: 'available',
      availableVersion: info.version,
      progressPercent: null,
      rollbackEligible: false,
      message: null
    })
    this.options.logger?.info('update-available', {
      version: info.version,
      channel: this.state.channel
    })
  }

  private readonly onNotAvailable = (_info: UpdateInfo): void => {
    this.consumeRollbackAllowance()
    this.updateState({
      phase: 'not-available',
      availableVersion: null,
      progressPercent: null,
      rollbackEligible: false,
      message: null
    })
  }

  private readonly onProgress = (progress: ProgressInfo): void => {
    this.updateState({
      phase: 'downloading',
      progressPercent: Math.max(0, Math.min(100, progress.percent))
    })
  }

  private readonly onDownloaded = (info: UpdateInfo): void => {
    this.updateState({
      phase: 'downloaded',
      availableVersion: info.version,
      progressPercent: 100,
      message: null
    })
    this.options.logger?.info('update-downloaded', { version: info.version })
  }

  private readonly onError = (error: Error): void => {
    this.consumeRollbackAllowance()
    const message = boundedMessage(error)
    this.updateState({ phase: 'error', progressPercent: null, rollbackEligible: false, message })
    this.options.logger?.error('update-error', { message })
  }

  private updateState(changes: Partial<AppUpdateState>): void {
    this.state = { ...this.state, ...changes }
    this.publish()
  }

  private consumeRollbackAllowance(): void {
    if (!this.state.rollbackEligible) return
    this.updater.allowDowngrade = false
  }

  private publish(): void {
    this.options.broadcast(this.getState())
  }
}

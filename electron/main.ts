// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { app, BrowserWindow, dialog, shell, globalShortcut, protocol, session } from 'electron'
import electronUpdater from 'electron-updater'
import { existsSync } from 'node:fs'
import { join } from 'path'
import { initDatabase, closeDatabase } from '../database/index'
import { PluginManager } from '../plugin-system/PluginManager'
import { readPluginManifest } from '../plugin-system/PluginManifestPolicy'
import { registerAllIpc } from './ipc/index'
import { registerPluginEventBridge } from './pluginEvents'
import { createAppMenu } from './menu'
import {
  PLUGIN_RENDERER_SCHEME,
  PluginRendererSessionRegistry
} from '../plugin-system/PluginRendererSessionRegistry'
import { registerPluginRendererProtocol } from '../plugin-system/PluginRendererProtocol'
import {
  PLUGIN_RENDERER_OWNER_PROOF_HEADER,
  PluginRendererRequestOwnerProof
} from '../plugin-system/PluginRendererRequestOwnerProof'
import { runPluginRendererSmoke } from './pluginRendererSmoke'
import { StartupMetrics } from './StartupMetrics'
import { DiagnosticLog } from './DiagnosticLog'
import { AppUpdateService } from './AppUpdateService'
import { SettingsRepository } from '../database/repositories/settings.repository'
import { IpcChannel } from '../shared/types/ipc.types'
import {
  isAllowedExternalUrl,
  isAllowedHostNavigation,
  isChromiumPermissionAllowed
} from './windowSecurityPolicy'
import {
  HOST_RENDERER_SCHEME,
  HOST_RENDERER_URL,
  registerHostRendererProtocol
} from './HostRendererProtocol'

let mainWindow: BrowserWindow | null = null
let pluginManager: PluginManager | null = null
let quitPrepared = false
let quitRequested = false
let shutdownPromise: Promise<void> | null = null
let appUpdateService: AppUpdateService | null = null
const isSmokeTest = process.env['OPENBOX_SMOKE_TEST'] === '1'
const pluginRendererSessions = new PluginRendererSessionRegistry()
const pluginRendererOwnerProof = new PluginRendererRequestOwnerProof()
let pluginRendererCleanupTimer: ReturnType<typeof setInterval> | null = null
let smokePluginId: string | null = null
let smokePluginBackendExpected = true
let startupInteractiveReported = false
const startupMetrics = new StartupMetrics()
let diagnosticLog: DiagnosticLog | null = null
let lastRendererRecoveryAt = 0
const packagedRendererPath = join(__dirname, '../renderer/index.html')
const packagedRendererUrl = HOST_RENDERER_URL

const gotSingleInstanceLock = app.requestSingleInstanceLock()
const { autoUpdater } = electronUpdater

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 900,
    minHeight: 600,
    show: !isSmokeTest,
    title: 'CrucibleBox',
    webPreferences: {
      preload: join(__dirname, '../preload/preload.js'),
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      nodeIntegrationInSubFrames: false,
      webSecurity: true,
      allowRunningInsecureContent: false
    }
  })
  const ownerWebContentsId = mainWindow.webContents.id
  const windowInstance = mainWindow
  startupMetrics.mark('window.created')
  mainWindow.webContents.once('did-finish-load', () => {
    if (startupInteractiveReported) return
    startupInteractiveReported = true
    logStartupMetrics('renderer.ready')
  })
  mainWindow.webContents.on('render-process-gone', (_event, details) => {
    diagnosticLog?.write('error', 'renderer-process-gone', {
      reason: details.reason,
      exitCode: details.exitCode
    })
    const now = Date.now()
    if (!quitPrepared && details.reason !== 'clean-exit' && now - lastRendererRecoveryAt > 60_000) {
      lastRendererRecoveryAt = now
      setTimeout(() => {
        if (!windowInstance.isDestroyed()) windowInstance.reload()
      }, 1_000)
    }
  })
  if (isSmokeTest) {
    mainWindow.webContents.on('console-message', (details) => {
      console.error(`[smoke:console] ${details.sourceId}:${details.lineNumber} ${details.message}`)
    })
    mainWindow.webContents.on('did-fail-load', (_event, code, description, url, isMainFrame) => {
      console.error(
        `[smoke:load] code=${code} main=${isMainFrame} url=${url} description=${description}`
      )
    })
  }
  mainWindow.once('closed', () => {
    pluginRendererSessions.disposeOwner(ownerWebContentsId)
    if (mainWindow === windowInstance) mainWindow = null
  })

  mainWindow.webContents.setWindowOpenHandler((details) => {
    if (isAllowedExternalUrl(details.url)) {
      void shell.openExternal(details.url).catch((error) => {
        diagnosticLog?.write('error', 'external-link-open-failed', {
          message: error instanceof Error ? error.message : String(error)
        })
      })
    }
    return { action: 'deny' }
  })

  mainWindow.webContents.on('will-navigate', (event, url) => {
    if (isAllowedHostNavigation(url, process.env['ELECTRON_RENDERER_URL'], packagedRendererUrl)) {
      return
    }
    event.preventDefault()
    if (isAllowedExternalUrl(url)) {
      void shell.openExternal(url).catch((error) => {
        diagnosticLog?.write('error', 'external-navigation-open-failed', {
          message: error instanceof Error ? error.message : String(error)
        })
      })
    }
  })

  if (process.env['ELECTRON_RENDERER_URL']) {
    mainWindow.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    mainWindow.loadURL(packagedRendererUrl)
  }

  if (isSmokeTest) {
    const timeout = setTimeout(() => {
      console.error('[smoke] renderer did not finish loading within 20 seconds')
      process.exitCode = 1
      app.quit()
    }, 20000)

    mainWindow.webContents.once('did-finish-load', async () => {
      clearTimeout(timeout)
      try {
        const state = await mainWindow?.webContents.executeJavaScript(`(async () => ({
          hasBridge: typeof window.electronAPI === 'object',
          hasNodeProcess: typeof window.process !== 'undefined',
          hasNodeRequire: typeof window.require !== 'undefined',
          geolocationPermission: await navigator.permissions
            .query({ name: 'geolocation' })
            .then((permission) => permission.state)
            .catch(() => 'denied')
        }))()`)
        if (
          !state?.hasBridge ||
          state.hasNodeProcess ||
          state.hasNodeRequire ||
          state.geolocationPermission !== 'denied'
        ) {
          throw new Error(`unexpected renderer security state: ${JSON.stringify(state)}`)
        }
        if (process.env['OPENBOX_SMOKE_PLUGIN_PATH']) {
          if (!smokePluginId) throw new Error('smoke plugin was not installed')
          const pluginState = await runPluginRendererSmoke(
            mainWindow!.webContents,
            smokePluginId,
            smokePluginBackendExpected
          )
          if (
            (smokePluginBackendExpected
              ? pluginState.backendResponsive !== true
              : pluginState.backendResponsive !== null) ||
            !pluginState.isolated ||
            pluginState.rendererApiVersion !== 2 ||
            pluginState.layoutHeight < 100
          ) {
            throw new Error(`unexpected plugin renderer state: ${JSON.stringify(pluginState)}`)
          }
          console.log(
            smokePluginBackendExpected
              ? '[smoke] plugin backend utility process responded over RPC v2'
              : '[smoke] renderer-only plugin skipped backend process'
          )
          console.log('[smoke] plugin renderer loaded in an isolated cross-origin frame')
        }
        console.log('[smoke] renderer loaded with sandboxed preload bridge')
      } catch (error) {
        console.error('[smoke] renderer security probe failed:', error)
        process.exitCode = 1
      } finally {
        app.quit()
      }
    })
    mainWindow.webContents.once('did-fail-load', (_event, code, description) => {
      clearTimeout(timeout)
      console.error(`[smoke] renderer failed to load (${code}): ${description}`)
      process.exitCode = 1
      app.quit()
    })
  }

  // Safety net: force show window after 10s
  if (!isSmokeTest) {
    setTimeout(() => {
      if (mainWindow && !mainWindow.isVisible()) {
        console.warn('Force showing window after timeout')
        mainWindow.show()
      }
    }, 10000)
  }
}

function registerShortcuts(): void {
  if (app.isPackaged) return
  globalShortcut.register('F12', () => {
    const win = BrowserWindow.getFocusedWindow()
    win?.webContents.toggleDevTools()
  })
}

protocol.registerSchemesAsPrivileged([
  {
    scheme: HOST_RENDERER_SCHEME,
    privileges: { standard: true, secure: true, supportFetchAPI: true, codeCache: true }
  },
  {
    scheme: 'plugin',
    privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true }
  },
  {
    scheme: PLUGIN_RENDERER_SCHEME,
    privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true }
  }
])

function registerPluginRendererIsolation(): void {
  session.defaultSession.webRequest.onBeforeSendHeaders(
    { urls: [`${PLUGIN_RENDERER_SCHEME}://*/*`] },
    (details, callback) => {
      const requestHeaders = { ...details.requestHeaders }
      if (details.webContentsId && details.webContentsId > 0) {
        requestHeaders[PLUGIN_RENDERER_OWNER_PROOF_HEADER] = pluginRendererOwnerProof.create(
          details.webContentsId
        )
      }
      callback({ requestHeaders })
    }
  )
  registerPluginRendererProtocol({
    protocol,
    registry: pluginRendererSessions,
    getOwnerWebContentsId: (request) =>
      pluginRendererOwnerProof.verify(request.headers.get(PLUGIN_RENDERER_OWNER_PROOF_HEADER))
  })
  pluginRendererCleanupTimer = setInterval(() => pluginRendererSessions.cleanupExpired(), 60_000)
  pluginRendererCleanupTimer.unref()
}

function registerSessionPermissionPolicy(): void {
  session.defaultSession.setPermissionCheckHandler(() => isChromiumPermissionAllowed())
  session.defaultSession.setPermissionRequestHandler((_webContents, _permission, callback) => {
    callback(isChromiumPermissionAllowed())
  })
}

if (!gotSingleInstanceLock) {
  app.quit()
} else {
  mainLoop()
}

app.on('second-instance', () => {
  if (mainWindow) {
    if (mainWindow.isMinimized()) {
      mainWindow.restore()
    }
    mainWindow.focus()
  }
})

function mainLoop(): void {
  createAppMenu()

  app.whenReady().then(async () => {
    startupMetrics.mark('app.ready')
    diagnosticLog = new DiagnosticLog(join(app.getPath('userData'), 'logs'))
    diagnosticLog.startSession(app.getVersion())
    registerHostRendererProtocol(protocol, join(packagedRendererPath, '..'))
    registerSessionPermissionPolicy()
    registerPluginRendererIsolation()
    try {
      await initDatabase()
      console.log('Database initialized')
      startupMetrics.mark('database.ready')
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      console.error('Database init failed:', err)
      startupMetrics.mark('database.failed')
      diagnosticLog.write('fatal', 'database-initialization-failed', { message })
      dialog.showErrorBox(
        'OpenBox 数据库启动失败',
        `数据库未被修改，应用将安全退出。\n\n${message}`
      )
      process.exitCode = 1
      app.quit()
      return
    }

    try {
      pluginManager = new PluginManager()
      appUpdateService = new AppUpdateService({
        currentVersion: app.getVersion(),
        packaged: app.isPackaged,
        configured: app.isPackaged && existsSync(join(process.resourcesPath, 'app-update.yml')),
        updater: autoUpdater,
        initialChannel: SettingsRepository.get('updateChannel'),
        persistChannel: (channel) => SettingsRepository.set('updateChannel', channel),
        broadcast: (state) => {
          for (const window of BrowserWindow.getAllWindows()) {
            if (!window.isDestroyed()) window.webContents.send(IpcChannel.AppUpdateChanged, state)
          }
        },
        beforeInstall: async () => {
          quitRequested = true
          await prepareShutdown()
          quitPrepared = true
        },
        logger: {
          info: (event, data) => diagnosticLog?.write('info', event, data),
          warn: (event, data) => diagnosticLog?.write('warn', event, data),
          error: (event, data) => diagnosticLog?.write('error', event, data)
        },
        autoCheckDelayMs: isSmokeTest ? null : 30_000
      })
      registerAllIpc(
        pluginManager,
        {
          registry: pluginRendererSessions,
          runtimePath: app.isPackaged
            ? join(process.resourcesPath, 'app.asar.unpacked/out/plugin-frame/runtime.js')
            : join(__dirname, '../plugin-frame/runtime.js')
        },
        appUpdateService
      )
      registerPluginEventBridge(pluginManager)
      const smokePluginPath = process.env['OPENBOX_SMOKE_PLUGIN_PATH']
      if (isSmokeTest && smokePluginPath) {
        smokePluginBackendExpected = readPluginManifest(smokePluginPath).backend !== false
        const smokePlugin = await pluginManager.installFromDirectory(smokePluginPath)
        smokePluginId = smokePlugin.id
        await pluginManager.activatePlugin(smokePlugin.id)
      }
      const smokeUniEnvPath = process.env['OPENBOX_SMOKE_UNIENV_PATH']
      if (isSmokeTest && smokeUniEnvPath) {
        const smokeUniEnv = await pluginManager.installFromDirectory(smokeUniEnvPath)
        await pluginManager.activatePlugin(smokeUniEnv.id)
        const tools = await pluginManager.sendMessage(smokeUniEnv.id, { type: 'listTools' })
        if (!Array.isArray(tools) || tools.length !== 5) {
          throw new Error(`unexpected UniEnv trusted service response: ${JSON.stringify(tools)}`)
        }
        console.log('[smoke] UniEnv trusted host service responded through the pinned proxy')
      }
      console.log('Plugin manager ready')
      startupMetrics.mark('plugin-manager.ready')
    } catch (err) {
      console.error('Plugin manager init failed:', err)
      startupMetrics.mark('plugin-manager.failed')
    }

    createWindow()
    appUpdateService?.start()
    registerShortcuts()

    // Plugin startup is deliberately background work. A slow or broken plugin
    // must not delay the first usable application window.
    void pluginManager
      ?.activateAllEnabled()
      .then(() => logStartupMetrics('plugins.restored'))
      .catch((err) => {
        console.error('Plugin activation failed:', err)
      })

    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) {
        createWindow()
      }
    })
  })

  app.on('before-quit', (e) => {
    globalShortcut.unregisterAll()
    if (pluginRendererCleanupTimer) {
      clearInterval(pluginRendererCleanupTimer)
      pluginRendererCleanupTimer = null
    }
    if (quitPrepared) {
      // Second quit: allow the app to actually exit.
      return
    }
    e.preventDefault()
    if (quitRequested) return
    quitRequested = true
    void prepareShutdown().finally(() => {
      quitPrepared = true
      app.quit()
    })
  })

  app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
      app.quit()
    }
  })
}

function prepareShutdown(): Promise<void> {
  if (shutdownPromise) return shutdownPromise
  appUpdateService?.dispose()
  // Wait for all plugin runtimes to stop before closing the database,
  // otherwise a plugin process exiting during shutdown can log to an
  // already-closed database and crash the main process.
  shutdownPromise = (pluginManager ? pluginManager.deactivateAll() : Promise.resolve())
    .catch((error) => {
      diagnosticLog?.write('error', 'plugin-shutdown-failed', {
        message: error instanceof Error ? error.message : String(error)
      })
    })
    .then(() => {
      try {
        closeDatabase()
      } finally {
        diagnosticLog?.finishSession()
      }
    })
  return shutdownPromise
}

function logStartupMetrics(phase: string): void {
  const report = startupMetrics.report(phase, app.getAppMetrics())
  console.log(`[metrics] ${JSON.stringify(report)}`)
  diagnosticLog?.write('info', 'startup-performance', report)
}

process.on('uncaughtExceptionMonitor', (error) => {
  diagnosticLog?.write('fatal', 'uncaught-exception', {
    name: error.name,
    message: error.message,
    stack: error.stack
  })
})

process.on('unhandledRejection', (reason) => {
  const error = reason instanceof Error ? reason : new Error(String(reason))
  diagnosticLog?.write('error', 'unhandled-rejection', {
    message: error.message,
    stack: error.stack
  })
  setImmediate(() => {
    throw error
  })
})

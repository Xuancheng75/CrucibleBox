import type { PluginMeta, PluginConfig, PluginLogEntry, PluginLogFilter } from './plugin.types'

export enum IpcChannel {
  // Plugin CRUD
  PluginInstall = 'plugin:install',
  PluginUninstall = 'plugin:uninstall',
  PluginList = 'plugin:list',
  PluginGet = 'plugin:get',
  PluginEnable = 'plugin:enable',
  PluginDisable = 'plugin:disable',
  PluginReorder = 'plugin:reorder',
  PluginUpdateConfig = 'plugin:update-config',
  PluginSendMessage = 'plugin:send-message',
  PluginGetLogs = 'plugin:get-logs',
  PluginClearLogs = 'plugin:clear-logs',
  PluginRegisterImportPath = 'plugin:register-import-path',
  PluginCreateRendererSession = 'plugin:create-renderer-session',
  PluginDisposeRendererSession = 'plugin:dispose-renderer-session',

  // Plugin lifecycle events (main -> renderer)
  PluginMessage = 'plugin:message',
  PluginLog = 'plugin:log',
  PluginStatusChange = 'plugin:status-change',

  // Settings
  SettingsGet = 'settings:get',
  SettingsSet = 'settings:set',
  SettingsGetAll = 'settings:get-all',

  // Theme
  ThemeGet = 'theme:get',
  ThemeSet = 'theme:set',
  ThemeList = 'theme:list',
  ThemeChanged = 'theme:changed',

  // Dialog
  DialogOpenFile = 'dialog:open-file',
  DialogOpenDirectory = 'dialog:open-directory',

  // App
  AppGetVersion = 'app:get-version',
  AppGetPlatform = 'app:get-platform',
  AppUpdateGetState = 'app:update:get-state',
  AppUpdateSetChannel = 'app:update:set-channel',
  AppUpdateCheck = 'app:update:check',
  AppUpdateDownload = 'app:update:download',
  AppUpdateInstall = 'app:update:install',
  AppUpdateChanged = 'app:update:changed'
}

export type AppUpdateChannel = 'stable' | 'beta'

export type AppUpdatePhase =
  | 'disabled'
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'downloaded'
  | 'not-available'
  | 'error'

export interface AppUpdateState {
  phase: AppUpdatePhase
  channel: AppUpdateChannel
  currentVersion: string
  availableVersion: string | null
  progressPercent: number | null
  rollbackEligible: boolean
  message: string | null
}

export interface IpcResponse<T = unknown> {
  success: boolean
  data?: T
  error?: string
}

export interface InstallPluginRequest {
  source: 'zip' | 'directory'
  path: string
}

export interface InstallPluginResponse {
  plugin: PluginMeta
}

export interface UninstallPluginRequest {
  id: string
}

export interface EnablePluginRequest {
  id: string
}

export interface DisablePluginRequest {
  id: string
}

/** Full permutation of every installed plugin id, in the desired display order. */
export interface ReorderPluginsRequest {
  orderedIds: string[]
}

export interface UpdatePluginConfigRequest {
  id: string
  config: PluginConfig
}

export interface PluginSendMessageRequest {
  id: string
  message: unknown
}

export interface PluginMessageEvent {
  pluginId: string
  message: unknown
}

export interface PluginStatusChangeEvent {
  pluginId: string
  status: string
}

export interface PluginLogEvent {
  pluginId: string
  level: string
  message: string
}

export interface PluginRendererSessionDescriptor {
  token: string
  handshakeToken: string
  origin: string
  indexUrl: string
  rendererApiVersion: 1 | 2
  expiresAt: number
}

export type PluginGetLogsRequest = PluginLogFilter

export type PluginGetLogsResponse = PluginLogEntry[]

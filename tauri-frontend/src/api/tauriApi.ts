// Tauri IPC 层（1.9.3）
// 对等 Electron 版 src/api/plugin.api.ts + theme.api.ts + electron/preload.ts 的 window.electronAPI。
// 所有后端命令经 @tauri-apps/api/core 的 invoke() 调用；事件经 @tauri-apps/api/event 的 listen() 订阅。
// 命令签名（src-tauri/src/commands.rs，参数 camelCase）：
//   settings_get(key) / settings_set(key,value) / settings_get_all()
//   app_get_version() / app_get_platform()
//   plugin_list() / plugin_get(id) / plugin_enable(id) / plugin_disable(id)
//   plugin_reorder(orderedIds) / plugin_update_config(id,config)
//   plugin_get_logs(pluginId?,level?,limit?) / plugin_clear_logs(pluginId?) / plugin_uninstall(id)
//   create_renderer_session(id) / dispose_renderer_session(token) / plugin_send_message(id,message)
//   plugin_install_preview(source) / plugin_install_commit(installToken) / plugin_install_discard(installToken)
// 事件：plugin:log / plugin:message / plugin:status-change（payload { pluginId, ... }）
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import type {
  PluginMeta,
  PluginConfig,
  PluginLogEntry,
  PluginLogFilter,
  ConfigField
} from '../../../shared/types/plugin.types'
import type { Permission } from '../../../shared/types/permissions'
import type { PluginRendererSessionDescriptor } from '../../../shared/types/ipc.types'

// ---------------------------------------------------------------------------
// DTO（后端 serde 序列化形状）
// ---------------------------------------------------------------------------

/** plugin_list / plugin_get 返回的 PluginMetaDto（serde rename_all = "camelCase"） */
export interface PluginMetaDto {
  id: string
  name: string
  version: string
  displayName: string
  description: string
  author: string
  icon: string
  entryMain: string
  entryRenderer: string
  permissions: string[]
  configSchema: Record<string, ConfigField>
  configData: PluginConfig
  enabled: boolean
  installedPath: string
  installedAt: string
  updatedAt: string
  sortOrder: number
}

export interface UserPluginTag {
  id: number
  name: string
  pluginIds: string[]
}

export interface PluginCommandContribution {
  pluginId: string
  id: string
  title: string
  keywords: string[]
}

export interface PluginFileHandler {
  pluginId: string
  extensions: string[]
}

/** plugin_get_logs 返回的日志条目（Rust 侧 snake_case 字段） */
export interface PluginLogEntryDto {
  id: number
  plugin_id: string
  level: string
  message: string
  timestamp: string
}

function toPluginMeta(dto: PluginMetaDto): PluginMeta {
  return {
    id: dto.id,
    name: dto.name,
    version: dto.version,
    displayName: dto.displayName,
    description: dto.description,
    author: dto.author,
    icon: dto.icon || undefined,
    entryMain: dto.entryMain,
    entryRenderer: dto.entryRenderer,
    permissions: dto.permissions as Permission[],
    configSchema: dto.configSchema ?? {},
    configData: dto.configData ?? {},
    enabled: dto.enabled,
    installedAt: dto.installedAt,
    updatedAt: dto.updatedAt,
    sortOrder: dto.sortOrder
  }
}

function toLogEntry(dto: PluginLogEntryDto): PluginLogEntry {
  return {
    id: dto.id,
    pluginId: dto.plugin_id,
    level: dto.level as PluginLogEntry['level'],
    message: dto.message,
    timestamp: dto.timestamp
  }
}

// ---------------------------------------------------------------------------
// 通用返回形状
// ---------------------------------------------------------------------------

export interface IpcResult<T = unknown> {
  success: boolean
  data?: T
  error?: string
}

export interface AppUpdateMetadata {
  rid: number
  currentVersion: string
  version: string
  date?: string
  body?: string
  rawJson: Record<string, unknown>
  networkRoute: string
}

export interface MarketplaceCatalogPluginDto {
  id: string
  version: string
  artifact: string
  size: number
  url: string
  displayName?: string
  icon?: string
  category?: string
  minHostVersion?: string
  publisher?: string
  description?: string
  highlights?: string[]
  tags?: string[]
  keywords?: string[]
}

export interface MarketplaceCatalogResponse {
  schemaVersion: number
  plugins: MarketplaceCatalogPluginDto[]
  source: string
  stale: boolean
  fetchedAt: number
  networkRoute: string
}

export interface MarketplaceSource {
  id: number
  name: string
  url: string
  enabled: boolean
}

export interface PluginMarketplaceOrigin {
  pluginId: string
  sourceId: number
  sourceUrl: string
}

export interface NetworkDiagnostic {
  mode: string
  route: string
  fallbackReason?: string
  stages: Array<{ name: string; success: boolean; elapsedMs: number; message: string }>
}

export interface InstallSource {
  type: 'zip' | 'directory'
  path: string
}

/** plugin_install_preview 的预览信息（后端 lane 并行实现，按契约调用） */
export interface PluginInstallPreview {
  isUpgrade: boolean
  version: string
  previousVersion: string | null
  permissions: string[]
  addedPermissions: string[]
  removedPermissions: string[]
  legacyFullTrust: boolean
  trustLevel?: 'standard' | 'full'
  fullTrust?: boolean
  capabilities?: Record<string, boolean | Record<string, unknown>>
}

export interface PluginInstallPreviewResponse {
  success: boolean
  installToken?: string
  data?: PluginInstallPreview
  error?: string
}

// ---------------------------------------------------------------------------
// 事件 payload
// ---------------------------------------------------------------------------

export interface PluginMessageEventPayload {
  pluginId: string
  message: unknown
}

export interface PluginLogEventPayload {
  pluginId: string
  level: string
  message: string
}

export interface PluginStatusChangeEventPayload {
  pluginId: string
  status: string
}

export interface PluginClipboardEventPayload {
  pluginId: string
  text: string
}

export interface MarketplaceProgressEventPayload {
  artifact: string
  downloaded: number
  total: number
  stage: 'cached' | 'downloading' | 'downloaded'
}

export interface HostTaskEventPayload {
  id: string
  title?: string
  detail?: string
  source: 'host' | 'plugin' | 'marketplace' | 'update'
  status: 'queued' | 'running' | 'paused' | 'waiting-user' | 'completed' | 'failed' | 'cancelled'
  progress?: number
  error?: string
  owner?: string
  kind?: string
  stage?: string
  sequence?: number
  createdAt?: number
  updatedAt?: number
  resultRefs?: string[]
  executorKey?: string | null
  checkpointRef?: string | null
  parentTaskId?: string | null
}

export type HostTaskMutation = Partial<HostTaskEventPayload> & Pick<HostTaskEventPayload, 'id'>

// ---------------------------------------------------------------------------
// API 封装
// ---------------------------------------------------------------------------

export const tauriApi = {
  hostTasks: {
    cancel: (id: string): Promise<boolean> => invoke('host_task_cancel', { id }),
    revealResult: (id: string, path: string): Promise<void> =>
      invoke('host_task_reveal_result', { id, path }),
    list: (): Promise<HostTaskEventPayload[]> => invoke('host_tasks_list'),
    upsert: (change: HostTaskMutation): Promise<HostTaskEventPayload> =>
      invoke('host_task_upsert', { change }),
    removeTerminal: (id?: string): Promise<number> => invoke('host_tasks_remove_terminal', { id })
  },
  settings: {
    get: (key: string): Promise<string | null> => invoke<string | null>('settings_get', { key }),
    set: (key: string, value: string): Promise<boolean> =>
      invoke<boolean>('settings_set', { key, value }),
    getAll: (): Promise<[string, string][]> => invoke<[string, string][]>('settings_get_all')
  },

  network: {
    diagnose: (): Promise<NetworkDiagnostic> => invoke<NetworkDiagnostic>('network_diagnose')
  },

  app: {
    getVersion: (): Promise<string> => invoke<string>('app_get_version'),
    getPlatform: (): Promise<string> => invoke<string>('app_get_platform'),
    faultHistory: (): Promise<{ path: string; text: string }> => invoke('app_fault_history'),
    checkUpdate: (
      channel: 'stable' | 'beta',
      timeoutMs?: number
    ): Promise<AppUpdateMetadata | null> =>
      invoke<AppUpdateMetadata | null>('app_check_update', { channel, timeoutMs })
  },

  plugin: {
    commandContributions: (): Promise<PluginCommandContribution[]> =>
      invoke('plugin_command_contributions_list'),
    fileHandlers: (): Promise<PluginFileHandler[]> => invoke('plugin_file_handlers_list'),
    tags: {
      list: (): Promise<UserPluginTag[]> => invoke<UserPluginTag[]>('plugin_tags_list'),
      create: (name: string): Promise<number> => invoke<number>('plugin_tags_create', { name }),
      rename: (id: number, name: string): Promise<void> =>
        invoke<void>('plugin_tags_rename', { id, name }),
      delete: (id: number): Promise<void> => invoke<void>('plugin_tags_delete', { id }),
      assign: (pluginIds: string[], tagIds: number[]): Promise<void> =>
        invoke<void>('plugin_tags_assign', { pluginIds, tagIds })
    },
    marketplaceCatalog: (
      forceRefresh = false,
      channel?: 'stable' | 'beta',
      sourceId?: number
    ): Promise<MarketplaceCatalogResponse> =>
      invoke<MarketplaceCatalogResponse>('marketplace_catalog', {
        forceRefresh,
        channel,
        sourceId
      }),
    marketplaceSources: {
      list: (): Promise<MarketplaceSource[]> =>
        invoke<MarketplaceSource[]>('marketplace_sources_list'),
      add: (name: string, url: string): Promise<number> =>
        invoke<number>('marketplace_sources_add', { name, url }),
      setEnabled: (id: number, enabled: boolean): Promise<void> =>
        invoke<void>('marketplace_sources_set_enabled', { id, enabled }),
      delete: (id: number): Promise<void> => invoke<void>('marketplace_sources_delete', { id })
    },
    marketplaceOrigins: (): Promise<PluginMarketplaceOrigin[]> =>
      invoke<PluginMarketplaceOrigin[]>('marketplace_origins_list'),
    marketplaceDownload: (
      id: string,
      channel?: 'stable' | 'beta',
      priority: 'foreground' | 'normal' = 'foreground',
      taskId?: string,
      sourceId?: number
    ): Promise<string> =>
      invoke<string>('marketplace_download_plugin', { id, channel, priority, taskId, sourceId }),
    marketplaceCancel: (taskId: string): Promise<boolean> =>
      invoke<boolean>('marketplace_cancel_task', { taskId }),
    list: async (): Promise<PluginMeta[]> => {
      const dtos = await invoke<PluginMetaDto[]>('plugin_list')
      return dtos.map(toPluginMeta)
    },

    get: async (id: string): Promise<PluginMeta | null> => {
      const dto = await invoke<PluginMetaDto | null>('plugin_get', { id })
      return dto ? toPluginMeta(dto) : null
    },

    enable: (id: string): Promise<IpcResult> => invoke<IpcResult>('plugin_enable', { id }),

    disable: (id: string): Promise<IpcResult> => invoke<IpcResult>('plugin_disable', { id }),

    reorder: (orderedIds: string[]): Promise<IpcResult> =>
      invoke<IpcResult>('plugin_reorder', { orderedIds }),

    updateConfig: (id: string, config: PluginConfig): Promise<IpcResult> =>
      invoke<IpcResult>('plugin_update_config', { id, config }),

    getLogs: async (filter?: PluginLogFilter): Promise<IpcResult<PluginLogEntry[]>> => {
      const result = await invoke<{ success: boolean; data?: PluginLogEntryDto[]; error?: string }>(
        'plugin_get_logs',
        {
          pluginId: filter?.pluginId,
          level: filter?.level,
          limit: filter?.limit
        }
      )
      return {
        success: result.success,
        data: result.data?.map(toLogEntry),
        error: result.error
      }
    },

    clearLogs: (pluginId?: string): Promise<IpcResult> =>
      invoke<IpcResult>('plugin_clear_logs', { pluginId }),

    uninstall: (id: string): Promise<IpcResult> => invoke<IpcResult>('plugin_uninstall', { id }),

    createRendererSession: (
      id: string,
      colorScheme?: 'dark' | 'light'
    ): Promise<PluginRendererSessionDescriptor> =>
      invoke<PluginRendererSessionDescriptor>('create_renderer_session', { id, colorScheme }),

    disposeRendererSession: (token: string): Promise<boolean> =>
      invoke<boolean>('dispose_renderer_session', { token }),

    sendMessage: (id: string, message: unknown): Promise<unknown> =>
      invoke<unknown>('plugin_send_message', { id, message }),

    // 安装事务（1.9.3 契约；后端 lane 并行实现中）
    registerImportPath: (path: string): Promise<void> =>
      invoke<void>('plugin_register_import_path', { path }),

    installPreview: (source: InstallSource): Promise<PluginInstallPreviewResponse> =>
      invoke<PluginInstallPreviewResponse>('plugin_install_preview', { source }),

    installCommit: (
      installToken: string,
      marketplaceSourceId?: number,
      taskId?: string
    ): Promise<IpcResult> =>
      // Rust 形参名为 token（1.9.3 起即如此；此前误传 installToken 导致确认安装必报
      // "missing required key token"，1.9.13 修复）
      invoke<IpcResult>('plugin_install_commit', {
        token: installToken,
        marketplaceSourceId,
        taskId
      }),

    installDiscard: (installToken: string): Promise<IpcResult> =>
      invoke<IpcResult>('plugin_install_discard', { token: installToken })
  },

  dialog: {
    openSelection: async (options: {
      directory: boolean
      multiple?: boolean
      extensions?: string[]
    }): Promise<string[]> => {
      const selected = await openDialog({
        multiple: options.multiple ?? false,
        directory: options.directory,
        ...(options.extensions && options.extensions.length > 0
          ? { filters: [{ name: '支持的文件', extensions: options.extensions }] }
          : {})
      })
      if (!selected) return []
      return Array.isArray(selected) ? selected : [selected]
    },
    /** 选择 .zip 插件包（@tauri-apps/plugin-dialog，替代 Electron dialog:open-file） */
    openFile: async (): Promise<string | null> => {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: 'ZIP 插件包', extensions: ['zip'] }]
      })
      return typeof selected === 'string' ? selected : null
    },

    /** 多选 .zip 插件包（1.9.12 批量导入） */
    openFiles: async (): Promise<string[]> => {
      const selected = await openDialog({
        multiple: true,
        directory: false,
        filters: [{ name: 'ZIP 插件包', extensions: ['zip'] }]
      })
      if (!selected) return []
      return Array.isArray(selected) ? selected : [selected]
    },

    /** 选择插件目录（@tauri-apps/plugin-dialog，替代 Electron dialog:open-directory） */
    openDirectory: async (): Promise<string | null> => {
      const selected = await openDialog({ multiple: false, directory: true })
      return typeof selected === 'string' ? selected : null
    }
  },

  events: {
    onMessage: (callback: (payload: PluginMessageEventPayload) => void): Promise<UnlistenFn> =>
      listen<PluginMessageEventPayload>('plugin:message', (event) => callback(event.payload)),

    onLog: (callback: (payload: PluginLogEventPayload) => void): Promise<UnlistenFn> =>
      listen<PluginLogEventPayload>('plugin:log', (event) => callback(event.payload)),

    onStatusChange: (
      callback: (payload: PluginStatusChangeEventPayload) => void
    ): Promise<UnlistenFn> =>
      listen<PluginStatusChangeEventPayload>('plugin:status-change', (event) =>
        callback(event.payload)
      ),

    onClipboard: (callback: (payload: PluginClipboardEventPayload) => void): Promise<UnlistenFn> =>
      listen<PluginClipboardEventPayload>('plugin:clipboard', (event) => callback(event.payload)),

    onMarketplaceProgress: (
      callback: (payload: MarketplaceProgressEventPayload) => void
    ): Promise<UnlistenFn> =>
      listen<MarketplaceProgressEventPayload>('marketplace:progress', (event) =>
        callback(event.payload)
      ),

    onHostTask: (callback: (payload: HostTaskEventPayload) => void): Promise<UnlistenFn> =>
      listen<HostTaskEventPayload>('host:task', (event) => callback(event.payload))
  }
}

export type TauriApi = typeof tauriApi

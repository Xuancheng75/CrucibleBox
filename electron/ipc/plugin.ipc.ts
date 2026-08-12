import { ipcMain, dialog } from 'electron'
import { IpcChannel } from '@shared/types/ipc.types'
import type { PluginManager } from '../../plugin-system/PluginManager'
import { assertTrustedSender } from './ipcGuard'
import { join } from 'node:path'
import { readPluginManifest } from '../../plugin-system/PluginManifestPolicy'
import type { PluginRendererSessionRegistry } from '../../plugin-system/PluginRendererSessionRegistry'
import type { PluginRendererSessionDescriptor } from '@shared/types/ipc.types'

/** Paths the user picked via the native file/folder dialog this session. */
const trustedInstallPaths = new Set<string>()

export function rememberTrustedInstallPath(path: string | null): void {
  if (path) {
    trustedInstallPaths.add(path)
    // Drop old entries to avoid unbounded growth.
    if (trustedInstallPaths.size > 50) {
      const first = trustedInstallPaths.values().next().value
      if (first) trustedInstallPaths.delete(first)
    }
  }
}

function assertInstallSource(source: { type: 'zip' | 'directory'; path: string }): void {
  if (!source || (source.type !== 'zip' && source.type !== 'directory')) {
    throw new Error('无效的安装来源')
  }
  if (!trustedInstallPaths.has(source.path)) {
    throw new Error('安装路径未经用户确认')
  }
}

export interface PluginRendererIpcDependencies {
  registry: PluginRendererSessionRegistry
  runtimePath: string
}

export function registerPluginIpc(
  pluginManager: PluginManager,
  renderer: PluginRendererIpcDependencies
): void {
  ipcMain.handle(IpcChannel.PluginList, (event) => {
    assertTrustedSender(event)
    try {
      return pluginManager.getInstalledPlugins()
    } catch (err) {
      console.error('[IPC] PluginList error:', err)
      return []
    }
  })

  ipcMain.handle(IpcChannel.PluginGet, (event, id: string) => {
    assertTrustedSender(event)
    try {
      return pluginManager.getPlugin(id)
    } catch (err) {
      console.error('[IPC] PluginGet error:', err)
      return null
    }
  })

  ipcMain.handle(
    IpcChannel.PluginInstall,
    async (event, source: { type: 'zip' | 'directory'; path: string }) => {
      assertTrustedSender(event)
      let installedId: string | null = null
      let wasUpgrade = false
      let preparedToken: string | null = null
      try {
        assertInstallSource(source)
        const preview = pluginManager.previewInstall(source)
        if (!preview || 'error' in preview) {
          const detail = preview && 'error' in preview ? preview.error : ''
          return {
            success: false,
            error: detail || '无法安装插件：预览失败'
          }
        }
        preparedToken = preview.installToken
        const permissionChanges = preview.isUpgrade
          ? `\n新增权限: ${preview.addedPermissions.join(', ') || '无'}\n移除权限: ${preview.removedPermissions.join(', ') || '无'}`
          : ''
        const runtimeTrust = preview.legacyFullTrust
          ? 'Legacy Full Trust（兼容模式；插件 backend 视为完全可信代码）'
          : preview.backend
            ? `Manifest v${preview.manifestVersion} / Backend API v${preview.backendApiVersion} / Renderer API v${preview.rendererApiVersion}`
            : `Manifest v${preview.manifestVersion} / Renderer-only / Renderer API v${preview.rendererApiVersion}`
        const confirmResult = await dialog.showMessageBox({
          type: preview.legacyFullTrust || preview.addedPermissions.length > 0 ? 'warning' : 'info',
          title: '安装插件确认',
          message: `${preview.isUpgrade ? '确认升级' : '确认安装'}插件「${preview.displayName || preview.name}」？`,
          detail: `名称: ${preview.name}\n版本: ${preview.previousVersion ? `${preview.previousVersion} → ` : ''}${preview.version}\n作者: ${preview.author || '未知'}\n描述: ${preview.description || '无'}\n运行模式: ${runtimeTrust}\n权限: ${preview.permissions.join(', ') || '无'}${permissionChanges}\n\n安装第三方插件意味着信任其代码可访问本机资源。`,
          buttons: ['取消', '安装'],
          defaultId: 1,
          cancelId: 0
        })
        if (confirmResult.response !== 1) {
          pluginManager.discardPreparedInstall(preparedToken)
          preparedToken = null
          return { success: false, error: '用户取消安装' }
        }
        const existingNames = new Set(pluginManager.getInstalledPlugins().map((p) => p.name))
        const result = await pluginManager.commitPreparedInstall(preparedToken)
        preparedToken = null
        installedId = result.id
        wasUpgrade = existingNames.has(result.name)
        // The manager preserves an upgraded plugin's previous enabled state and
        // performs its own rollback if reactivation fails. Only a new install is
        // enabled here; otherwise upgrading a disabled plugin would enable it.
        if (!wasUpgrade) await pluginManager.activatePlugin(result.id)
        return { success: true, data: result }
      } catch (err) {
        if (preparedToken) pluginManager.discardPreparedInstall(preparedToken)
        if (installedId && !wasUpgrade) {
          try {
            await pluginManager.uninstall(installedId)
          } catch (rollbackError) {
            console.error('[IPC] Failed to roll back plugin installation:', rollbackError)
          }
        }
        return { success: false, error: (err as Error).message }
      }
    }
  )

  ipcMain.handle(IpcChannel.PluginRegisterImportPath, (event, path: string) => {
    assertTrustedSender(event)
    if (typeof path === 'string' && path.length > 0) {
      rememberTrustedInstallPath(path)
    }
    return { success: true }
  })

  ipcMain.handle(IpcChannel.PluginCreateRendererSession, (event, id: string) => {
    assertTrustedSender(event)
    if (typeof id !== 'string' || id.length === 0 || id.length > 128) {
      throw new Error('Invalid plugin id')
    }
    const plugin = pluginManager.getPlugin(id)
    if (!plugin || !plugin.enabled) throw new Error('Plugin is unavailable')
    const pluginDirectory = join(pluginManager.pluginsPath, plugin.name)
    const manifest = readPluginManifest(pluginDirectory)
    if (
      manifest.name !== plugin.name ||
      manifest.renderer !== plugin.entryRenderer ||
      JSON.stringify([...manifest.permissions].sort()) !==
        JSON.stringify([...plugin.permissions].sort())
    ) {
      throw new Error('Plugin metadata does not match the installed manifest')
    }
    const session = renderer.registry.create({
      pluginId: plugin.id,
      pluginName: plugin.name,
      pluginDirectory,
      rendererEntry: manifest.renderer,
      runtimePath: renderer.runtimePath,
      rendererApiVersion: manifest.rendererApiVersion ?? 1,
      permissions: manifest.permissions,
      ownerWebContentsId: event.sender.id
    })
    const descriptor: PluginRendererSessionDescriptor = {
      token: session.token,
      handshakeToken: session.handshakeToken,
      origin: session.origin,
      indexUrl: session.indexUrl,
      rendererApiVersion: session.rendererApiVersion,
      expiresAt: session.expiresAt
    }
    return descriptor
  })

  ipcMain.handle(IpcChannel.PluginDisposeRendererSession, (event, token: string) => {
    assertTrustedSender(event)
    if (typeof token !== 'string') return { success: false }
    const access = renderer.registry.get(token, event.sender.id)
    if (!access.ok) return { success: false }
    return { success: renderer.registry.dispose(token) }
  })

  ipcMain.handle(IpcChannel.PluginUninstall, async (event, id: string) => {
    assertTrustedSender(event)
    try {
      await pluginManager.uninstall(id)
      return { success: true }
    } catch (err) {
      return { success: false, error: (err as Error).message }
    }
  })

  ipcMain.handle(IpcChannel.PluginEnable, async (event, id: string) => {
    assertTrustedSender(event)
    try {
      await pluginManager.activatePlugin(id)
      return { success: true }
    } catch (err) {
      return { success: false, error: (err as Error).message }
    }
  })

  ipcMain.handle(IpcChannel.PluginDisable, async (event, id: string) => {
    assertTrustedSender(event)
    try {
      await pluginManager.deactivatePlugin(id)
      return { success: true }
    } catch (err) {
      return { success: false, error: (err as Error).message }
    }
  })

  ipcMain.handle(IpcChannel.PluginReorder, async (event, orderedIds: string[]) => {
    assertTrustedSender(event)
    try {
      const plugins = pluginManager.reorderPlugins(orderedIds)
      return { success: true, data: plugins }
    } catch (err) {
      return { success: false, error: (err as Error).message }
    }
  })

  ipcMain.handle(
    IpcChannel.PluginUpdateConfig,
    async (event, id: string, config: Record<string, unknown>) => {
      assertTrustedSender(event)
      try {
        await pluginManager.updateConfig(id, config)
        return { success: true }
      } catch (err) {
        return { success: false, error: (err as Error).message }
      }
    }
  )

  ipcMain.handle(IpcChannel.PluginSendMessage, async (event, id: string, message: unknown) => {
    assertTrustedSender(event)
    try {
      const result = await pluginManager.sendMessage(id, message)
      return result ?? { success: true }
    } catch (err) {
      return { success: false, error: (err as Error).message }
    }
  })

  ipcMain.handle(
    IpcChannel.PluginGetLogs,
    (event, filter: { pluginId?: string; level?: string; limit?: number }) => {
      assertTrustedSender(event)
      try {
        return { success: true, data: pluginManager.getLogs(filter) }
      } catch (err) {
        return { success: false, error: (err as Error).message }
      }
    }
  )

  ipcMain.handle(IpcChannel.PluginClearLogs, (event, pluginId?: string) => {
    assertTrustedSender(event)
    try {
      pluginManager.clearLogs(pluginId)
      return { success: true }
    } catch (err) {
      return { success: false, error: (err as Error).message }
    }
  })

  ipcMain.handle(IpcChannel.DialogOpenFile, async (event) => {
    assertTrustedSender(event)
    try {
      const result = await dialog.showOpenDialog({
        properties: ['openFile'],
        filters: [
          { name: '插件包', extensions: ['zip'] },
          { name: '所有文件', extensions: ['*'] }
        ]
      })
      const picked = result.canceled ? null : result.filePaths[0]
      rememberTrustedInstallPath(picked)
      return picked
    } catch (err) {
      console.error('[IPC] DialogOpenFile error:', err)
      return null
    }
  })

  ipcMain.handle(IpcChannel.DialogOpenDirectory, async (event) => {
    assertTrustedSender(event)
    try {
      const result = await dialog.showOpenDialog({
        properties: ['openDirectory']
      })
      const picked = result.canceled ? null : result.filePaths[0]
      rememberTrustedInstallPath(picked)
      return picked
    } catch (err) {
      console.error('[IPC] DialogOpenDirectory error:', err)
      return null
    }
  })
}

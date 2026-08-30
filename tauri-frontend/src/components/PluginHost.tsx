import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Modal, Spin } from 'antd'
import type { PluginConfig } from '../../../shared/types/plugin.types'
import type { PluginRendererSessionDescriptor } from '../../../shared/types/ipc.types'
import type { PluginRendererRpcJsonValue } from '../../../shared/types/plugin-renderer-rpc.types'
import { Permission } from '../../../shared/types/permissions'
import { PluginFrameBridge } from '../../../src/plugin-runtime/PluginFrameBridge'
import { tauriApi } from '../api/tauriApi'
import { themeApi } from '../api/theme.api'
import { useThemeStore } from '../store/theme.store'

interface PluginHostProps {
  pluginId: string
  pluginName: string
  rendererEntry: string
  config: PluginConfig
  permissions?: Permission[]
  onConfigChange: (config: PluginConfig) => void
}

export function PluginHost({
  pluginId,
  pluginName,
  rendererEntry,
  config,
  permissions = [],
  onConfigChange
}: PluginHostProps) {
  const [session, setSession] = useState<PluginRendererSessionDescriptor | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [height, setHeight] = useState(600)
  const [ready, setReady] = useState(false)
  const iframeRef = useRef<HTMLIFrameElement>(null)
  const bridgeRef = useRef<PluginFrameBridge | null>(null)
  const connectedTokenRef = useRef<string | null>(null)
  const readyTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const configRef = useRef(config)
  const onConfigChangeRef = useRef(onConfigChange)
  const theme = useThemeStore((state) => state.theme)
  const permissionKey = useMemo(() => [...permissions].sort().join('\n'), [permissions])
  // 插件 confirm 的宿主侧实现：沙箱 iframe 无 allow-modals，window.confirm 被浏览器
  // 静默拒绝（恒 false → UniEnv 非 current 版本安装无声取消，Bug B 真机发现）。
  // 用 antd Modal 承载，不放宽 sandbox。
  const [confirmApi, confirmContext] = Modal.useModal()

  configRef.current = config
  onConfigChangeRef.current = onConfigChange

  useEffect(() => {
    let active = true
    let issuedToken: string | null = null
    bridgeRef.current?.dispose()
    bridgeRef.current = null
    connectedTokenRef.current = null
    if (readyTimeoutRef.current) clearTimeout(readyTimeoutRef.current)
    readyTimeoutRef.current = null
    setSession(null)
    setError(null)
    setHeight(600)
    setReady(false)

    tauriApi.plugin
      .createRendererSession(
        pluginId,
        useThemeStore.getState().theme.mode === 'light' ? 'light' : 'dark'
      )
      .then((nextSession) => {
        issuedToken = nextSession.token
        if (active) {
          setSession(nextSession)
          return
        }
        void tauriApi.plugin.disposeRendererSession(nextSession.token)
      })
      .catch((reason: unknown) => {
        if (active) {
          setError(`加载插件界面失败：${reason instanceof Error ? reason.message : String(reason)}`)
        }
      })

    return () => {
      active = false
      bridgeRef.current?.dispose()
      bridgeRef.current = null
      connectedTokenRef.current = null
      if (readyTimeoutRef.current) clearTimeout(readyTimeoutRef.current)
      readyTimeoutRef.current = null
      if (issuedToken) void tauriApi.plugin.disposeRendererSession(issuedToken)
    }
  }, [pluginId, pluginName, rendererEntry, permissionKey])

  useEffect(() => {
    bridgeRef.current?.updateConfig(config)
  }, [config])

  useEffect(() => {
    bridgeRef.current?.updateTheme(theme)
  }, [theme])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    tauriApi.events
      .onMessage((data) => {
        if (data.pluginId === pluginId) {
          try {
            bridgeRef.current?.sendBackendMessage(data.message as PluginRendererRpcJsonValue)
          } catch (reason) {
            console.error('[PluginFrameBridge] invalid backend event', reason)
          }
        }
      })
      .then((fn) => {
        unlisten = fn
      })
    return () => unlisten?.()
  }, [pluginId])

  // Clipboard monitor events are delivered to the backend for persistence and
  // also forwarded to the renderer so the history view updates immediately.
  // Keeping this event path separate from plugin:message avoids echoing the
  // monitor event back into the sidecar.
  useEffect(() => {
    let unlisten: (() => void) | undefined
    tauriApi.events
      .onClipboard((data) => {
        if (data.pluginId !== pluginId) return
        try {
          bridgeRef.current?.sendBackendMessage({
            type: 'clipboard:changed',
            text: data.text
          })
        } catch (reason) {
          console.error('[PluginFrameBridge] invalid clipboard event', reason)
        }
      })
      .then((fn) => {
        unlisten = fn
      })
    return () => unlisten?.()
  }, [pluginId])

  // 窗口级 OS 拖放由宿主解析真实路径后转发；iframe 内的 File 对象不再自行猜测路径。
  useEffect(() => {
    const handleDocumentDrop = (event: Event) => {
      const detail = (event as CustomEvent<{ pluginId?: unknown; paths?: unknown }>).detail
      if (detail?.pluginId !== undefined && detail.pluginId !== pluginId) return
      const paths = detail?.paths
      if (!Array.isArray(paths)) return
      const safePaths = paths.filter((path): path is string => typeof path === 'string')
      if (safePaths.length > 0) bridgeRef.current?.sendFilesDropped(safePaths)
    }
    window.addEventListener('cruciblebox:document-files-dropped', handleDocumentDrop)
    return () =>
      window.removeEventListener('cruciblebox:document-files-dropped', handleDocumentDrop)
  }, [pluginId])

  const connectFrame = useCallback(() => {
    if (!session || connectedTokenRef.current === session.token) return
    const targetWindow = iframeRef.current?.contentWindow
    if (!targetWindow) return

    const bridge = new PluginFrameBridge({
      token: session.handshakeToken,
      origin: session.origin,
      permissions,
      initialConfig: configRef.current,
      initialTheme: useThemeStore.getState().theme,
      // 直传 pluginId：宿主 webview 是可信调用方（sandboxed renderer 无法直接
      // invoke 命令）。旧版传 session token，session 过期/重建后反查失败会变成
      // 笼统的 INTERNAL_ERROR（Bug C）；token 路径在宿主侧仍保留兼容。
      sendToBackend: (message) => tauriApi.plugin.sendMessage(pluginId, message),
      updateConfig: (nextConfig) => onConfigChangeRef.current(nextConfig),
      showNotification: (title, body) => {
        try {
          // WebView2 中 Notification API 可能不可用；失败时静默降级
          new Notification(title, { body })
          return true
        } catch {
          return false
        }
      },
      getTheme: () => useThemeStore.getState().theme,
      listThemes: () => themeApi.list(),
      async setTheme(nextTheme) {
        return useThemeStore.getState().setTheme(nextTheme)
      },
      async confirm(options) {
        return await new Promise<boolean>((resolve) => {
          confirmApi.confirm({
            title: options.title,
            content: options.message,
            okText: options.confirmLabel ?? '确定',
            cancelText: options.cancelLabel ?? '取消',
            onOk: () => resolve(true),
            onCancel: () => resolve(false)
          })
        })
      },
      async openDialog(options) {
        const selected = await tauriApi.dialog.openSelection({
          directory: options.type === 'folder',
          multiple: options.multiple ?? false,
          extensions: options.extensions
        })
        return selected
      },
      resize: (nextHeight) => {
        if (readyTimeoutRef.current) clearTimeout(readyTimeoutRef.current)
        readyTimeoutRef.current = null
        setHeight(Math.max(100, Math.min(16384, nextHeight)))
        setReady(true)
      },
      onProtocolError: (reason) => console.error('[PluginFrameBridge]', reason)
    })
    try {
      bridgeRef.current = bridge
      connectedTokenRef.current = session.token
      readyTimeoutRef.current = setTimeout(() => {
        readyTimeoutRef.current = null
        bridge.dispose()
        bridgeRef.current = null
        connectedTokenRef.current = null
        setError('连接插件界面超时')
      }, 10_000)
      bridge.connect(targetWindow)
    } catch (reason) {
      if (readyTimeoutRef.current) clearTimeout(readyTimeoutRef.current)
      readyTimeoutRef.current = null
      bridge.dispose()
      bridgeRef.current = null
      connectedTokenRef.current = null
      setError(`连接插件界面失败：${reason instanceof Error ? reason.message : String(reason)}`)
    }
  }, [permissions, session, pluginId, confirmApi])

  if (error) return <Alert className="ob-alert-error" type="error" message={error} showIcon />

  if (!session) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}>
        <Spin tip="正在加载插件..." />
      </div>
    )
  }

  return (
    <>
      {confirmContext}
      <iframe
        ref={iframeRef}
        src={session.indexUrl}
        title={`${pluginName} 插件`}
        sandbox={
          session.rendererApiVersion === 1
            ? 'allow-scripts allow-same-origin allow-downloads allow-modals'
            : 'allow-scripts allow-same-origin allow-downloads'
        }
        allow="camera 'none'; microphone 'none'; geolocation 'none'; payment 'none'; usb 'none'; serial 'none'; clipboard-read 'none'; clipboard-write 'none'"
        referrerPolicy="no-referrer"
        data-plugin-ready={ready ? 'true' : 'false'}
        data-renderer-api-version={session.rendererApiVersion}
        onLoad={connectFrame}
        style={{
          display: 'block',
          width: '100%',
          height,
          border: 0,
          background: 'transparent'
        }}
      />
    </>
  )
}

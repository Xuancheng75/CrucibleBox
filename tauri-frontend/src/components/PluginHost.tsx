import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Spin } from 'antd'
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
  const sessionTokenRef = useRef<string | null>(null)
  const readyTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const configRef = useRef(config)
  const onConfigChangeRef = useRef(onConfigChange)
  const theme = useThemeStore((state) => state.theme)
  const permissionKey = useMemo(() => [...permissions].sort().join('\n'), [permissions])

  configRef.current = config
  onConfigChangeRef.current = onConfigChange

  useEffect(() => {
    let active = true
    let issuedToken: string | null = null
    bridgeRef.current?.dispose()
    bridgeRef.current = null
    connectedTokenRef.current = null
    sessionTokenRef.current = null
    if (readyTimeoutRef.current) clearTimeout(readyTimeoutRef.current)
    readyTimeoutRef.current = null
    setSession(null)
    setError(null)
    setHeight(600)
    setReady(false)

    tauriApi.plugin
      .createRendererSession(pluginId)
      .then((nextSession) => {
        issuedToken = nextSession.token
        if (active) {
          sessionTokenRef.current = nextSession.token
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
      sessionTokenRef.current = null
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
    tauriApi.events.onMessage((data) => {
      if (data.pluginId === pluginId) {
        try {
          bridgeRef.current?.sendBackendMessage(data.message as PluginRendererRpcJsonValue)
        } catch (reason) {
          console.error('[PluginFrameBridge] invalid backend event', reason)
        }
      }
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
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
      // 后端 plugin_send_message 从 renderer session registry 反查 plugin_id，
      // 因此传 session token（对等 tauri-frontend 骨架 PluginHost 约定）。
      sendToBackend: (message) => tauriApi.plugin.sendMessage(sessionTokenRef.current ?? session.token, message),
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
        return window.confirm(`${options.title}\n\n${options.message}`)
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
  }, [permissions, session])

  if (error) return <Alert className="ob-alert-error" type="error" message={error} showIcon />

  if (!session) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}>
        <Spin tip="正在加载插件..." />
      </div>
    )
  }

  return (
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
  )
}
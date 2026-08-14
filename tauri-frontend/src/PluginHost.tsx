import { useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { PluginFrameBridge } from '../../src/plugin-runtime/PluginFrameBridge'

interface RendererSessionDescriptor {
  token: string
  handshakeToken: string
  origin: string
  indexUrl: string
  rendererApiVersion: number
  expiresAt: number
}

interface PluginMeta {
  id: string
  name: string
  displayName: string
  enabled: boolean
}

// 最小插件宿主：列出插件 → 创建 renderer session → iframe 加载协议 URL →
// PluginFrameBridge 握手（与 Electron 版 PluginHost.tsx 同构，注入层换 tauri invoke/event）。
export function PluginHost() {
  const [plugins, setPlugins] = useState<PluginMeta[]>([])
  const [active, setActive] = useState<RendererSessionDescriptor | null>(null)
  const [height, setHeight] = useState(400)
  const bridgeRef = useRef<PluginFrameBridge | null>(null)
  const frameRef = useRef<HTMLIFrameElement | null>(null)

  useEffect(() => {
    invoke<PluginMeta[]>('plugin_list').then(setPlugins).catch(console.error)
  }, [])

  const dispose = useCallback(() => {
    bridgeRef.current?.dispose()
    bridgeRef.current = null
    if (active) {
      invoke('dispose_renderer_session', { token: active.token }).catch(console.error)
    }
    setActive(null)
  }, [active])

  useEffect(() => () => dispose(), [dispose])

  const openPlugin = useCallback(
    async (id: string) => {
      dispose()
      const session = await invoke<RendererSessionDescriptor>('create_renderer_session', { id })
      setActive(session)
    },
    [dispose]
  )

  const connectFrame = useCallback(() => {
    if (!active || !frameRef.current) return
    const bridge = new PluginFrameBridge({
      token: active.handshakeToken,
      origin: active.origin,
      permissions: [],
      initialConfig: {},
      initialTheme: null,
      sendToBackend: (message: unknown) =>
        invoke('plugin_send_message', { id: active.token, message }).catch((e) =>
          console.error('sendToBackend', e)
        ),
      updateConfig: (config) => console.log('[bridge] config.update', config),
      showNotification: (title: string, body?: string) => {
        try {
          // 在 Tauri 的 WebView2 中 Notification API 可能不可用；退化为 console
          new Notification(title, { body })
        } catch {
          console.log(`[bridge] notify: ${title} ${body ?? ''}`)
        }
      },
      getTheme: () => null,
      listThemes: () => [],
      setTheme: (theme) => {
        console.log('[bridge] theme.set', theme)
        return null
      },
      confirm: (message: string) => window.confirm(message),
      resize: (h: number) => {
        setHeight(Math.min(16384, Math.max(100, h)))
        console.log('[bridge] resize', h)
      },
      onReady: () => console.log('[bridge] ready'),
      onProtocolError: (err) => console.error('[bridge] protocol error', err)
    })
    bridge.connect(frameRef.current.contentWindow)
    bridgeRef.current = bridge
  }, [active])

  return (
    <div style={{ padding: 16, fontFamily: 'system-ui' }}>
      <h2>CrucibleBox Tauri 骨架 — 插件宿主</h2>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: 12 }}>
        {plugins.map((p) => (
          <button key={p.id} onClick={() => openPlugin(p.id)}>
            {p.displayName || p.name}
          </button>
        ))}
        {active && (
          <button onClick={dispose} style={{ marginLeft: 8 }}>
            关闭
          </button>
        )}
      </div>
      {active && (
        <iframe
          ref={frameRef}
          src={active.indexUrl}
          onLoad={connectFrame}
          sandbox="allow-scripts allow-same-origin allow-downloads"
          referrerPolicy="no-referrer"
          style={{
            width: '100%',
            height,
            border: '1px solid #e5e7eb',
            borderRadius: 8,
            background: '#fff'
          }}
        />
      )}
    </div>
  )
}

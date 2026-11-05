import { createNextUiController } from '../features/plugins/runtime/NextUiController'
import { useThemeStore } from '../store/theme.store'
import { useAppStore } from '../store/app.store'
import { themeToCssVars } from '../../../shared/themes/css-vars'
import { validateAppearance, validateFilesDropped } from '../../../shared/frame-validation.mjs'
import { useEffect, useRef, useState } from 'react'
import { validateResponse as validateNext2Response } from '../../../packages/cruciblebox-next2-api/src/index.mjs'
import { Alert, Button, Spin } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import {
  createNextDispatcher,
  isNextReady,
  transport,
  nextSandbox,
  createNextLease,
  type NextSession
} from '../features/plugins/runtime/NextFrameBridge'

export function NextPluginFrame({
  session,
  name,
  pluginId
}: {
  session: NextSession
  name: string
  pluginId: string
}) {
  const frameRef = useRef<HTMLIFrameElement>(null)
  const [ready, setReady] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [leaseError, setLeaseError] = useState(false)
  const reconnectRef = useRef<(() => Promise<void>) | null>(null)
  useEffect(() => {
    const frame = frameRef.current
    if (!frame) return
    let port: MessagePort | undefined
    let dispatcher: ReturnType<typeof createNextDispatcher> | undefined
    let connected = false
    let active = true
    let themeStyleEnabled = false
    const themeController = createNextUiController()
    function shutdown(message: string) {
      if (!active) return
      active = false
      themeController.dispose()
      clearTimeout(timer)
      lease.dispose()
      reconnectRef.current = null
      window.removeEventListener('focus', resumeLease)
      document.removeEventListener('visibilitychange', resumeLease)
      unsubscribe()
      unsubscribeVisibility()
      window.removeEventListener('message', listener)
      dropEvents.forEach((name) => window.removeEventListener(name, drop))
      dispatcher?.dispose()
      port?.close()
      frame!.setAttribute('sandbox', 'allow-scripts')
      frame!.src = 'about:blank'
      setReady(false)
      setError(message)
      void invoke('dispose_renderer_session', { token: session.token }).catch(() => {})
    }
    const drop = (event: Event) => {
      if (!active || !port || !session.permissions?.includes('dialog')) return
      const detail = (event as CustomEvent<{ pluginId?: unknown; paths?: unknown }>).detail
      if (detail?.pluginId !== pluginId) return
      try {
        port.postMessage({
          kind: 'next-files-dropped',
          paths: validateFilesDropped(JSON.stringify(detail.paths))
        })
      } catch {
        /* Invalid or oversized OS drop leaves the session usable. */
      }
    }
    const dropEvents = ['cruciblebox:plugin-files-dropped']
    dropEvents.forEach((name) => window.addEventListener(name, drop))
    const lease = createNextLease(
      session,
      () =>
        invoke<{ expiresAt: number }>(
          'renew_next_renderer_session',
          { token: session.token },
          { headers: { Origin: window.location.origin } }
        ),
      (reason) => {
        // A directly granted browser capability cannot be left usable after failed
        // reauthorization. Other pages can retain local drafts while RPC is denied.
        if (
          session.permissions?.includes('browser:downloads') ||
          ['SESSION_DENIED', 'SESSION_REPLACED'].includes(String(reason))
        ) {
          shutdown('插件连接已失效，请重新打开')
        } else setLeaseError(true)
      },
      () => setLeaseError(false)
    )
    reconnectRef.current = () => lease.ensure(true)
    const resumeLease = () => {
      if (document.visibilityState === 'visible') void lease.ensure().catch(() => {})
    }
    window.addEventListener('focus', resumeLease)
    document.addEventListener('visibilitychange', resumeLease)
    const getAppearance = () =>
      validateAppearance(
        JSON.stringify({
          mode: useThemeStore.getState().theme.mode,
          cssVars: themeToCssVars(useThemeStore.getState().theme)
        })
      )
    const unsubscribe = useThemeStore.subscribe((state, previous) => {
      if (!active || !port) return
      try {
        if (state.theme !== previous.theme)
          port.postMessage({ kind: 'next-appearance', appearance: getAppearance() })
        if (themeStyleEnabled && state.style !== previous.style && state.style)
          port.postMessage({ kind: 'next-theme-style', style: state.style })
      } catch {
        shutdown('插件主题连接已关闭')
      }
    })
    const sendVisibility = () => {
      const state = useAppStore.getState()
      const visible = state.currentPage === 'pluginView' && state.activePluginId === pluginId
      themeController.setVisible(visible)
      if (active && port)
        port.postMessage({
          kind: 'next-visibility',
          visible
        })
    }
    const unsubscribeVisibility = useAppStore.subscribe((state, previous) => {
      if (
        state.currentPage !== previous.currentPage ||
        state.activePluginId !== previous.activePluginId
      )
        sendVisibility()
    })
    sendVisibility()
    const timer = setTimeout(() => {
      shutdown('连接插件界面超时')
    }, transport.handshakeTimeoutMs)
    const listener = (event: MessageEvent) => {
      if (!active || connected || !isNextReady(event, frame.contentWindow, session)) return
      connected = true
      const channel = new MessageChannel()
      port = channel.port1
      dispatcher = createNextDispatcher(
        session,
        async (raw) => {
          const nativeResponse = await invoke<unknown>(
            'next_renderer_request',
            { raw },
            { headers: { Origin: window.location.origin } }
          )
          const requestId = (JSON.parse(raw) as { requestId: string }).requestId
          const response = validateNext2Response(JSON.stringify(nativeResponse), requestId)
          if (!active || !response.ok) return response
          const request = JSON.parse(raw) as { method: string; params: Record<string, unknown> }
          if (
            (!request.method.startsWith('theme.') || request.method.startsWith('theme.assets.')) &&
            !['dialog.open', 'dialog.confirm', 'notification.show'].includes(request.method)
          )
            return response
          const grant = response.result as { operation?: unknown; params?: unknown } | null
          if (
            !grant ||
            grant.operation !== request.method ||
            !grant.params ||
            typeof grant.params !== 'object' ||
            Array.isArray(grant.params)
          )
            throw new Error('INVALID_RESPONSE')
          return {
            ...response,
            result: await themeController.execute(
              request.method,
              grant.params as Record<string, unknown>
            )
          }
        },
        (response) => port?.postMessage({ kind: 'next-response', response }),
        () => lease.ensure()
      )
      port.onmessageerror = () => {
        shutdown('插件连接已关闭')
      }
      port.onmessage = ({ data }: MessageEvent) => {
        if (!active) return
        if (data?.kind === 'next-mounted') {
          clearTimeout(timer)
          setReady(true)
          sendVisibility()
        } else if (data?.kind === 'next-theme-style-subscribe' && data.version === 2) {
          themeStyleEnabled = true
          const style = useThemeStore.getState().style
          if (style) port?.postMessage({ kind: 'next-theme-style', style })
        } else if (data?.kind === 'next-request') dispatcher?.dispatch(data.request)
        else {
          shutdown('插件连接已关闭')
        }
      }
      port.start()
      frame.contentWindow?.postMessage(
        {
          kind: 'next-connect',
          wireVersion: 4,
          nonce: session.handshakeToken,
          session: session.token,
          appearance: getAppearance()
        },
        '*',
        [channel.port2]
      )
    }
    window.addEventListener('message', listener)
    frame.src = session.indexUrl
    return () => {
      active = false
      themeController.dispose()
      unsubscribe()
      unsubscribeVisibility()
      lease.dispose()
      reconnectRef.current = null
      window.removeEventListener('focus', resumeLease)
      document.removeEventListener('visibilitychange', resumeLease)
      clearTimeout(timer)
      window.removeEventListener('message', listener)
      dropEvents.forEach((name) => window.removeEventListener(name, drop))
      dispatcher?.dispose()
      port?.close()
    }
  }, [session, pluginId])
  return (
    <>
      {!ready && !error && (
        <div className="cbx-plugin-loading" role="status">
          <Spin />
          <span>正在打开插件…</span>
        </div>
      )}
      {error && <Alert type="error" message={error} />}
      {leaseError && !error && (
        <Alert
          type="warning"
          message="连接暂时不可用，当前页面内容已保留。"
          action={
            <Button
              size="small"
              onClick={() => {
                void reconnectRef.current?.().catch(() => {})
              }}
            >
              重试连接
            </Button>
          }
        />
      )}
      <iframe
        ref={frameRef}
        title={`${name} 插件`}
        sandbox={nextSandbox(session)}
        referrerPolicy="no-referrer"
        data-plugin-ready={ready ? 'true' : 'false'}
        aria-busy={!ready}
        data-renderer-api-version={session.rendererApiVersion}
        allow="camera 'none'; microphone 'none'; geolocation 'none'; payment 'none'; usb 'none'; serial 'none'; clipboard-read 'none'; clipboard-write 'none'"
        style={{
          display: 'block',
          width: '100%',
          height: '100%',
          minHeight: 360,
          flex: 1,
          border: 0,
          background: 'transparent'
        }}
      />
    </>
  )
}

import { createNextUiController } from '../features/plugins/runtime/NextUiController'
import { useThemeStore } from '../store/theme.store'
import { themeToCssVars } from '../../../shared/themes/css-vars'
import {
  validateAppearance,
  validateFilesDropped,
  validateResponse
} from '../../../packages/cruciblebox-next-api/src/index.mjs'
import { useEffect, useRef, useState } from 'react'
import { Alert } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import {
  createNextDispatcher,
  isNextReady,
  transport,
  nextSandbox,
  scheduleNextLease,
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
  useEffect(() => {
    const frame = frameRef.current
    if (!frame) return
    let port: MessagePort | undefined
    let dispatcher: ReturnType<typeof createNextDispatcher> | undefined
    let connected = false
    let active = true
    const themeController = createNextUiController()
    function shutdown(message: string) {
      if (!active) return
      active = false
      themeController.dispose()
      clearTimeout(timer)
      clearLease()
      unsubscribe()
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
    const dropEvents = [
      'cruciblebox:document-files-dropped',
      'cruciblebox:archive-files-dropped',
      'cruciblebox:plugin-files-dropped'
    ]
    dropEvents.forEach((name) => window.addEventListener(name, drop))
    const clearLease = scheduleNextLease(session, () => shutdown('插件会话已过期，请重新打开'))
    const getAppearance = () =>
      validateAppearance(
        JSON.stringify({
          mode: useThemeStore.getState().theme.mode,
          cssVars: themeToCssVars(useThemeStore.getState().theme)
        })
      )
    const unsubscribe = useThemeStore.subscribe((state, previous) => {
      if (!active || !port || state.theme === previous.theme) return
      try {
        port.postMessage({ kind: 'next-appearance', appearance: getAppearance() })
      } catch {
        shutdown('插件主题连接已关闭')
      }
    })
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
          const nativeResponse = await invoke<
            import('../../../packages/cruciblebox-next-api/src/index.mjs').Response
          >('next_renderer_request', { raw }, { headers: { Origin: window.location.origin } })
          const requestId = (JSON.parse(raw) as { requestId: string }).requestId
          const response = validateResponse(JSON.stringify(nativeResponse), requestId)
          if (!active || !response.ok) return response
          const request = JSON.parse(raw) as { method: string; params: Record<string, unknown> }
          if (
            !request.method.startsWith('theme.') &&
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
        (response) => port?.postMessage({ kind: 'next-response', response })
      )
      port.onmessageerror = () => {
        shutdown('插件连接已关闭')
      }
      port.onmessage = ({ data }: MessageEvent) => {
        if (!active) return
        if (data?.kind === 'next-mounted') {
          clearTimeout(timer)
          setReady(true)
        } else if (data?.kind === 'next-request') dispatcher?.dispatch(data.request)
        else {
          shutdown('插件连接已关闭')
        }
      }
      port.start()
      frame.contentWindow?.postMessage(
        {
          kind: 'next-connect',
          wireVersion: 3,
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
      clearLease()
      clearTimeout(timer)
      window.removeEventListener('message', listener)
      dropEvents.forEach((name) => window.removeEventListener(name, drop))
      dispatcher?.dispose()
      port?.close()
    }
  }, [session, pluginId])
  return (
    <>
      {error && <Alert type="error" message={error} />}
      <iframe
        ref={frameRef}
        title={`${name} 插件`}
        sandbox={nextSandbox(session)}
        referrerPolicy="no-referrer"
        data-plugin-ready={ready ? 'true' : 'false'}
        data-renderer-api-version="5"
        allow="camera 'none'; microphone 'none'; geolocation 'none'; payment 'none'; usb 'none'; serial 'none'; clipboard-read 'none'; clipboard-write 'none'"
        style={{
          display: 'block',
          width: '100%',
          height: 600,
          border: 0,
          background: 'transparent'
        }}
      />
    </>
  )
}

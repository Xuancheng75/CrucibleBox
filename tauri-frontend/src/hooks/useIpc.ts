// 事件订阅 hooks（1.9.3）
// 对等 Electron 版 src/hooks/useIpc.ts；订阅层换为 @tauri-apps/api/event 的 listen()。
// listen() 返回 Promise<UnlistenFn>，因此 cleanup 需要异步处理已 resolve 的 unlisten。
import { useEffect, useCallback, useRef } from 'react'
import { tauriApi } from '../api/tauriApi'

export function usePluginMessage(
  pluginId: string,
  handler: (message: unknown) => void
): void {
  const handlerRef = useRef(handler)
  handlerRef.current = handler

  const stableHandler = useCallback(
    (data: { pluginId: string; message: unknown }) => {
      if (data.pluginId === pluginId) {
        handlerRef.current(data.message)
      }
    },
    [pluginId]
  )

  useEffect(() => {
    let unlisten: (() => void) | undefined
    tauriApi.events.onMessage(stableHandler).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [stableHandler])
}

export function usePluginLog(
  handler: (data: { pluginId: string; level: string; message: string }) => void
): void {
  const handlerRef = useRef(handler)
  handlerRef.current = handler

  useEffect(() => {
    let unlisten: (() => void) | undefined
    tauriApi.events.onLog((data) => {
      handlerRef.current(data)
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])
}

export function usePluginStatusChange(
  handler: (data: { pluginId: string; status: string }) => void
): void {
  const handlerRef = useRef(handler)
  handlerRef.current = handler

  useEffect(() => {
    let unlisten: (() => void) | undefined
    tauriApi.events.onStatusChange((data) => {
      handlerRef.current(data)
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])
}
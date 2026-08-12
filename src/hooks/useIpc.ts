import { useEffect, useCallback, useRef } from 'react'

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
    const cleanup = window.electronAPI?.plugin.onMessage(stableHandler)
    return cleanup
  }, [stableHandler])
}

export function usePluginLog(
  handler: (data: { pluginId: string; level: string; message: string }) => void
): void {
  useEffect(() => {
    const cleanup = window.electronAPI?.plugin.onLog(handler)
    return cleanup
  }, [handler])
}

export function usePluginStatusChange(
  handler: (data: { pluginId: string; status: string }) => void
): void {
  useEffect(() => {
    const cleanup = window.electronAPI?.plugin.onStatusChange(handler)
    return cleanup
  }, [handler])
}

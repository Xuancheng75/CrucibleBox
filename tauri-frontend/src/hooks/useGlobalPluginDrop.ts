import { useEffect, useState } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { resolveDocumentDropPaths, resolveDropPaths } from '../utils/drop-target'
import { usePluginStore } from '../store/plugin.store'
import { useAppStore } from '../store/app.store'

export interface GlobalDropState {
  dragActive: boolean
  dragTarget: 'plugin' | 'document' | 'mixed'
}

/**
 * 全窗口拖拽导入（1.9.12）：不打开导入弹窗，把 .zip/插件目录或文档拖到窗口任意位置即可。
 *
 * - Tauri onDragDropEvent 为 OS 级事件（dragDropEnabled 默认开启），与 dnd-kit
 *   卡片排序的指针事件互不干扰
 * - drop 解析规则见 utils/drop-target.ts；安装中 / 已有待确认预览 / 队列消费中
 *   时忽略新 drop（防叠加）
 */
export function useGlobalPluginDrop(): GlobalDropState {
  const [dragActive, setDragActive] = useState(false)
  const [dragTarget, setDragTarget] = useState<GlobalDropState['dragTarget']>('plugin')

  useEffect(() => {
    let unlisten: (() => void) | undefined
    let disposed = false
    // 拖拽离开窗口/在外释放时 Tauri 不发事件：用 over 心跳超时兜底隐藏遮罩
    let hideTimer: ReturnType<typeof setTimeout> | null = null
    const bumpActive = (target?: GlobalDropState['dragTarget']) => {
      setDragActive(true)
      if (target) setDragTarget(target)
      if (hideTimer) clearTimeout(hideTimer)
      hideTimer = setTimeout(() => setDragActive(false), 1200)
    }

    getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        if (event.payload.type === 'enter' || event.payload.type === 'over') {
          const app = useAppStore.getState()
          const documentActive =
            app.currentPage === 'pluginView' && app.activePluginId === 'document-engine'
          const resolved =
            documentActive && event.payload.type === 'enter'
              ? resolveDocumentDropPaths(event.payload.paths)
              : null
          const target =
            event.payload.type === 'over'
              ? undefined
              : resolved
                ? resolved.pluginZips.length > 0 && resolved.documents.length > 0
                  ? 'mixed'
                  : resolved.pluginZips.length > 0
                    ? 'plugin'
                    : 'document'
                : documentActive
                  ? 'document'
                  : 'plugin'
          bumpActive(target)
          return
        }
        if (event.payload.type !== 'drop') return
        if (hideTimer) clearTimeout(hideTimer)
        setDragActive(false)

        const store = usePluginStore.getState()
        // 守卫：正在安装/预览未决/队列消费中 → 忽略新 drop
        if (
          store.loading ||
          store.installPreview ||
          store.queueProcessing ||
          store.batchOperationBusy
        )
          return
        if (store.installQueue.length > 0) return

        const app = useAppStore.getState()
        const documentActive =
          app.currentPage === 'pluginView' && app.activePluginId === 'document-engine'
        if (documentActive) {
          const resolved = resolveDocumentDropPaths(event.payload.paths)
          if (!resolved) return
          if (resolved.documents.length > 0) {
            window.dispatchEvent(
              new CustomEvent('cruciblebox:document-files-dropped', {
                detail: { pluginId: 'document-engine', paths: resolved.documents }
              })
            )
          }
          if (resolved.pluginZips.length > 0) {
            store.enqueueInstalls(
              resolved.pluginZips.map((path) => ({ source: 'zip' as const, path }))
            )
          }
          return
        }

        const resolved = resolveDropPaths(event.payload.paths)
        if (resolved) {
          store.enqueueInstalls(resolved.targets.map((path) => ({ source: resolved.kind, path })))
        }
      })
      .then((fn) => {
        if (disposed) fn()
        else unlisten = fn
      })

    return () => {
      disposed = true
      unlisten?.()
      if (hideTimer) clearTimeout(hideTimer)
    }
  }, [])

  return { dragActive, dragTarget }
}

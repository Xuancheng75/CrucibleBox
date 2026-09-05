import { useEffect, useState } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { resolveDocumentDropPaths, resolveDropPaths } from '../utils/drop-target'
import { shouldHandleGlobalFileDrop } from '../utils/plugin-drop'
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

    // dnd-kit 的插件排序是窗口内指针手势，不应被全局 OS 文件拖放层消费。
    // 订阅 store 让内部拖拽一开始就撤掉可能已经显示的外部拖放遮罩。
    const unsubscribeInternalDrag = usePluginStore.subscribe((state) => {
      if (!state.internalPluginDragActive) return
      if (hideTimer) clearTimeout(hideTimer)
      setDragActive(false)
    })

    getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        const store = usePluginStore.getState()
        const payload = event.payload
        const paths = 'paths' in payload ? payload.paths ?? [] : []
        // Tauri's `over` heartbeat only carries the cursor position. Keep the
        // existing overlay alive without pretending that it supplied paths.
        if (payload.type === 'over' && paths.length === 0) {
          bumpActive()
          return
        }
        if (payload.type === 'leave') {
          if (hideTimer) clearTimeout(hideTimer)
          setDragActive(false)
          return
        }
        if (!shouldHandleGlobalFileDrop(store.internalPluginDragActive, paths)) {
          if (payload.type === 'drop') {
            setDragActive(false)
          }
          return
        }
        if (payload.type === 'enter' || payload.type === 'over') {
          const app = useAppStore.getState()
          const documentActive =
            app.currentPage === 'pluginView' && app.activePluginId === 'document-engine'
          const resolved =
            documentActive && payload.type === 'enter'
              ? resolveDocumentDropPaths(paths)
              : null
          const target =
            payload.type === 'over'
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
        if (payload.type !== 'drop') return
        if (hideTimer) clearTimeout(hideTimer)
        setDragActive(false)

        // 守卫：正在安装/预览未决/队列消费中 → 忽略新 drop
        if (
          store.loading ||
          store.installPreview ||
          store.queueProcessing ||
          store.batchOperationBusy ||
          store.reorderBusy
        )
          return
        if (store.installQueue.length > 0) return

        const app = useAppStore.getState()
        const documentActive =
          app.currentPage === 'pluginView' && app.activePluginId === 'document-engine'
        if (documentActive) {
          const resolved = resolveDocumentDropPaths(paths)
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

        const resolved = resolveDropPaths(paths)
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
      unsubscribeInternalDrag()
      if (hideTimer) clearTimeout(hideTimer)
    }
  }, [])

  return { dragActive, dragTarget }
}

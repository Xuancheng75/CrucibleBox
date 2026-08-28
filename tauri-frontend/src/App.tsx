import { lazy, Suspense, useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import MainLayout from './layouts/MainLayout'
import { useAppStore } from './store/app.store'
import { usePluginStore } from './store/plugin.store'
import { ThemeProvider } from './components/ThemeProvider'
import { PageErrorBoundary } from './components/PageErrorBoundary'
import PluginDropOverlay from './components/PluginDropOverlay'
import PluginInstallPreviewModal from './components/PluginInstallPreviewModal'
import { useGlobalPluginDrop } from './hooks/useGlobalPluginDrop'
import { PluginLifecycleStatus } from '../../shared/types/plugin.types'
import { APP_PAGE_LOADERS, type AppPage } from './app-pages'
import { tauriApi } from './api/tauriApi'

const PAGE_COMPONENTS: Record<AppPage, React.LazyExoticComponent<React.ComponentType>> = {
  home: lazy(APP_PAGE_LOADERS.home),
  logs: lazy(APP_PAGE_LOADERS.logs),
  pluginView: lazy(APP_PAGE_LOADERS.pluginView),
  settings: lazy(APP_PAGE_LOADERS.settings)
}

const PAGE_NAMES: Record<AppPage, string> = {
  home: '主页',
  logs: '插件日志',
  pluginView: '插件详情',
  settings: '设置'
}

function PageLoading() {
  return (
    <div
      role="status"
      aria-live="polite"
      style={{
        display: 'grid',
        minHeight: 180,
        placeItems: 'center',
        color: 'var(--ob-color-text-secondary, #666)',
        fontSize: 13,
        letterSpacing: '0.08em'
      }}
    >
      正在加载界面…
    </div>
  )
}

export default function App() {
  const currentPage = useAppStore((s) => s.currentPage)
  const setCurrentPage = useAppStore((s) => s.setCurrentPage)
  const setPluginImportOpen = useAppStore((s) => s.setPluginImportOpen)
  // 1.9.12：全窗口拖拽导入（zip/目录）+ 全局安装确认弹窗
  const { dragActive } = useGlobalPluginDrop()

  // 菜单「导入插件」事件（Tauri 2 菜单点击经 tauri://menu 事件下发，payload 为菜单项 id）。
  // 后端菜单尚未定义（1.9.3 后端 lane 并行处理），此处按契约订阅，payload 匹配
  // 'import-plugin' 时打开导入弹窗。
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<string>('tauri://menu', (event) => {
      const menuId = event.payload
      if (menuId === 'import-plugin' || menuId === 'plugin-import') {
        setCurrentPage('home')
        setPluginImportOpen(true)
      }
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [setCurrentPage, setPluginImportOpen])

  useEffect(() => {
    const setPluginStatus = usePluginStore.getState().setPluginStatus
    let unlisten: (() => void) | undefined
    listen<{ pluginId: string; status: string }>('plugin:status-change', (event) => {
      setPluginStatus(event.payload.pluginId, event.payload.status as PluginLifecycleStatus)
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])

  // 1.9.17：剪贴板监控事件转发（宿主侧 clipboard_monitor 广播 → 插件 onMessage）
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<{ pluginId: string; text: string }>('plugin:clipboard', (event) => {
      const deliver = async () => {
        const delays = [0, 250, 1000, 2000]
        let lastError: unknown
        for (const delay of delays) {
          if (delay > 0) await new Promise((resolve) => window.setTimeout(resolve, delay))
          try {
            await tauriApi.plugin.sendMessage(event.payload.pluginId, {
              type: 'clipboard:changed',
              text: event.payload.text
            })
            return
          } catch (error) {
            lastError = error
          }
        }
        console.warn('[clipboard] failed to deliver clipboard event', lastError)
      }
      void deliver()
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<{ pluginId: string; title: string; body: string }>(
      'plugin:notification',
      (event) => {
        try {
          new Notification(event.payload.title, { body: event.payload.body })
        } catch {
          // WebView2 Notification 不可用时静默降级
        }
      }
    ).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [])

  const Page = PAGE_COMPONENTS[currentPage]

  return (
    <ThemeProvider>
      <MainLayout>
        <Suspense fallback={<PageLoading />}>
          <PageErrorBoundary key={currentPage} pageName={PAGE_NAMES[currentPage]}>
            <Page />
          </PageErrorBoundary>
        </Suspense>
      </MainLayout>
      <PluginDropOverlay active={dragActive} />
      <PluginInstallPreviewModal />
    </ThemeProvider>
  )
}

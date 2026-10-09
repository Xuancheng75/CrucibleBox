import { useState } from 'react'
import { Badge, Button, Layout, theme } from 'antd'
import { ClockCircleOutlined } from '@ant-design/icons'
import IconRail from '../components/IconRail'
import CommandPalette from '../components/CommandPalette'
import { useAppStore } from '../store/app.store'
import { useThemeStore } from '../store/theme.store'
import { useTaskStore } from '../store/task.store'
import type { AppPage } from '../app-pages'

const { Header, Sider, Content } = Layout

const PAGE_META: Record<AppPage, { title: string; hud: string }> = {
  home: { title: '工作台', hud: 'WORKBENCH' },
  marketplace: { title: '插件市场', hud: 'MARKET' },
  tasks: { title: '任务中心', hud: 'TASKS' },
  logs: { title: '插件日志', hud: 'TRACE LOG' },
  settings: { title: '设置', hud: 'SETTINGS' },
  pluginView: { title: '插件页面', hud: 'PLUGIN' }
}

interface MainLayoutProps {
  children: React.ReactNode
  appVersion: string
}

export default function MainLayout({ children, appVersion }: MainLayoutProps) {
  const { token } = theme.useToken()
  const currentPage = useAppStore((state) => state.currentPage)
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const themeName = useThemeStore((state) => state.theme.name)
  const activeTaskCount = useTaskStore(
    (state) =>
      state.tasks.filter((task) =>
        ['queued', 'running', 'paused', 'waiting-user'].includes(task.status)
      ).length
  )
  const [compact, setCompact] = useState(false)
  const railWidth = compact ? 56 : 176
  const selectedNavigationPage = currentPage === 'pluginView' ? 'home' : currentPage
  const pageMeta = PAGE_META[currentPage] ?? PAGE_META.home

  return (
    <Layout
      className="ob-app-layout"
      style={{
        height: '100dvh',
        minHeight: 0,
        overflow: 'hidden',
        background: token.colorBgLayout
      }}
    >
      <Sider
        className="ob-rail-shell"
        width={railWidth}
        collapsedWidth={56}
        style={{
          position: 'fixed',
          left: 0,
          top: 0,
          zIndex: 20,
          height: '100dvh',
          minHeight: 0,
          flexShrink: 0,
          background: token.colorPrimaryBg,
          borderRight: `1px solid ${token.colorBorder}`,
          overflow: 'hidden'
        }}
      >
        <IconRail
          appVersion={appVersion}
          selectedKey={selectedNavigationPage}
          onChange={setCurrentPage}
          compact={compact}
          onCompactChange={() => setCompact((value) => !value)}
        />
      </Sider>
      <Layout
        className="ob-app-main-layout"
        style={{
          height: '100dvh',
          minWidth: 0,
          minHeight: 0,
          overflow: 'hidden',
          marginLeft: railWidth
        }}
      >
        <Header
          className="ob-app-header"
          style={{
            height: 48,
            minHeight: 48,
            padding: '0 22px',
            display: 'flex',
            alignItems: 'center',
            gap: 18,
            background: token.colorBgContainer,
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            position: 'relative',
            zIndex: 10
          }}
        >
          <span className="cbx-header-location">{pageMeta.title}</span>
          <div style={{ flex: 1 }} />
          <Badge count={activeTaskCount} size="small">
            <Button
              type="text"
              icon={<ClockCircleOutlined />}
              aria-label="打开任务中心"
              onClick={() => setCurrentPage('tasks')}
            >
              任务
            </Button>
          </Badge>
          <span className="cbx-header-theme">
            <span aria-hidden="true" />
            {themeName}
          </span>
        </Header>
        <Content
          className="ob-main-content"
          data-hud={pageMeta.hud}
          style={{
            minHeight: 0,
            flex: 1,
            padding: '20px 22px 22px',
            overflow: currentPage === 'home' ? 'hidden' : 'auto',
            overscrollBehavior: 'contain'
          }}
        >
          <div
            className="ob-main-surface"
            style={{ height: currentPage === 'home' ? '100%' : undefined }}
          >
            {children}
          </div>
        </Content>
      </Layout>
      <CommandPalette />
    </Layout>
  )
}

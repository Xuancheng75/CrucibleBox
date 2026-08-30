import { Layout, theme } from 'antd'
import IconRail from '../components/IconRail'
import CommandPalette from '../components/CommandPalette'
import { useAppStore } from '../store/app.store'
import { useThemeStore } from '../store/theme.store'

const { Header, Sider, Content } = Layout

const PAGE_META: Record<string, { title: string; subtitle: string; hud: string }> = {
  home: { title: '工作台', subtitle: '启动与管理工作台中的工具插件', hud: 'WORKBENCH' },
  logs: { title: '插件日志', subtitle: '查看插件运行日志', hud: 'TRACE LOG' },
  settings: { title: '设置', subtitle: '应用信息与运行环境', hud: 'SYSTEM CFG' },
  pluginView: { title: '插件详情', subtitle: '插件运行界面', hud: 'PLUGIN LINK' }
}

interface MainLayoutProps {
  children: React.ReactNode
}

export default function MainLayout({ children }: MainLayoutProps) {
  const { token } = theme.useToken()
  const currentPage = useAppStore((s) => s.currentPage)
  const setCurrentPage = useAppStore((s) => s.setCurrentPage)
  const themeName = useThemeStore((s) => s.theme.name)
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
        width={72}
        style={{
          background: token.colorBgContainer,
          borderRight: `1px solid ${token.colorBorder}`,
          // Keep the rail anchored to the viewport.  `sticky` can still move
          // with an intermediate flex scrolling context in compact windows.
          position: 'fixed',
          left: 0,
          top: 0,
          height: '100dvh',
          minHeight: 0,
          zIndex: 20,
          flexShrink: 0,
          overflow: 'hidden',
          alignSelf: 'flex-start'
        }}
      >
        <IconRail selectedKey={currentPage} onChange={setCurrentPage} />
      </Sider>
      <Layout
        className="ob-app-main-layout"
        style={{
          height: '100dvh',
          minWidth: 0,
          minHeight: 0,
          overflow: 'hidden',
          marginLeft: 72
        }}
      >
        <Header
          className="ob-app-header"
          style={{
            // 1.9.13：不透明背景 + 分隔线。此前 transparent 使滚动内容穿透顶栏
            // 与标题文字重叠（cyber/neon 有 !important 背景不受影响）。
            background: token.colorBgLayout,
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            padding: '0 28px',
            display: 'flex',
            alignItems: 'center',
            gap: 20,
            height: 64,
            position: 'sticky',
            top: 0,
            zIndex: 10
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              gap: 12,
              flexShrink: 0,
              minWidth: 0
            }}
          >
            <span
              className="ob-brand-wordmark"
              style={{
                fontSize: 22,
                fontWeight: 800,
                color: token.colorPrimary,
                letterSpacing: 0.5,
                whiteSpace: 'nowrap'
              }}
            >
              CrucibleBox
            </span>
            <span
              className="ob-page-subtitle"
              style={{
                fontSize: 12,
                color: token.colorTextTertiary,
                whiteSpace: 'nowrap'
              }}
            >
              {pageMeta.subtitle}
            </span>
          </div>
          <div style={{ flex: 1 }} />
          <div
            className="ob-theme-status"
            style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}
          >
            <span
              style={{
                width: 8,
                height: 8,
                borderRadius: '50%',
                background: token.colorPrimary,
                boxShadow: `0 0 8px ${token.colorPrimary}`
              }}
            />
            <span style={{ fontSize: 13, color: token.colorTextSecondary }}>{themeName}</span>
          </div>
        </Header>
        <Content
          className="ob-main-content"
          data-hud={pageMeta.hud}
          style={{
            margin: '0 28px 28px',
            padding: 4,
            minHeight: 0,
            flex: 1,
            overflow: 'auto',
            overscrollBehavior: 'contain'
          }}
        >
          <div className="ob-hud-strip" aria-hidden="true">
            <span>CRUCIBLEBOX // {pageMeta.hud}</span>
            <span>LINK STABLE · NODE 01</span>
          </div>
          <div className="ob-main-surface">{children}</div>
        </Content>
      </Layout>
      <CommandPalette />
    </Layout>
  )
}

import { useMemo, useState } from 'react'
import { Button, Drawer, Empty, Input, Space, Tag, Typography, theme } from 'antd'
import {
  CheckOutlined,
  DownloadOutlined,
  SearchOutlined,
  SafetyCertificateOutlined
} from '@ant-design/icons'
import { OFFICIAL_MARKETPLACE_CATALOG, type MarketplacePlugin } from '../marketplace-catalog'
import { pluginIdentity } from '../plugin-identity'
import PluginGlyph from '../components/PluginGlyph'
import { usePluginStore } from '../store/plugin.store'
import { useAppStore } from '../store/app.store'
import { tauriApi } from '../api/tauriApi'
import { useTaskStore } from '../store/task.store'

const { Title, Text, Paragraph } = Typography

export default function Marketplace() {
  const { token } = theme.useToken()
  const plugins = usePluginStore((state) => state.plugins)
  const setActivePluginId = useAppStore((state) => state.setActivePluginId)
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const setPluginImportOpen = useAppStore((state) => state.setPluginImportOpen)
  const installPlugin = usePluginStore((state) => state.installPlugin)
  const [downloadingId, setDownloadingId] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<MarketplacePlugin | null>(null)

  const installedByName = useMemo(
    () => new Map(plugins.map((plugin) => [plugin.name, plugin])),
    [plugins]
  )
  const catalog = useMemo(() => {
    const normalized = query.trim().toLowerCase()
    if (!normalized) return OFFICIAL_MARKETPLACE_CATALOG
    return OFFICIAL_MARKETPLACE_CATALOG.filter((plugin) =>
      `${plugin.name} ${plugin.id} ${plugin.category} ${plugin.description}`
        .toLowerCase()
        .includes(normalized)
    )
  }, [query])

  const openInstalled = (plugin: MarketplacePlugin) => {
    const installed = installedByName.get(plugin.id)
    if (!installed) return
    setActivePluginId(installed.id)
    setCurrentPage('pluginView')
  }

  const requestInstall = async (plugin: MarketplacePlugin) => {
    const taskId = `marketplace-${plugin.id}-${Date.now()}`
    setDownloadingId(plugin.id)
    useTaskStore.getState().upsertTask({ id: taskId, title: `下载 ${plugin.name}`, source: 'marketplace', status: 'running', progress: 10 })
    try {
      const path = await tauriApi.plugin.marketplaceDownload(plugin.id)
      useTaskStore.getState().patchTask(taskId, { title: `校验 ${plugin.name}`, progress: 70 })
      const prepared = await installPlugin('zip', path)
      useTaskStore.getState().patchTask(taskId, prepared ? { status: 'completed', progress: 100 } : { status: 'failed', error: '插件安装预检失败' })
    } catch (error) {
      useTaskStore.getState().patchTask(taskId, { status: 'failed', error: error instanceof Error ? error.message : String(error) })
      setCurrentPage('home')
      setPluginImportOpen(true)
    } finally {
      setDownloadingId(null)
    }
  }

  return (
    <div className="ob-marketplace-page">
      <div className="ob-page-heading">
        <div>
          <Title level={3} style={{ margin: 0 }}>
            插件市场
          </Title>
          <Text type="secondary">官方目录已就绪，远程市场服务保持可替换</Text>
        </div>
        <Tag icon={<SafetyCertificateOutlined />} color="blue">
          CrucibleBox 官方目录
        </Tag>
      </div>

      <Input
        allowClear
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        prefix={<SearchOutlined />}
        placeholder="按名称、分类或功能搜索插件"
        aria-label="搜索插件市场"
        size="large"
        style={{ marginBottom: 20 }}
      />

      {catalog.length === 0 ? (
        <Empty description="没有找到匹配的插件" />
      ) : (
        <div className="ob-market-grid" role="list" aria-label="插件市场目录">
          {catalog.map((plugin) => {
            const installed = installedByName.get(plugin.id)
            const identity = pluginIdentity(plugin.id)
            return (
              <article
                key={plugin.id}
                role="listitem"
                className="ob-market-card ob-surface-card"
                onClick={() => setSelected(plugin)}
                style={{
                  border: `1px solid ${token.colorBorder}`,
                  borderRadius: token.borderRadius,
                  background: token.colorBgContainer
                }}
              >
                <PluginGlyph pluginId={plugin.id} name={plugin.name} size={52} />
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="ob-market-card-title">
                    <span>{plugin.name}</span>
                    <span style={{ color: token.colorTextTertiary, fontSize: 11 }}>
                      v{installed?.version ?? plugin.version}
                    </span>
                  </div>
                  <div style={{ color: token.colorTextTertiary, fontSize: 11, marginTop: 2 }}>
                    {plugin.publisher} · {identity.category}
                  </div>
                  <Paragraph
                    ellipsis={{ rows: 2 }}
                    style={{ color: token.colorTextSecondary, fontSize: 12, margin: '8px 0 10px' }}
                  >
                    {plugin.description}
                  </Paragraph>
                  <Button
                    size="small"
                    type={installed ? 'default' : 'primary'}
                    loading={downloadingId === plugin.id}
                    icon={installed ? <CheckOutlined /> : <DownloadOutlined />}
                    onClick={(event) => {
                      event.stopPropagation()
                      if (installed) openInstalled(plugin)
                      else void requestInstall(plugin)
                    }}
                  >
                    {installed ? '已安装' : '获取'}
                  </Button>
                </div>
              </article>
            )
          })}
        </div>
      )}

      <Drawer
        width={520}
        open={selected !== null}
        onClose={() => setSelected(null)}
        title={selected?.name}
      >
        {selected && (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
              <PluginGlyph pluginId={selected.id} name={selected.name} size={68} />
              <div>
                <Title level={4} style={{ margin: 0 }}>
                  {selected.name}
                </Title>
                <Text type="secondary">
                  {selected.publisher} · v{selected.version}
                </Text>
              </div>
            </div>
            <Paragraph>{selected.description}</Paragraph>
            <div>
              <Text strong>主要能力</Text>
              <ul style={{ marginTop: 8, paddingLeft: 20 }}>
                {selected.highlights.map((highlight) => (
                  <li key={highlight}>{highlight}</li>
                ))}
              </ul>
            </div>
            <Button
              type="primary"
              block
              loading={downloadingId === selected.id}
              icon={
                installedByName.has(selected.id) ? <CheckOutlined /> : <DownloadOutlined />
              }
              onClick={() =>
                installedByName.has(selected.id) ? openInstalled(selected) : void requestInstall(selected)
              }
            >
              {installedByName.has(selected.id) ? '打开已安装插件' : '选择插件包安装'}
            </Button>
          </Space>
        )}
      </Drawer>
    </div>
  )
}

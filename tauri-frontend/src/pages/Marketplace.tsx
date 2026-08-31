import { useEffect, useMemo, useState } from 'react'
import { Alert, Button, Drawer, Empty, Input, Space, Tag, Typography, theme } from 'antd'
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

function compareVersions(left: string, right: string): number {
  const parse = (value: string) => value.replace(/^v/i, '').split(/[.-]/).map((part) => Number.parseInt(part, 10) || 0)
  const a = parse(left)
  const b = parse(right)
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const diff = (a[index] ?? 0) - (b[index] ?? 0)
    if (diff !== 0) return diff
  }
  return 0
}

export default function Marketplace() {
  const { token } = theme.useToken()
  const plugins = usePluginStore((state) => state.plugins)
  const setActivePluginId = useAppStore((state) => state.setActivePluginId)
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const installPlugin = usePluginStore((state) => state.installPlugin)
  const [downloadingId, setDownloadingId] = useState<string | null>(null)
  const [downloadError, setDownloadError] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<MarketplacePlugin | null>(null)
  const [remoteCatalog, setRemoteCatalog] = useState<MarketplacePlugin[] | null>(null)

  useEffect(() => {
    let active = true
    void tauriApi.plugin.marketplaceCatalog().then((payload) => {
      if (!active || !payload || typeof payload !== 'object') return
      const entries = Array.isArray((payload as { plugins?: unknown }).plugins)
        ? (payload as { plugins: Array<Record<string, unknown>> }).plugins
        : []
      const byId = new Map(OFFICIAL_MARKETPLACE_CATALOG.map((plugin) => [plugin.id, plugin]))
      const merged = entries
        .map((entry) => {
          const base = byId.get(String(entry.id))
          if (!base || typeof entry.version !== 'string') return null
          return {
            ...base,
            version: entry.version,
            ...(typeof entry.artifact === 'string' ? { artifact: entry.artifact } : {}),
            ...(typeof entry.sha256 === 'string' ? { sha256: entry.sha256 } : {}),
            ...(typeof entry.size === 'number' ? { size: entry.size } : {}),
            ...(typeof entry.url === 'string' ? { url: entry.url } : {}),
            minHostVersion:
              typeof entry.minHostVersion === 'string'
                ? entry.minHostVersion
                : typeof entry.min_host_version === 'string'
                  ? entry.min_host_version
                  : base.minHostVersion
          } as MarketplacePlugin
        })
        .filter((plugin): plugin is MarketplacePlugin => plugin !== null)
      if (merged.length > 0) setRemoteCatalog(merged)
    }).catch(() => undefined)
    return () => {
      active = false
    }
  }, [])

  const installedById = useMemo(
    () => new Map(plugins.map((plugin) => [plugin.id, plugin])),
    [plugins]
  )
  const catalog = useMemo(() => {
    const source = remoteCatalog ?? OFFICIAL_MARKETPLACE_CATALOG
    const normalized = query.trim().toLowerCase()
    if (!normalized) return source
    return source.filter((plugin) =>
      `${plugin.name} ${plugin.id} ${plugin.category} ${plugin.description}`
        .toLowerCase()
        .includes(normalized)
    )
  }, [query, remoteCatalog])

  const openInstalled = (plugin: MarketplacePlugin) => {
    const installed = installedById.get(plugin.id)
    if (!installed) return
    setActivePluginId(installed.id)
    setCurrentPage('pluginView')
  }

  const requestInstall = async (plugin: MarketplacePlugin) => {
    setDownloadError(null)
    const taskId = `marketplace-${plugin.id}-${Date.now()}`
    setDownloadingId(plugin.id)
    useTaskStore.getState().upsertTask({ id: taskId, title: `下载 ${plugin.name}`, source: 'marketplace', status: 'running', progress: 10 })
    try {
      const path = await tauriApi.plugin.marketplaceDownload(plugin.id)
      useTaskStore.getState().patchTask(taskId, { title: `校验 ${plugin.name}`, progress: 70 })
      const prepared = await installPlugin('zip', path)
      useTaskStore.getState().patchTask(taskId, prepared ? { status: 'completed', progress: 100 } : { status: 'failed', error: '插件安装预检失败' })
      if (!prepared) setDownloadError('插件安装预检失败，请查看任务中心后重试。')
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      useTaskStore.getState().patchTask(taskId, { status: 'failed', error: message })
      setDownloadError(message)
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

      {downloadError && (
        <Alert
          type="error"
          showIcon
          closable
          message="插件下载失败"
          description={downloadError}
          onClose={() => setDownloadError(null)}
          style={{ marginBottom: 20 }}
        />
      )}

      {catalog.length === 0 ? (
        <Empty description="没有找到匹配的插件" />
      ) : (
        <div className="ob-market-grid" role="list" aria-label="插件市场目录">
          {catalog.map((plugin) => {
            const installed = installedById.get(plugin.id)
            const identity = pluginIdentity(plugin.id)
            const installedVersion = installed?.version
            const updateAvailable = Boolean(installedVersion && compareVersions(plugin.version, installedVersion) > 0)
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
                      v{plugin.version}
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
                    type={installed && !updateAvailable ? 'default' : 'primary'}
                    loading={downloadingId === plugin.id}
                    icon={installed && !updateAvailable ? <CheckOutlined /> : <DownloadOutlined />}
                    onClick={(event) => {
                      event.stopPropagation()
                      if (installed && !updateAvailable) openInstalled(plugin)
                      else void requestInstall(plugin)
                    }}
                  >
                    {installed && !updateAvailable ? '打开' : installed ? '更新' : '获取'}
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
                  {installedById.has(selected.id) ? ` · 已安装 v${installedById.get(selected.id)?.version}` : ''}
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
              icon={installedById.has(selected.id) && compareVersions(selected.version, installedById.get(selected.id)?.version ?? '0.0.0') <= 0 ? <CheckOutlined /> : <DownloadOutlined />}
              onClick={() =>
                installedById.has(selected.id) && compareVersions(selected.version, installedById.get(selected.id)?.version ?? '0.0.0') <= 0
                  ? openInstalled(selected)
                  : void requestInstall(selected)
              }
            >
              {installedById.has(selected.id) && compareVersions(selected.version, installedById.get(selected.id)?.version ?? '0.0.0') <= 0 ? '打开' : installedById.has(selected.id) ? '更新' : '获取'}
            </Button>
          </Space>
        )}
      </Drawer>
    </div>
  )
}

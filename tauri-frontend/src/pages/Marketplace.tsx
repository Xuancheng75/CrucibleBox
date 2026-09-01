import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Button, Drawer, Empty, Input, Progress, Space, Tag, Typography, theme } from 'antd'
import {
  CheckOutlined,
  DownloadOutlined,
  ReloadOutlined,
  SearchOutlined,
  SafetyCertificateOutlined
} from '@ant-design/icons'
import { OFFICIAL_MARKETPLACE_CATALOG, type MarketplacePlugin } from '../marketplace-catalog'
import { pluginIdentity } from '../plugin-identity'
import PluginGlyph from '../components/PluginGlyph'
import { usePluginStore } from '../store/plugin.store'
import { useAppStore } from '../store/app.store'
import { tauriApi, type MarketplaceCatalogResponse } from '../api/tauriApi'
import { useTaskStore } from '../store/task.store'

const { Title, Text, Paragraph } = Typography

function compareVersions(left: string, right: string): number {
  const parse = (value: string) => {
    const match = value.replace(/^v/i, '').match(/^(\d+)\.(\d+)\.(\d+)(?:-([^+]+))?/)
    if (!match) return { core: [0, 0, 0], pre: [] as string[] }
    return { core: [Number(match[1]), Number(match[2]), Number(match[3])], pre: match[4]?.split('.') ?? [] }
  }
  const a = parse(left)
  const b = parse(right)
  for (let index = 0; index < 3; index += 1) {
    if (a.core[index] !== b.core[index]) return a.core[index] - b.core[index]
  }
  if (a.pre.length === 0 && b.pre.length === 0) return 0
  if (a.pre.length === 0) return 1
  if (b.pre.length === 0) return -1
  for (let index = 0; index < Math.max(a.pre.length, b.pre.length); index += 1) {
    const av = a.pre[index]
    const bv = b.pre[index]
    if (av === undefined) return -1
    if (bv === undefined) return 1
    if (av === bv) continue
    const an = /^\d+$/.test(av)
    const bn = /^\d+$/.test(bv)
    if (an && bn) return Number(av) - Number(bv)
    if (an !== bn) return an ? -1 : 1
    return av < bv ? -1 : 1
  }
  return 0
}

export default function Marketplace() {
  const { token } = theme.useToken()
  const plugins = usePluginStore((state) => state.plugins)
  const setActivePluginId = useAppStore((state) => state.setActivePluginId)
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const installPlugin = usePluginStore((state) => state.installPlugin)
  const fetchPlugins = usePluginStore((state) => state.fetchPlugins)
  const [downloadingId, setDownloadingId] = useState<string | null>(null)
  const [downloadError, setDownloadError] = useState<string | null>(null)
  const [catalogError, setCatalogError] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<MarketplacePlugin | null>(null)
  const [remoteCatalog, setRemoteCatalog] = useState<MarketplacePlugin[] | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [channel, setChannel] = useState<'stable' | 'beta'>('stable')
  const [channelReady, setChannelReady] = useState(false)
  const [catalogSource, setCatalogSource] = useState('内置目录')
  const [catalogStale, setCatalogStale] = useState(false)
  const [lastCheckedAt, setLastCheckedAt] = useState<number | null>(null)
  const [newPluginCount, setNewPluginCount] = useState(0)
  const [updateCount, setUpdateCount] = useState(0)
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({})
  const activeDownloadTaskRef = useRef<string | null>(null)

  useEffect(() => {
    let active = true
    void tauriApi.settings.get('updateChannel').then((value) => {
      if (!active) return
      if (value === 'beta' || value === 'stable') setChannel(value)
      setChannelReady(true)
    }).catch(() => {
      if (active) setChannelReady(true)
    })
    return () => {
      active = false
    }
  }, [])

  const loadCatalog = useCallback(async (forceRefresh: boolean) => {
    setRefreshing(true)
    setCatalogError(null)
    try {
      const payload: MarketplaceCatalogResponse = await tauriApi.plugin.marketplaceCatalog(
        forceRefresh,
        channel
      )
      const byId = new Map(OFFICIAL_MARKETPLACE_CATALOG.map((plugin) => [plugin.id, plugin]))
      const knownIds = new Set(byId.keys())
      const merged = payload.plugins
        .map((entry) => {
          const base = byId.get(String(entry.id))
          if (!entry.id || typeof entry.version !== 'string') return null
          if (!base && !entry.displayName) return null
          const identity = pluginIdentity(entry.id, entry.publisher)
          const highlights = Array.isArray(entry.highlights)
            ? entry.highlights.filter((item): item is string => typeof item === 'string')
            : base?.highlights ?? []
          return {
            ...(base ?? {
              id: entry.id,
              name: entry.displayName ?? entry.id,
              version: entry.version,
              publisher: entry.publisher ?? 'CrucibleBox',
              category: entry.category ?? identity.category,
              description: entry.description ?? '',
              highlights
            }),
            id: entry.id,
            name: entry.displayName ?? base?.name ?? entry.id,
            version: entry.version,
            publisher: entry.publisher ?? base?.publisher ?? 'CrucibleBox',
            category: entry.category ?? base?.category ?? identity.category,
            description: entry.description ?? base?.description ?? '',
            highlights,
            ...(entry.artifact ? { artifact: entry.artifact } : {}),
            ...(entry.sha256 ? { sha256: entry.sha256 } : {}),
            ...(typeof entry.size === 'number' ? { size: entry.size } : {}),
            ...(entry.url ? { url: entry.url } : {}),
            ...(entry.icon ? { icon: entry.icon } : {}),
            ...(entry.minHostVersion ? { minHostVersion: entry.minHostVersion } : {})
          } as MarketplacePlugin
        })
        .filter((plugin): plugin is MarketplacePlugin => plugin !== null)
      setRemoteCatalog(merged)
      setCatalogSource(payload.source)
      setCatalogStale(payload.stale)
      setLastCheckedAt(payload.fetchedAt ? payload.fetchedAt * 1000 : Date.now())
      await fetchPlugins()
      const installed = usePluginStore.getState().plugins
      setNewPluginCount(merged.filter((plugin) => !knownIds.has(plugin.id)).length)
      setUpdateCount(
        merged.filter((plugin) => {
          const installedVersion = installed.find((item) => item.id === plugin.id)?.version
          return Boolean(installedVersion && compareVersions(plugin.version, installedVersion) > 0)
        }).length
      )
    } catch (error) {
      setCatalogError(error instanceof Error ? error.message : String(error))
    } finally {
      setRefreshing(false)
    }
  }, [channel, fetchPlugins])

  useEffect(() => {
    if (channelReady) void loadCatalog(false)
  }, [channelReady, loadCatalog])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void tauriApi.events.onMarketplaceProgress((payload) => {
      const plugin = (remoteCatalog ?? OFFICIAL_MARKETPLACE_CATALOG).find(
        (item) => item.artifact === payload.artifact
      )
      if (!plugin) return
      const percent = payload.total > 0 ? Math.round((payload.downloaded / payload.total) * 100) : 0
      setDownloadProgress((current) => ({ ...current, [plugin.id]: percent }))
      if (activeDownloadTaskRef.current) {
        useTaskStore.getState().patchTask(activeDownloadTaskRef.current, {
          progress: Math.max(10, Math.min(95, percent))
        })
      }
    }).then((stop) => {
      if (disposed) stop()
      else unlisten = stop
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [remoteCatalog])

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
    activeDownloadTaskRef.current = taskId
    setDownloadProgress((current) => ({ ...current, [plugin.id]: 0 }))
    setDownloadingId(plugin.id)
    useTaskStore.getState().upsertTask({ id: taskId, title: `下载 ${plugin.name}`, source: 'marketplace', status: 'running', progress: 10 })
    try {
      const path = await tauriApi.plugin.marketplaceDownload(plugin.id, channel)
      useTaskStore.getState().patchTask(taskId, { title: `校验 ${plugin.name}`, progress: 70 })
      const prepared = await installPlugin('zip', path)
      const installError = usePluginStore.getState().error
      useTaskStore.getState().patchTask(taskId, prepared
        ? { status: 'completed', progress: 100, detail: '下载和校验完成，等待安装确认' }
        : { status: 'failed', error: installError ?? '插件安装预检失败' })
      if (!prepared) setDownloadError(installError ?? '插件安装预检失败，请查看任务中心后重试。')
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      useTaskStore.getState().patchTask(taskId, { status: 'failed', error: message })
      setDownloadError(message)
    } finally {
      setDownloadingId(null)
      activeDownloadTaskRef.current = null
    }
  }

  return (
    <div className="ob-marketplace-page">
      <div className="ob-page-heading">
        <div>
          <Title level={3} style={{ margin: 0 }}>
            插件市场
          </Title>
          <Text type="secondary">
            {catalogStale ? '当前使用最近一次可用目录' : `通道：${channel === 'beta' ? '测试版' : '稳定版'}`}
            {lastCheckedAt ? ` · ${new Date(lastCheckedAt).toLocaleTimeString()}` : ''}
          </Text>
        </div>
        <Space>
          {(newPluginCount > 0 || updateCount > 0) && (
            <Tag color="orange">新增 {newPluginCount} · 可更新 {updateCount}</Tag>
          )}
          <Tag icon={<SafetyCertificateOutlined />} color={catalogStale ? 'gold' : 'blue'}>
            {catalogStale
              ? '缓存目录'
              : catalogSource.includes('tauri-beta')
                ? 'CrucibleBox 测试版目录'
                : 'CrucibleBox 官方目录'}
          </Tag>
          <Button
            icon={<ReloadOutlined />}
            loading={refreshing}
            onClick={() => void loadCatalog(true)}
          >
            刷新
          </Button>
        </Space>
      </div>

      {catalogError && (
        <Alert
          type="warning"
          showIcon
          message="插件目录刷新失败"
          description={`${catalogError}。当前仍显示可用的本地目录。`}
          closable
          onClose={() => setCatalogError(null)}
          style={{ marginBottom: 16 }}
        />
      )}

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
            const progress = downloadProgress[plugin.id]
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
                  {(updateAvailable || (!installed && newPluginCount > 0 && !OFFICIAL_MARKETPLACE_CATALOG.some((item) => item.id === plugin.id))) && (
                    <Tag color={updateAvailable ? 'orange' : 'green'} style={{ marginTop: 4 }}>
                      {updateAvailable ? '可更新' : '新增'}
                    </Tag>
                  )}
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
                  {downloadingId === plugin.id && typeof progress === 'number' && (
                    <Progress percent={progress} size="small" showInfo style={{ marginTop: 8 }} />
                  )}
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

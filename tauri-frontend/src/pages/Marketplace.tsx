import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Alert,
  Button,
  Checkbox,
  Drawer,
  Dropdown,
  Empty,
  Input,
  Modal,
  Progress,
  Select,
  Space,
  Tag,
  Typography,
  theme
} from 'antd'
import { CheckOutlined, DownloadOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { marketplaceEntry, type MarketplacePlugin } from '../marketplace-catalog'
import { searchPlugins } from '../plugin-search'
import { usePluginStore } from '../store/plugin.store'
import { useAppStore } from '../store/app.store'
import { cardVariants } from '../theme/card-variant'
import {
  tauriApi,
  type MarketplaceCatalogResponse,
  type PluginMarketplaceOrigin
} from '../api/tauriApi'
import { useTaskStore } from '../store/task.store'
import { compareVersions } from '../features/marketplace/selectors/compareVersions'

const { Title, Text, Paragraph } = Typography

export default function Marketplace() {
  const { token } = theme.useToken()
  const plugins = usePluginStore((state) => state.plugins)
  const setActivePluginId = useAppStore((state) => state.setActivePluginId)
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const enqueueInstalls = usePluginStore((state) => state.enqueueInstalls)
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
  const [origins, setOrigins] = useState<PluginMarketplaceOrigin[]>([])
  const [catalogStale, setCatalogStale] = useState(false)

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [selectionMode, setSelectionMode] = useState(false)
  const [batchBusy, setBatchBusy] = useState(false)
  const [batchResult, setBatchResult] = useState<string | null>(null)
  const activeDownloadTaskRef = useRef<string | null>(null)
  const catalogGeneration = useRef(0)
  const tasks = useTaskStore((state) => state.tasks)

  useEffect(() => {
    let active = true
    void Promise.all([tauriApi.settings.get('marketplaceChannel'), tauriApi.app.getVersion()])
      .then(([value, version]) => {
        if (!active) return
        setChannel(
          value === 'beta' || (!value && /-(beta|rc)\./i.test(version)) ? 'beta' : 'stable'
        )
        setChannelReady(true)
      })
      .catch(() => {
        if (active) setChannelReady(true)
      })
    return () => {
      active = false
    }
  }, [])

  const loadCatalog = useCallback(
    async (forceRefresh: boolean) => {
      const generation = ++catalogGeneration.current
      setRefreshing(true)
      setCatalogError(null)
      try {
        const payload: MarketplaceCatalogResponse = await tauriApi.plugin.marketplaceCatalog(
          forceRefresh,
          channel
        )
        if (generation !== catalogGeneration.current) return
        const merged = payload.plugins.map(marketplaceEntry)
        setRemoteCatalog(merged)
        setCatalogStale(payload.stale)

        await fetchPlugins()
        const nextOrigins = await tauriApi.plugin.marketplaceOrigins()
        if (generation !== catalogGeneration.current) return
        setOrigins(nextOrigins)
      } catch (error) {
        if (generation === catalogGeneration.current)
          setCatalogError(error instanceof Error ? error.message : String(error))
      } finally {
        if (generation === catalogGeneration.current) setRefreshing(false)
      }
    },
    [channel, fetchPlugins]
  )

  useEffect(() => {
    if (channelReady) void loadCatalog(false)
  }, [channelReady, loadCatalog])

  const installedByName = useMemo(
    () => new Map(plugins.map((plugin) => [plugin.name, plugin])),
    [plugins]
  )
  const marketplaceItems = useMemo(() => remoteCatalog ?? [], [remoteCatalog])
  const variants = useMemo(
    () => cardVariants(marketplaceItems.map((plugin) => plugin.id)),
    [marketplaceItems]
  )
  const catalog = useMemo(
    () =>
      searchPlugins(marketplaceItems, query, (plugin) => ({
        name: plugin.name,
        id: plugin.id,
        keywords: [...plugin.highlights, ...(plugin.keywords ?? [])],
        description: plugin.description
      })),
    [marketplaceItems, query]
  )

  const updatePlugins = useMemo(
    () =>
      marketplaceItems.filter((plugin) => {
        const installedVersion = installedByName.get(plugin.id)?.version
        return Boolean(installedVersion && compareVersions(plugin.version, installedVersion) > 0)
      }),
    [installedByName, marketplaceItems]
  )

  const selectedPlugins = useMemo(
    () => catalog.filter((plugin) => selectedIds.has(plugin.id)),
    [catalog, selectedIds]
  )
  const installableSelected = selectedPlugins.filter((plugin) => {
    const installedVersion = installedByName.get(plugin.id)?.version
    return !installedVersion || compareVersions(plugin.version, installedVersion) > 0
  })

  const openInstalled = (plugin: MarketplacePlugin) => {
    const installed = installedByName.get(plugin.id)
    if (!installed) return
    setSelected(null)
    setActivePluginId(installed.id)
    setCurrentPage('pluginView')
  }

  const requestInstall = async (plugin: MarketplacePlugin) => {
    if (batchBusy || downloadingId !== null) return
    const installed = installedByName.get(plugin.id)
    const origin = origins.find((item) => item.pluginId === installed?.id)
    if (installed && (origin?.sourceId ?? 0) !== 0) {
      const confirmed = await new Promise<boolean>((resolve) =>
        Modal.confirm({
          title: '确认更换插件来源',
          content: `${plugin.name} 已从另一目录安装。改用当前目录的版本可能改变插件内容与权限，确定继续吗？`,
          onOk: () => resolve(true),
          onCancel: () => resolve(false)
        })
      )
      if (!confirmed) return
    }
    setDownloadError(null)
    const taskId = `marketplace-${plugin.id}-${Date.now()}`

    activeDownloadTaskRef.current = taskId
    setDownloadingId(plugin.id)
    try {
      const path = await tauriApi.plugin.marketplaceDownload(
        plugin.id,
        channel,
        'foreground',
        taskId
      )
      activeDownloadTaskRef.current = null
      const phaseTaskId = `${taskId}:install`
      useTaskStore.getState().upsertTask({
        id: phaseTaskId,
        source: 'marketplace',
        title: `安装 ${plugin.name}`,
        status: 'queued',
        progress: 70,
        detail: '已下载，等待进入安装确认'
      })
      enqueueInstalls([
        {
          source: 'zip',
          path,
          taskId: phaseTaskId,
          title: `安装 ${plugin.name}`,
          marketplaceSourceId: 0
        }
      ])
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      const cancelled = message.includes('CANCELLED')
      if (!cancelled) setDownloadError(message)
    } finally {
      setDownloadingId(null)
      activeDownloadTaskRef.current = null
    }
  }

  const toggleSelected = (pluginId: string, checked: boolean) => {
    setSelectedIds((current) => {
      const next = new Set(current)
      if (checked) next.add(pluginId)
      else next.delete(pluginId)
      return next
    })
  }

  const allVisibleSelected =
    catalog.length > 0 && catalog.every((plugin) => selectedIds.has(plugin.id))
  const someVisibleSelected = catalog.some((plugin) => selectedIds.has(plugin.id))
  const toggleAllVisible = (checked: boolean) => {
    setSelectedIds((current) => {
      const next = new Set(current)
      for (const plugin of catalog) {
        if (checked) next.add(plugin.id)
        else next.delete(plugin.id)
      }
      return next
    })
  }

  const downloadBatch = async (items: MarketplacePlugin[], action: 'download' | 'update') => {
    const installable = items.filter((plugin) => {
      const installedVersion = installedByName.get(plugin.id)?.version
      return !installedVersion || compareVersions(plugin.version, installedVersion) > 0
    })
    if (installable.length === 0 || batchBusy || downloadingId !== null) return
    const differentSource = installable.filter(
      (plugin) =>
        installedByName.has(plugin.id) &&
        (origins.find((origin) => origin.pluginId === installedByName.get(plugin.id)?.id)
          ?.sourceId ?? 0) !== 0
    )
    if (differentSource.length > 0) {
      setDownloadError(
        `以下插件已从其他目录安装，请逐个确认是否更换来源：${differentSource.map((plugin) => plugin.name).join('、')}`
      )
      return
    }
    setBatchBusy(true)
    setBatchResult(null)
    setDownloadError(null)
    const downloads: Array<{ path: string; taskId: string; plugin: MarketplacePlugin }> = []
    const failures: string[] = []
    for (const plugin of installable) {
      const taskId = `marketplace-${action}-${plugin.id}-${Date.now()}`
      activeDownloadTaskRef.current = taskId
      setDownloadingId(plugin.id)
      try {
        const path = await tauriApi.plugin.marketplaceDownload(plugin.id, channel, 'normal', taskId)
        const installTaskId = `${taskId}:install`
        downloads.push({ path, taskId: installTaskId, plugin })
        useTaskStore.getState().upsertTask({
          id: installTaskId,
          source: 'marketplace',
          title: `安装 ${plugin.name}`,
          status: 'queued',
          progress: 70,
          detail: '已下载，等待进入安装确认'
        })
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        const cancelled = message.includes('CANCELLED')
        if (!cancelled) failures.push(`${plugin.name}: ${message}`)
      }
    }
    setDownloadingId(null)
    activeDownloadTaskRef.current = null
    if (downloads.length > 0) {
      usePluginStore.getState().enqueueInstalls(
        downloads.map(({ path, taskId, plugin }) => ({
          source: 'zip' as const,
          path,
          taskId,
          title: `安装 ${plugin.name}`,
          marketplaceSourceId: 0
        }))
      )
    }
    if (failures.length > 0) {
      setBatchResult(
        `已下载 ${downloads.length}/${installable.length} 个；失败 ${failures.length} 个。`
      )
      setDownloadError(failures.join('\n'))
    } else {
      setBatchResult(`已下载 ${downloads.length} 个插件，正在逐个等待安装确认。`)
    }
    setSelectedIds(new Set())
    setBatchBusy(false)
  }

  return (
    <div className="ob-marketplace-page">
      <div className="ob-page-heading">
        <div>
          <Title level={3} style={{ margin: 0 }}>
            插件市场
          </Title>
        </div>
        <div className="cbx-market-actions">
          <Select<'stable' | 'beta'>
            aria-label="插件市场通道"
            value={channel}
            disabled={!channelReady || batchBusy || downloadingId !== null}
            options={[
              { value: 'stable', label: '稳定版' },
              { value: 'beta', label: '测试版' }
            ]}
            onChange={(next) => {
              ++catalogGeneration.current
              setChannel(next)
              setRemoteCatalog(null)
              setSelected(null)
              setSelectionMode(false)
              setSelectedIds(new Set())
              setDownloadError(null)
              setBatchResult(null)
              void tauriApi.settings
                .set('marketplaceChannel', next)
                .catch((error) => setCatalogError(String(error)))
            }}
          />
          <Button
            disabled={batchBusy || downloadingId !== null}
            onClick={() => {
              setSelectionMode(!selectionMode)
              setSelectedIds(new Set())
            }}
          >
            {selectionMode ? '完成管理' : '批量管理'}
          </Button>
          <Button
            icon={<ReloadOutlined />}
            loading={refreshing}
            disabled={!channelReady || batchBusy || downloadingId !== null}
            onClick={() => void loadCatalog(true)}
          >
            刷新
          </Button>
        </div>
      </div>
      {catalogStale && (
        <Alert
          type="warning"
          showIcon
          message="当前显示最近一次获取的官方目录"
          style={{ marginBottom: 16 }}
        />
      )}
      <Input
        prefix={<SearchOutlined />}
        allowClear
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="搜索插件名称或功能…"
        aria-label="搜索插件市场"
        style={{ marginBottom: 16 }}
      />
      {selectionMode && (
        <div className="cbx-market-selection">
          <Checkbox
            checked={allVisibleSelected}
            indeterminate={!allVisibleSelected && someVisibleSelected}
            disabled={batchBusy || catalog.length === 0}
            onChange={(event) => toggleAllVisible(event.target.checked)}
          >
            {allVisibleSelected ? '取消全选' : '全选'}
          </Checkbox>
          <Text type="secondary">已选 {selectedIds.size} 项</Text>
          <Button
            type="primary"
            disabled={
              batchBusy || downloadingId !== null || refreshing || installableSelected.length === 0
            }
            loading={batchBusy}
            icon={<DownloadOutlined />}
            onClick={() => void downloadBatch(installableSelected, 'download')}
          >
            安装或更新 ({installableSelected.length})
          </Button>
          {updatePlugins.length > 0 && (
            <Button
              disabled={batchBusy || downloadingId !== null || refreshing}
              onClick={() => void downloadBatch(updatePlugins, 'update')}
            >
              全部更新 ({updatePlugins.length})
            </Button>
          )}
        </div>
      )}
      {catalogError && (
        <Alert
          type="warning"
          showIcon
          message="插件目录刷新失败"
          description={`${catalogError}。请刷新重试。`}
          closable
          onClose={() => setCatalogError(null)}
          style={{ marginBottom: 16 }}
        />
      )}

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

      {batchResult && (
        <Alert
          type="info"
          showIcon
          closable
          message="批量任务"
          description={batchResult}
          onClose={() => setBatchResult(null)}
          style={{ marginBottom: 20 }}
        />
      )}

      {catalog.length === 0 ? (
        <Empty description="没有找到匹配的插件" />
      ) : (
        <div className="ob-market-grid" role="list" aria-label="插件市场目录">
          {catalog.map((plugin) => {
            const installed = installedByName.get(plugin.id)
            const installedVersion = installed?.version
            const updateAvailable = Boolean(
              installedVersion && compareVersions(plugin.version, installedVersion) > 0
            )
            const pluginTask = tasks.find(
              (task) =>
                task.source === 'marketplace' &&
                task.id.includes(`-${plugin.id}-`) &&
                ['queued', 'running', 'paused', 'waiting-user'].includes(task.status)
            )
            return (
              <Dropdown
                key={plugin.id}
                trigger={['contextMenu']}
                menu={{
                  items: [
                    {
                      key: 'select',
                      label: selectionMode ? '已进入多选模式' : '进入多选并选择此插件',
                      disabled: selectionMode,
                      onClick: () => {
                        setSelectionMode(true)
                        toggleSelected(plugin.id, true)
                      }
                    }
                  ]
                }}
              >
                <article
                  role="listitem"
                  className="ob-market-card ob-surface-card"
                  data-card-variant={variants.get(plugin.id)}
                  data-module={plugin.id.toUpperCase().slice(0, 12)}
                  aria-selected={selectionMode ? selectedIds.has(plugin.id) : undefined}
                  onClick={() => {
                    if (selectionMode) toggleSelected(plugin.id, !selectedIds.has(plugin.id))
                    else setSelected(plugin)
                  }}
                  style={{
                    border: `1px solid ${token.colorBorder}`,
                    borderRadius: 14,
                    background: token.colorBgContainer
                  }}
                >
                  {selectionMode && (
                    <Checkbox
                      checked={selectedIds.has(plugin.id)}
                      disabled={batchBusy || downloadingId !== null}
                      aria-label={`选择 ${plugin.name}`}
                      onClick={(event) => event.stopPropagation()}
                      onChange={(event) => toggleSelected(plugin.id, event.target.checked)}
                    />
                  )}
                  <div className="ob-market-card-content">
                    <div className="ob-market-card-title ob-card-heading">
                      <span>{plugin.name}</span>
                      <span style={{ color: token.colorTextTertiary, fontSize: 11 }}>
                        v{plugin.version}
                      </span>
                    </div>
                    <div className="ob-market-card-status">
                      {updateAvailable && (
                        <Tag color={updateAvailable ? 'orange' : 'green'}>
                          {updateAvailable ? '可更新' : '新增'}
                        </Tag>
                      )}
                    </div>
                    <Paragraph
                      className="ob-market-card-description"
                      ellipsis={{ rows: 2 }}
                      title={plugin.description}
                      style={{ color: token.colorTextSecondary, fontSize: 12, margin: 0 }}
                    >
                      {plugin.description}
                    </Paragraph>
                    {pluginTask && typeof pluginTask.progress === 'number' && (
                      <Progress percent={pluginTask.progress} size="small" showInfo={false} />
                    )}
                    <Button
                      className="ob-market-card-action"
                      size="small"
                      type={installed && !updateAvailable ? 'default' : 'primary'}
                      loading={downloadingId === plugin.id}
                      disabled={batchBusy || downloadingId !== null}
                      icon={
                        installed && !updateAvailable ? <CheckOutlined /> : <DownloadOutlined />
                      }
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
              </Dropdown>
            )
          })}
        </div>
      )}

      <Drawer
        width={520}
        open={selected !== null}
        onClose={() => setSelected(null)}
        title="插件详情"
        className="cbx-market-detail"
      >
        {selected && (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 14 }}>
              <div>
                <Title level={4} style={{ margin: 0 }}>
                  {selected.name}
                </Title>
                <Text type="secondary">
                  {selected.publisher} · v{selected.version}
                  {installedByName.has(selected.id)
                    ? ` · 已安装 v${installedByName.get(selected.id)?.version}`
                    : ''}
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
              disabled={batchBusy || downloadingId !== null}
              icon={
                installedByName.has(selected.id) &&
                compareVersions(
                  selected.version,
                  installedByName.get(selected.id)?.version ?? '0.0.0'
                ) <= 0 ? (
                  <CheckOutlined />
                ) : (
                  <DownloadOutlined />
                )
              }
              onClick={() =>
                installedByName.has(selected.id) &&
                compareVersions(
                  selected.version,
                  installedByName.get(selected.id)?.version ?? '0.0.0'
                ) <= 0
                  ? openInstalled(selected)
                  : void requestInstall(selected)
              }
            >
              {installedByName.has(selected.id) &&
              compareVersions(
                selected.version,
                installedByName.get(selected.id)?.version ?? '0.0.0'
              ) <= 0
                ? '打开'
                : installedByName.has(selected.id)
                  ? '更新'
                  : '获取'}
            </Button>
          </Space>
        )}
      </Drawer>
    </div>
  )
}

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
import {
  CheckOutlined,
  DownloadOutlined,
  MoreOutlined,
  ReloadOutlined,
  SearchOutlined,
  SafetyCertificateOutlined
} from '@ant-design/icons'
import {
  OFFICIAL_MARKETPLACE_CATALOG,
  isNextOfficialPlugin,
  type MarketplacePlugin
} from '../marketplace-catalog'
import { pluginIdentity } from '../plugin-identity'
import { searchPlugins } from '../plugin-search'
import SearchDialog, { type SearchResult } from '../components/SearchDialog'
import { usePluginStore } from '../store/plugin.store'
import { useAppStore } from '../store/app.store'
import {
  tauriApi,
  type MarketplaceCatalogResponse,
  type MarketplaceSource,
  type PluginMarketplaceOrigin
} from '../api/tauriApi'
import { useTaskStore } from '../store/task.store'
import { compareVersions } from '../features/marketplace/selectors/compareVersions'

const { Title, Text, Paragraph } = Typography
const RETIRED_PLUGIN_IDS = new Set(['dice-roller', 'productivity-toolkit'])

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
  const [selectedTags, setSelectedTags] = useState<string[]>([])
  const [tagDialogOpen, setTagDialogOpen] = useState(false)
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null)
  const [searchOpen, setSearchOpen] = useState(false)
  const [selected, setSelected] = useState<MarketplacePlugin | null>(null)
  const [remoteCatalog, setRemoteCatalog] = useState<MarketplacePlugin[] | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [channel, setChannel] = useState<'stable' | 'beta'>('stable')
  const [channelReady, setChannelReady] = useState(false)
  const [sources, setSources] = useState<MarketplaceSource[]>([])
  const [origins, setOrigins] = useState<PluginMarketplaceOrigin[]>([])
  const [selectedSourceId, setSelectedSourceId] = useState<number | undefined>()
  const [sourceDialogOpen, setSourceDialogOpen] = useState(false)
  const [sourceName, setSourceName] = useState('')
  const [sourceUrl, setSourceUrl] = useState('')
  const [catalogSource, setCatalogSource] = useState('内置目录')
  const [catalogStale, setCatalogStale] = useState(false)

  const [lastCheckedAt, setLastCheckedAt] = useState<number | null>(null)
  const [newPluginCount, setNewPluginCount] = useState(0)
  const [updateCount, setUpdateCount] = useState(0)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [selectionMode, setSelectionMode] = useState(false)
  const [batchBusy, setBatchBusy] = useState(false)
  const [batchResult, setBatchResult] = useState<string | null>(null)
  const activeDownloadTaskRef = useRef<string | null>(null)
  const tasks = useTaskStore((state) => state.tasks)

  const loadSources = useCallback(async () => {
    setSources(await tauriApi.plugin.marketplaceSources.list())
  }, [])
  useEffect(() => {
    void loadSources().catch(() => setSources([]))
  }, [loadSources])

  useEffect(() => {
    let active = true
    void Promise.all([tauriApi.settings.get('updateChannel'), tauriApi.app.getVersion()])
      .then(([value, version]) => {
        if (!active) return
        setChannel(/-(beta|rc)\./i.test(version) ? 'beta' : value === 'beta' ? 'beta' : 'stable')
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
      setRefreshing(true)
      setCatalogError(null)
      setRemoteCatalog(null)
      try {
        const payload: MarketplaceCatalogResponse = await tauriApi.plugin.marketplaceCatalog(
          forceRefresh,
          channel,
          selectedSourceId
        )
        const byId = new Map(OFFICIAL_MARKETPLACE_CATALOG.map((plugin) => [plugin.id, plugin]))
        const knownIds = new Set(byId.keys())
        const merged = payload.plugins
          .map((entry) => {
            const base = byId.get(String(entry.id))
            if (!entry.id || typeof entry.version !== 'string') return null
            if (selectedSourceId === undefined && !isNextOfficialPlugin(entry.id)) return null
            if (
              (selectedSourceId === undefined &&
                payload.schemaVersion === 1 &&
                RETIRED_PLUGIN_IDS.has(entry.id)) ||
              (!base && !entry.displayName)
            )
              return null
            if (
              selectedSourceId === undefined &&
              payload.schemaVersion === 1 &&
              base &&
              compareVersions(entry.version, base.version) < 0
            )
              return null
            const identity = pluginIdentity(entry.id, entry.publisher)
            const highlights = Array.isArray(entry.highlights)
              ? entry.highlights.filter((item): item is string => typeof item === 'string')
              : (base?.highlights ?? [])
            return {
              ...(base ?? {
                id: entry.id,
                name: entry.displayName ?? entry.id,
                version: entry.version,
                publisher: entry.publisher ?? 'CrucibleBox',
                category: entry.category ?? identity.category,
                description: entry.description ?? '',
                highlights,
                tags: entry.tags ?? []
              }),
              id: entry.id,
              name:
                selectedSourceId === undefined
                  ? (base?.name ?? entry.displayName ?? entry.id)
                  : (entry.displayName ?? base?.name ?? entry.id),
              version: entry.version,
              publisher: entry.publisher ?? base?.publisher ?? 'CrucibleBox',
              category: entry.category ?? base?.category ?? identity.category,
              description: entry.description ?? base?.description ?? '',
              highlights,
              tags: entry.tags ?? base?.tags ?? [],
              keywords: entry.keywords ?? base?.keywords ?? [],
              ...(entry.artifact ? { artifact: entry.artifact } : {}),
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
        setOrigins(await tauriApi.plugin.marketplaceOrigins())
        const installed = usePluginStore.getState().plugins
        setNewPluginCount(merged.filter((plugin) => !knownIds.has(plugin.id)).length)
        setUpdateCount(
          merged.filter((plugin) => {
            const installedVersion = installed.find((item) => item.id === plugin.id)?.version
            return Boolean(
              installedVersion && compareVersions(plugin.version, installedVersion) > 0
            )
          }).length
        )
      } catch (error) {
        setCatalogError(error instanceof Error ? error.message : String(error))
      } finally {
        setRefreshing(false)
      }
    },
    [channel, fetchPlugins, selectedSourceId]
  )

  useEffect(() => {
    if (channelReady) void loadCatalog(false)
  }, [channelReady, loadCatalog])

  const installedById = useMemo(
    () => new Map(plugins.map((plugin) => [plugin.id, plugin])),
    [plugins]
  )
  const marketplaceItems = useMemo(
    () => remoteCatalog ?? (selectedSourceId === undefined ? OFFICIAL_MARKETPLACE_CATALOG : []),
    [remoteCatalog, selectedSourceId]
  )
  const availableTags = useMemo(
    () => [...new Set(marketplaceItems.flatMap((plugin) => plugin.tags))].sort(),
    [marketplaceItems]
  )
  const availableCategories = useMemo(
    () => [...new Set(marketplaceItems.map((plugin) => plugin.category))].sort(),
    [marketplaceItems]
  )
  const catalog = useMemo(() => {
    const filtered = marketplaceItems.filter((plugin) => {
      if (selectedCategory && plugin.category !== selectedCategory) return false
      if (selectedTags.length > 0 && !selectedTags.some((tag) => plugin.tags.includes(tag)))
        return false
      return true
    })
    return searchPlugins(filtered, query, (plugin) => ({
      name: plugin.name,
      id: plugin.id,
      keywords: [plugin.category, ...plugin.highlights, ...plugin.tags, ...(plugin.keywords ?? [])],
      description: plugin.description
    }))
  }, [marketplaceItems, query, selectedCategory, selectedTags])

  const updatePlugins = useMemo(
    () =>
      marketplaceItems.filter((plugin) => {
        const installedVersion = installedById.get(plugin.id)?.version
        return Boolean(installedVersion && compareVersions(plugin.version, installedVersion) > 0)
      }),
    [installedById, marketplaceItems]
  )

  const selectedPlugins = useMemo(
    () => catalog.filter((plugin) => selectedIds.has(plugin.id)),
    [catalog, selectedIds]
  )
  const installableSelected = selectedPlugins.filter((plugin) => {
    const installedVersion = installedById.get(plugin.id)?.version
    return !installedVersion || compareVersions(plugin.version, installedVersion) > 0
  })

  const openInstalled = (plugin: MarketplacePlugin) => {
    const installed = installedById.get(plugin.id)
    if (!installed) return
    setActivePluginId(installed.id)
    setCurrentPage('pluginView')
  }

  const requestInstall = async (plugin: MarketplacePlugin) => {
    const installed = installedById.get(plugin.id)
    const origin = origins.find((item) => item.pluginId === plugin.id)
    if (installed && (origin?.sourceId ?? 0) !== (selectedSourceId ?? 0)) {
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
        taskId,
        selectedSourceId
      )
      activeDownloadTaskRef.current = null
      const phaseTaskId = `${taskId}:install`
      const prepared = await installPlugin('zip', path, {
        id: phaseTaskId,
        title: `安装 ${plugin.name}`,
        marketplaceSourceId: selectedSourceId ?? 0
      })
      const installError = usePluginStore.getState().error
      if (!prepared)
        useTaskStore
          .getState()
          .patchTask(phaseTaskId, { status: 'failed', error: installError ?? '插件安装预检失败' })
      if (!prepared) setDownloadError(installError ?? '插件安装预检失败，请查看任务中心后重试。')
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      const cancelled = message.includes('CANCELLED')
      if (!cancelled) setDownloadError(message)
    } finally {
      setDownloadingId(null)
      activeDownloadTaskRef.current = null
    }
  }

  const installLocal = async (source: 'zip' | 'directory') => {
    const path =
      source === 'zip' ? await tauriApi.dialog.openFile() : await tauriApi.dialog.openDirectory()
    if (!path) return
    const prepared = await installPlugin(source, path, {
      id: `local-plugin-install-${crypto.randomUUID()}`,
      title: source === 'zip' ? '安装本地插件包' : '安装开发目录'
    })
    if (!prepared) setDownloadError(usePluginStore.getState().error ?? '本地插件安装预检失败')
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

  const exitSelectionMode = () => {
    setSelectionMode(false)
    setSelectedIds(new Set())
  }

  const downloadBatch = async (items: MarketplacePlugin[], action: 'download' | 'update') => {
    const installable = items.filter((plugin) => {
      const installedVersion = installedById.get(plugin.id)?.version
      return !installedVersion || compareVersions(plugin.version, installedVersion) > 0
    })
    if (installable.length === 0 || batchBusy) return
    const differentSource = installable.filter(
      (plugin) =>
        installedById.has(plugin.id) &&
        (origins.find((origin) => origin.pluginId === plugin.id)?.sourceId ?? 0) !==
          (selectedSourceId ?? 0)
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
        const path = await tauriApi.plugin.marketplaceDownload(
          plugin.id,
          channel,
          'normal',
          taskId,
          selectedSourceId
        )
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
          marketplaceSourceId: selectedSourceId ?? 0
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
          <Text type="secondary">
            {catalogStale
              ? '当前使用最近一次可用目录'
              : `通道：${channel === 'beta' ? '测试版' : '稳定版'}`}
            {lastCheckedAt ? ` · ${new Date(lastCheckedAt).toLocaleTimeString()}` : ''}
          </Text>
        </div>
        <Space>
          {(newPluginCount > 0 || updateCount > 0) && (
            <Tag color="orange">
              新增 {newPluginCount} · 可更新 {updateCount}
            </Tag>
          )}
          <Tag icon={<SafetyCertificateOutlined />} color={catalogStale ? 'gold' : 'blue'}>
            {catalogStale
              ? '缓存目录'
              : selectedSourceId !== undefined
                ? (sources.find((source) => source.id === selectedSourceId)?.name ?? '第三方目录')
                : catalogSource.includes('tauri-beta')
                  ? 'CrucibleBox 测试版目录'
                  : 'CrucibleBox 官方目录'}
          </Tag>
          {selectionMode ? (
            <>
              <Checkbox
                checked={allVisibleSelected}
                indeterminate={!allVisibleSelected && someVisibleSelected}
                disabled={batchBusy || catalog.length === 0}
                onChange={(event) => toggleAllVisible(event.target.checked)}
              >
                全选当前结果
              </Checkbox>
              <Button
                disabled={batchBusy || installableSelected.length === 0}
                loading={batchBusy}
                icon={<DownloadOutlined />}
                onClick={() => void downloadBatch(installableSelected, 'download')}
              >
                批量下载 ({installableSelected.length})
              </Button>
              <Button
                disabled={batchBusy || updatePlugins.length === 0}
                loading={batchBusy}
                type={updatePlugins.length > 0 ? 'primary' : 'default'}
                icon={<DownloadOutlined />}
                onClick={() => void downloadBatch(updatePlugins, 'update')}
              >
                全部更新 ({updatePlugins.length})
              </Button>
              <Button disabled={batchBusy} onClick={exitSelectionMode}>
                完成
              </Button>
            </>
          ) : (
            <Button icon={<MoreOutlined />} onClick={() => setSelectionMode(true)}>
              多选
            </Button>
          )}
          <Button
            icon={<ReloadOutlined />}
            loading={refreshing}
            onClick={() => void loadCatalog(true)}
          >
            刷新
          </Button>
        </Space>
      </div>

      <Space wrap style={{ marginBottom: 16 }}>
        <Text type="secondary">插件来源</Text>
        <Select
          aria-label="选择插件目录"
          style={{ minWidth: 230 }}
          value={selectedSourceId ?? 0}
          onChange={(value) => {
            setSelectedSourceId(value === 0 ? undefined : value)
            setSelectedIds(new Set())
            setSelected(null)
          }}
          options={[
            { value: 0, label: 'CrucibleBox 官方目录' },
            ...sources
              .filter((source) => source.enabled)
              .map((source) => ({ value: source.id, label: source.name }))
          ]}
        />
        <Button onClick={() => setSourceDialogOpen(true)}>管理第三方目录</Button>
        <Button onClick={() => void installLocal('zip')}>安装本地 ZIP</Button>
        <Button onClick={() => void installLocal('directory')}>安装开发目录</Button>
        {selectedSourceId !== undefined && (
          <Text type="secondary">第三方插件由所选来源提供，安装前请查看权限。</Text>
        )}
      </Space>

      <Modal
        title="管理第三方插件目录"
        open={sourceDialogOpen}
        onCancel={() => setSourceDialogOpen(false)}
        footer={null}
      >
        <p>可添加自己信任的 HTTPS 插件目录；插件的本地进程等权限会在安装时单独确认。</p>
        <Input
          value={sourceName}
          onChange={(event) => setSourceName(event.target.value)}
          placeholder="目录名称"
          style={{ marginBottom: 8 }}
        />
        <Input
          value={sourceUrl}
          onChange={(event) => setSourceUrl(event.target.value)}
          placeholder="https://example.com/plugins.json"
          style={{ marginBottom: 8 }}
        />
        <Button
          type="primary"
          disabled={!sourceName.trim() || !sourceUrl.trim()}
          onClick={() =>
            void (async () => {
              try {
                await tauriApi.plugin.marketplaceSources.add(sourceName, sourceUrl)
                setSourceName('')
                setSourceUrl('')
                await loadSources()
              } catch (error) {
                setCatalogError(error instanceof Error ? error.message : String(error))
              }
            })()
          }
        >
          添加目录
        </Button>
        <div style={{ marginTop: 18 }}>
          {sources.map((source) => (
            <div
              key={source.id}
              style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}
            >
              <div style={{ minWidth: 0, flex: 1 }}>
                <strong>{source.name}</strong>
                <div style={{ overflowWrap: 'anywhere' }}>{source.url}</div>
              </div>
              <Button
                size="small"
                onClick={() =>
                  void (async () => {
                    await tauriApi.plugin.marketplaceSources.setEnabled(source.id, !source.enabled)
                    if (selectedSourceId === source.id) setSelectedSourceId(undefined)
                    await loadSources()
                  })()
                }
              >
                {source.enabled ? '停用' : '启用'}
              </Button>
              <Button
                size="small"
                danger
                onClick={() =>
                  void (async () => {
                    await tauriApi.plugin.marketplaceSources.delete(source.id)
                    if (selectedSourceId === source.id) setSelectedSourceId(undefined)
                    await loadSources()
                  })()
                }
              >
                删除
              </Button>
            </div>
          ))}
        </div>
      </Modal>

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

      <button
        type="button"
        className="ob-search-btn"
        onClick={() => setSearchOpen(true)}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          width: '100%',
          padding: '10px 16px',
          marginBottom: 20,
          border: `1px solid ${token.colorBorder}`,
          borderRadius: 10,
          background: token.colorBgContainer,
          color: token.colorTextTertiary,
          fontSize: 14,
          cursor: 'pointer',
          transition: 'all 0.2s ease'
        }}
      >
        <SearchOutlined aria-hidden="true" />
        <span>{query || '按名称、分类或功能搜索插件…'}</span>
      </button>

      <div className="ob-market-filters" aria-label="插件市场筛选">
        <div className="ob-market-filter-row">
          <Select
            aria-label="插件分类"
            value={selectedCategory ?? ''}
            options={[
              { label: '全部分类', value: '' },
              ...availableCategories.map((category) => ({ label: category, value: category }))
            ]}
            onChange={(category) => setSelectedCategory(category || null)}
            style={{ width: 160 }}
          />
          {availableTags.length > 0 && (
            <Button
              type={selectedTags.length > 0 ? 'primary' : 'default'}
              onClick={() => setTagDialogOpen(true)}
            >
              功能筛选{selectedTags.length > 0 ? `（${selectedTags.length}）` : ''}
            </Button>
          )}
          {(selectedTags.length > 0 || selectedCategory !== null || query) && (
            <Button
              size="small"
              onClick={() => {
                setSelectedTags([])
                setSelectedCategory(null)
                setQuery('')
              }}
            >
              清除筛选
            </Button>
          )}
        </div>
      </div>

      <Modal
        title="选择功能标签"
        open={tagDialogOpen}
        onCancel={() => setTagDialogOpen(false)}
        footer={
          <Button type="primary" onClick={() => setTagDialogOpen(false)}>
            完成
          </Button>
        }
      >
        <Select
          mode="multiple"
          allowClear
          showSearch
          value={selectedTags}
          options={availableTags.map((tag) => ({ label: tag, value: tag }))}
          placeholder="搜索并选择标签"
          style={{ width: '100%' }}
          onChange={setSelectedTags}
          optionFilterProp="label"
        />
        <Text type="secondary" style={{ display: 'block', marginTop: 12 }}>
          标签之间采用“或”关系，并与文字查询同时生效。
        </Text>
      </Modal>

      <SearchDialog
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        query={query}
        onQueryChange={setQuery}
        title="搜索插件市场"
        results={catalog.map((plugin): SearchResult => ({
          key: plugin.id,
          label: plugin.name,
          icon: <SearchOutlined />,
          kind: '插件',
          run: () => {
            if (selectionMode) toggleSelected(plugin.id, !selectedIds.has(plugin.id))
            else setSelected(plugin)
          }
        }))}
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
            const installed = installedById.get(plugin.id)
            const identity = pluginIdentity(plugin.id)
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
                      disabled={batchBusy}
                      aria-label={`选择 ${plugin.name}`}
                      onClick={(event) => event.stopPropagation()}
                      onChange={(event) => toggleSelected(plugin.id, event.target.checked)}
                    />
                  )}
                  <div className="ob-market-card-content">
                    <div className="ob-market-card-title">
                      <span>{plugin.name}</span>
                      <span style={{ color: token.colorTextTertiary, fontSize: 11 }}>
                        v{plugin.version}
                      </span>
                    </div>
                    <div className="ob-market-card-status">
                      {(updateAvailable ||
                        (!installed &&
                          newPluginCount > 0 &&
                          !OFFICIAL_MARKETPLACE_CATALOG.some((item) => item.id === plugin.id))) && (
                        <Tag color={updateAvailable ? 'orange' : 'green'}>
                          {updateAvailable ? '可更新' : '新增'}
                        </Tag>
                      )}
                      <Text type="secondary">{plugin.category || identity.category}</Text>
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
                      disabled={batchBusy}
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
                  {installedById.has(selected.id)
                    ? ` · 已安装 v${installedById.get(selected.id)?.version}`
                    : ''}
                </Text>
              </div>
            </div>
            <Paragraph>{selected.description}</Paragraph>
            <Text type="secondary">{selected.category}</Text>
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
              disabled={batchBusy}
              icon={
                installedById.has(selected.id) &&
                compareVersions(
                  selected.version,
                  installedById.get(selected.id)?.version ?? '0.0.0'
                ) <= 0 ? (
                  <CheckOutlined />
                ) : (
                  <DownloadOutlined />
                )
              }
              onClick={() =>
                installedById.has(selected.id) &&
                compareVersions(
                  selected.version,
                  installedById.get(selected.id)?.version ?? '0.0.0'
                ) <= 0
                  ? openInstalled(selected)
                  : void requestInstall(selected)
              }
            >
              {installedById.has(selected.id) &&
              compareVersions(
                selected.version,
                installedById.get(selected.id)?.version ?? '0.0.0'
              ) <= 0
                ? '打开'
                : installedById.has(selected.id)
                  ? '更新'
                  : '获取'}
            </Button>
          </Space>
        )}
      </Drawer>
    </div>
  )
}

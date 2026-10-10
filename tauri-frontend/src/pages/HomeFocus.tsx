import {
  DndContext,
  PointerSensor,
  KeyboardSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent
} from '@dnd-kit/core'
import {
  SortableContext,
  verticalListSortingStrategy,
  sortableKeyboardCoordinates,
  arrayMove
} from '@dnd-kit/sortable'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { App, Alert, Button, Checkbox, Drawer, Empty, Input, Modal, Spin, Tooltip } from 'antd'
import {
  CheckSquareOutlined,
  ImportOutlined,
  ReloadOutlined,
  SearchOutlined
} from '@ant-design/icons'
import type { PluginMeta } from '../../../shared/types/plugin.types'
import { usePlugins } from '../hooks/usePlugins'
import { useAppStore } from '../store/app.store'
import { useTaskStore } from '../store/task.store'
import { isOfficialPlugin } from '../plugin-identity'
import { OFFICIAL_MARKETPLACE_CATALOG } from '../marketplace-catalog'
import { searchPlugins } from '../plugin-search'
import PluginConfig from '../components/PluginConfig'
import PluginImport from '../components/PluginImport'
import PluginDirectoryRow from '../features/workbench/components/PluginDirectoryRow'
import UserTagManager from '../components/UserTagManager'
import './home-focus.css'

const RECENT_PLUGIN_KEY = 'cruciblebox.recent-plugin-id'
const ACTIVE_TASK_STATUSES = new Set(['queued', 'running', 'paused', 'waiting-user'])
const OFFICIAL_SEARCH_FIELDS = new Map(
  OFFICIAL_MARKETPLACE_CATALOG.map((plugin) => [plugin.id, plugin])
)

export default function HomeFocus() {
  const { message } = App.useApp()
  const {
    plugins,
    loading,
    error,
    fetchPlugins,
    enablePlugin,
    disablePlugin,
    batchEnablePlugins,
    batchDisablePlugins,
    uninstallPlugin,
    reorderPlugins,
    pluginOperationBusy,
    batchOperationBusy,
    reorderBusy
  } = usePlugins()
  const tasks = useTaskStore((state) => state.tasks)
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const setActivePluginId = useAppStore((state) => state.setActivePluginId)
  const importOpen = useAppStore((state) => state.pluginImportOpen)
  const setImportOpen = useAppStore((state) => state.setPluginImportOpen)

  const [previewId, setPreviewId] = useState<string | null>(() => {
    try {
      return window.localStorage.getItem(RECENT_PLUGIN_KEY)
    } catch {
      return null
    }
  })
  const [query, setQuery] = useState('')
  const [batchMode, setBatchMode] = useState(false)
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [batchBusy, setBatchBusy] = useState(false)
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false)
  const [configPlugin, setConfigPlugin] = useState<PluginMeta | null>(null)
  const [tagDrawerOpen, setTagDrawerOpen] = useState(false)
  const [tagFilterIds, setTagFilterIds] = useState<string[] | null>(null)
  const [refreshing, setRefreshing] = useState(false)

  const preview = useMemo(
    () => plugins.find((plugin) => plugin.id === previewId) ?? plugins[0] ?? null,
    [plugins, previewId]
  )
  const visiblePlugins = useMemo(() => {
    const tagged = plugins.filter(
      (plugin) => tagFilterIds === null || tagFilterIds.includes(plugin.id)
    )
    return searchPlugins(tagged, query, (plugin) => {
      const official = OFFICIAL_SEARCH_FIELDS.get(plugin.id)
      return {
        name: plugin.displayName || plugin.name,
        id: plugin.id,
        keywords: [
          plugin.name,
          ...(official?.tags ?? []),
          ...(official?.keywords ?? []),
          ...(official?.highlights ?? [])
        ],
        description: plugin.description
      }
    })
  }, [plugins, query, tagFilterIds])
  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds])
  const selectedVisibleCount = visiblePlugins.filter((plugin) => selectedSet.has(plugin.id)).length
  const allVisibleSelected =
    visiblePlugins.length > 0 && selectedVisibleCount === visiblePlugins.length
  const busy =
    loading ||
    batchBusy ||
    batchOperationBusy ||
    reorderBusy ||
    Object.values(pluginOperationBusy).some(Boolean)
  const activeTaskCount = preview
    ? tasks.filter((task) => task.owner === preview.id && ACTIVE_TASK_STATUSES.has(task.status))
        .length
    : 0

  useEffect(() => {
    const ids = new Set(plugins.map((plugin) => plugin.id))
    setSelectedIds((current) => current.filter((id) => ids.has(id)))
  }, [plugins])

  const openPlugin = useCallback(
    (plugin: PluginMeta) => {
      try {
        window.localStorage.setItem(RECENT_PLUGIN_KEY, plugin.id)
      } catch {
        /* optional recent shortcut */
      }
      setPreviewId(plugin.id)
      setActivePluginId(plugin.id)
      setCurrentPage('pluginView')
    },
    [setActivePluginId, setCurrentPage]
  )

  const toggleSelected = (id: string) => {
    setSelectedIds((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id]
    )
  }

  const toggleSelectAll = () => {
    const visibleIds = visiblePlugins.map((plugin) => plugin.id)
    setSelectedIds((current) =>
      allVisibleSelected
        ? current.filter((id) => !visibleIds.includes(id))
        : [...new Set([...current, ...visibleIds])]
    )
  }

  const runBatchToggle = async (enabled: boolean) => {
    const targetIds = selectedIds.filter(
      (id) => plugins.find((plugin) => plugin.id === id)?.enabled !== enabled
    )
    if (targetIds.length === 0 || busy) return
    setBatchBusy(true)
    try {
      const result = enabled
        ? await batchEnablePlugins(targetIds)
        : await batchDisablePlugins(targetIds)
      if (result.failures.length > 0) {
        message.warning(
          `${enabled ? '启用' : '停用'}成功 ${result.succeeded.length} 个，失败 ${result.failures.length} 个`
        )
        setSelectedIds(result.failures.map((failure) => failure.id))
      } else {
        message.success(`已${enabled ? '启用' : '停用'} ${result.succeeded.length} 个插件`)
        setSelectedIds([])
      }
    } finally {
      setBatchBusy(false)
    }
  }

  const deleteSelected = async () => {
    if (busy || selectedIds.length === 0) return
    setBatchBusy(true)
    const failures: string[] = []
    let succeeded = 0
    try {
      for (const id of selectedIds) {
        if (await uninstallPlugin(id)) succeeded += 1
        else failures.push(id)
      }
      setSelectedIds(failures)
      setDeleteConfirmOpen(false)
      if (failures.length > 0) message.warning(`已卸载 ${succeeded} 个，失败 ${failures.length} 个`)
      else message.success(`已卸载 ${succeeded} 个插件`)
    } finally {
      setBatchBusy(false)
    }
  }

  const togglePreviewEnabled = async () => {
    if (!preview || busy) return
    const ok = preview.enabled ? await disablePlugin(preview.id) : await enablePlugin(preview.id)
    if (ok) message.success(preview.enabled ? '插件已停用' : '插件已启用')
    else message.error('操作失败')
  }

  const movePreview = async (direction: -1 | 1) => {
    if (!preview || busy) return
    const index = plugins.findIndex((plugin) => plugin.id === preview.id)
    const target = index + direction
    if (target < 0 || target >= plugins.length) return
    const orderedIds = plugins.map((plugin) => plugin.id)
    ;[orderedIds[index], orderedIds[target]] = [orderedIds[target], orderedIds[index]]
    if (!(await reorderPlugins(orderedIds))) message.error('调整顺序失败')
  }

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { delay: 500, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates })
  )
  const onDragEnd = async ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id || busy) return
    const ids = plugins.map((plugin) => plugin.id)
    const from = ids.indexOf(String(active.id))
    const to = ids.indexOf(String(over.id))
    if (from < 0 || to < 0) return
    if (!(await reorderPlugins(arrayMove(ids, from, to)))) message.error('调整顺序失败')
  }

  const refresh = async () => {
    setRefreshing(true)
    try {
      await fetchPlugins()
    } finally {
      setRefreshing(false)
    }
  }

  if (loading && plugins.length === 0) {
    return (
      <div className="cbx-focus-loading">
        <Spin size="large" />
      </div>
    )
  }

  return (
    <div className="cbx-workbench">
      <header className="cbx-workbench-head">
        <div className="cbx-workbench-heading">
          <span>YOUR WORKSPACE</span>
          <h1>工作台</h1>
        </div>
        <div className="cbx-workbench-actions">
          <Button
            icon={<ReloadOutlined />}
            onClick={() => void refresh()}
            loading={refreshing}
            disabled={busy}
          >
            刷新
          </Button>
          <Button icon={<ImportOutlined />} onClick={() => setImportOpen(true)} disabled={busy}>
            导入插件
          </Button>
          <Button
            type={batchMode ? 'primary' : 'default'}
            icon={<CheckSquareOutlined />}
            onClick={() => {
              setBatchMode((current) => !current)
              setSelectedIds([])
            }}
            disabled={busy}
          >
            {batchMode ? '完成管理' : '批量管理'}
          </Button>
        </div>
      </header>

      {error && (
        <Alert
          type="error"
          showIcon
          message="插件加载失败"
          description={error}
          action={
            <Button size="small" onClick={() => void refresh()}>
              重试
            </Button>
          }
        />
      )}

      {plugins.length === 0 ? (
        <div className="cbx-workbench-empty">
          <Empty description="还没有安装插件">
            <Button type="primary" onClick={() => setImportOpen(true)}>
              导入插件
            </Button>
          </Empty>
        </div>
      ) : (
        <div className="cbx-workbench-body">
          <section className="cbx-focus-panel" aria-label="插件简介">
            <div className="cbx-focus-top">
              <span>工作台焦点</span>
              <span>
                {String(plugins.findIndex((plugin) => plugin.id === preview?.id) + 1).padStart(
                  2,
                  '0'
                )}{' '}
                / {plugins.length}
              </span>
            </div>
            {preview && (
              <>
                <div className="cbx-focus-summary">
                  <span className="cbx-focus-eyebrow">
                    {preview.id === previewId ? '当前预览的插件' : '最近使用的插件'}
                  </span>
                  <h2>{preview.displayName}</h2>
                  <p>{preview.description || '该插件尚未提供简介。'}</p>
                </div>
                <div className="cbx-focus-facts">
                  <div>
                    <span>启用状态</span>
                    <strong>{preview.enabled ? '已启用' : '已停用'}</strong>
                  </div>
                  <div>
                    <span>插件来源</span>
                    <strong>
                      {isOfficialPlugin(preview.name)
                        ? 'CrucibleBox 官方'
                        : preview.author || '第三方'}
                    </strong>
                  </div>
                </div>
                <div className="cbx-focus-activity">
                  <strong>任务动态</strong>
                  <span>
                    {activeTaskCount > 0
                      ? `${activeTaskCount} 个任务进行中`
                      : '当前没有进行中的任务'}
                  </span>
                </div>
                <div className="cbx-focus-actions">
                  <Button type="primary" onClick={() => openPlugin(preview)}>
                    打开插件 ↗
                  </Button>
                  <Button onClick={() => setConfigPlugin(preview)}>插件配置</Button>
                </div>
                <div className="cbx-focus-utilities">
                  <Button
                    type="link"
                    size="small"
                    onClick={() => void togglePreviewEnabled()}
                    disabled={busy}
                  >
                    {preview.enabled ? '停用插件' : '启用插件'}
                  </Button>
                  <Button
                    type="link"
                    size="small"
                    onClick={() => void movePreview(-1)}
                    disabled={busy || plugins[0]?.id === preview.id}
                  >
                    上移
                  </Button>
                  <Button
                    type="link"
                    size="small"
                    onClick={() => void movePreview(1)}
                    disabled={busy || plugins[plugins.length - 1]?.id === preview.id}
                  >
                    下移
                  </Button>
                </div>
              </>
            )}
          </section>

          <section className="cbx-directory" aria-label="已安装插件">
            <div className="cbx-directory-head">
              <div>
                <strong>插件目录</strong>
                <span>已安装 {plugins.length} 个插件</span>
              </div>
              <span>ALL TOOLS</span>
            </div>
            <div className="cbx-directory-tools">
              <Input
                prefix={<SearchOutlined />}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索插件名称…"
                aria-label="搜索插件"
                allowClear
              />
              <Tooltip title="筛选与管理标签">
                <Button onClick={() => setTagDrawerOpen(true)}>标签</Button>
              </Tooltip>
            </div>
            {batchMode && (
              <div className="cbx-directory-select-all">
                <Checkbox
                  checked={allVisibleSelected}
                  indeterminate={selectedVisibleCount > 0 && !allVisibleSelected}
                  onChange={toggleSelectAll}
                >
                  选择当前结果
                </Checkbox>
                <span>已选 {selectedIds.length}</span>
              </div>
            )}
            <DndContext
              sensors={sensors}
              collisionDetection={closestCenter}
              onDragEnd={(event) => void onDragEnd(event)}
            >
              <SortableContext
                items={visiblePlugins.map((plugin) => plugin.id)}
                strategy={verticalListSortingStrategy}
              >
                <div className="cbx-directory-list" role="list">
                  {visiblePlugins.length === 0 && (
                    <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="没有匹配的插件" />
                  )}
                  {visiblePlugins.map((plugin) => (
                    <PluginDirectoryRow
                      key={plugin.id}
                      id={plugin.id}
                      reorderDisabled={busy || batchMode}
                      displayName={plugin.displayName}
                      enabled={plugin.enabled}
                      previewed={preview?.id === plugin.id}
                      batchMode={batchMode}
                      selected={selectedSet.has(plugin.id)}
                      onSelect={() => toggleSelected(plugin.id)}
                      onPreview={() => setPreviewId(plugin.id)}
                      onOpen={() => openPlugin(plugin)}
                    />
                  ))}
                </div>
              </SortableContext>
            </DndContext>
            {batchMode ? (
              <div className="cbx-directory-batch">
                <span>已选 {selectedIds.length} 项</span>
                <div>
                  <Button
                    size="small"
                    onClick={() => void runBatchToggle(true)}
                    disabled={busy || selectedIds.length === 0}
                  >
                    启用
                  </Button>
                  <Button
                    size="small"
                    onClick={() => void runBatchToggle(false)}
                    disabled={busy || selectedIds.length === 0}
                  >
                    停用
                  </Button>
                  <Button
                    size="small"
                    danger
                    onClick={() => setDeleteConfirmOpen(true)}
                    disabled={busy || selectedIds.length === 0}
                  >
                    卸载
                  </Button>
                </div>
              </div>
            ) : (
              <div className="cbx-directory-foot">单击预览 · 双击打开 · 长按拖动排序</div>
            )}
          </section>
        </div>
      )}

      <PluginConfig
        plugin={configPlugin}
        open={configPlugin !== null}
        onClose={() => setConfigPlugin(null)}
      />
      <PluginImport open={importOpen} onClose={() => setImportOpen(false)} />
      <Drawer
        title="插件标签"
        open={tagDrawerOpen}
        onClose={() => setTagDrawerOpen(false)}
        width={400}
      >
        <UserTagManager
          plugins={plugins}
          selectedPluginIds={selectedIds}
          onFilterChange={setTagFilterIds}
        />
      </Drawer>
      <Modal
        title="确认卸载插件"
        open={deleteConfirmOpen}
        onOk={() => void deleteSelected()}
        onCancel={() => setDeleteConfirmOpen(false)}
        okText={`卸载 ${selectedIds.length} 个`}
        okButtonProps={{ danger: true, loading: batchBusy }}
        centered
      >
        <p>将卸载选中的 {selectedIds.length} 个插件及其数据。此操作无法撤销。</p>
        <ul className="cbx-batch-names">
          {selectedIds.map((id) => (
            <li key={id}>{plugins.find((plugin) => plugin.id === id)?.displayName ?? id}</li>
          ))}
        </ul>
      </Modal>
    </div>
  )
}

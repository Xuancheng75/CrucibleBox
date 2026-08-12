import { useCallback, useMemo, useState } from 'react'
import { App, Button, Empty, Spin, Alert, theme } from 'antd'
import { ImportOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import {
  DndContext,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  DragOverlay,
  type DragEndEvent,
  type DragStartEvent
} from '@dnd-kit/core'
import { SortableContext, rectSortingStrategy, useSortable, arrayMove } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import LauncherCard from '../components/LauncherCard'
import PluginConfig from '../components/PluginConfig'
import PluginImport from '../components/PluginImport'
import { usePlugins } from '../hooks/usePlugins'
import { useAppStore } from '../store/app.store'
import { usePluginStore } from '../store/plugin.store'
import type { PluginMeta } from '@shared/types/plugin.types'

interface SortableLauncherCardProps {
  plugin: PluginMeta
  index: number
  total: number
  status?: import('@shared/types/plugin.types').PluginLifecycleStatus
  isSorting: boolean
  onToggle: (id: string, enabled: boolean) => void
  onDelete: (id: string) => void
  onConfigure: (plugin: PluginMeta) => void
  onOpen: (plugin: PluginMeta) => void
  onMove: (id: string, direction: -1 | 1) => void
}

function SortableLauncherCard({
  plugin,
  index,
  total,
  status,
  isSorting,
  onToggle,
  onDelete,
  onConfigure,
  onOpen,
  onMove
}: SortableLauncherCardProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: plugin.id,
    data: { plugin, index }
  })

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    listStyle: 'none'
  }

  return (
    <li
      ref={setNodeRef}
      className="ob-sortable-item"
      style={style}
      {...attributes}
      {...listeners}
      role="listitem"
      tabIndex={-1}
    >
      <LauncherCard
        plugin={plugin}
        status={status}
        onToggle={onToggle}
        onDelete={onDelete}
        onConfigure={onConfigure}
        onOpen={onOpen}
        sortable={{
          index,
          total,
          isDragging,
          isSorting,
          onMoveUp: () => onMove(plugin.id, -1),
          onMoveDown: () => onMove(plugin.id, 1)
        }}
      />
    </li>
  )
}

export default function Home() {
  const { token } = theme.useToken()
  const { message } = App.useApp()
  const {
    plugins,
    loading,
    error,
    activePlugins,
    fetchPlugins,
    enablePlugin,
    disablePlugin,
    uninstallPlugin,
    reorderPlugins
  } = usePlugins()
  const { setCurrentPage, setActivePluginId } = useAppStore()
  const importOpen = useAppStore((s) => s.pluginImportOpen)
  const setImportOpen = useAppStore((s) => s.setPluginImportOpen)
  const setCommandOpen = useAppStore((s) => s.setCommandOpen)
  const [configPlugin, setConfigPlugin] = useState<PluginMeta | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [activeId, setActiveId] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState('')

  const isSorting = activeId !== null
  const pluginIds = useMemo(() => plugins.map((p) => p.id), [plugins])
  const activePlugin = useMemo(() => plugins.find((p) => p.id === activeId), [plugins, activeId])

  const runningCount = Object.values(activePlugins).filter((s) => s === 'active').length

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { delay: 500, tolerance: 8 }
    })
  )

  const handleRefresh = async () => {
    setRefreshing(true)
    try {
      await fetchPlugins()
    } finally {
      setRefreshing(false)
    }
    if (!usePluginStore.getState().error) {
      message.success('已刷新')
    }
  }

  const handleToggle = async (id: string, enabled: boolean) => {
    const success = enabled ? await enablePlugin(id) : await disablePlugin(id)
    if (success) {
      message.success(enabled ? '插件已启用' : '插件已禁用')
    } else {
      message.error('操作失败')
    }
  }

  const handleDelete = async (id: string) => {
    const success = await uninstallPlugin(id)
    if (success) {
      message.success('插件已删除')
    } else {
      message.error('删除失败')
    }
  }

  const handleOpen = useCallback(
    (plugin: PluginMeta) => {
      setActivePluginId(plugin.id)
      setCurrentPage('pluginView')
    },
    [setActivePluginId, setCurrentPage]
  )

  const announceMove = useCallback((pluginName: string, newIndex: number, totalCount: number) => {
    setAnnouncement(`${pluginName} 移动到第 ${newIndex + 1} 位，共 ${totalCount} 个插件`)
  }, [])

  const applyReorder = useCallback(
    async (nextIds: string[], movedPluginName: string, newIndex: number) => {
      const ok = await reorderPlugins(nextIds)
      if (ok) {
        announceMove(movedPluginName, newIndex, nextIds.length)
      }
    },
    [reorderPlugins, announceMove]
  )

  const handleMove = useCallback(
    (id: string, direction: -1 | 1) => {
      const currentIndex = plugins.findIndex((p) => p.id === id)
      const nextIndex = currentIndex + direction
      if (currentIndex < 0 || nextIndex < 0 || nextIndex >= plugins.length) return
      const nextIds = arrayMove(
        plugins.map((p) => p.id),
        currentIndex,
        nextIndex
      )
      void applyReorder(nextIds, plugins[currentIndex].displayName, nextIndex)
    },
    [plugins, applyReorder]
  )

  const handleDragStart = useCallback((event: DragStartEvent) => {
    setActiveId(event.active.id as string)
  }, [])

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event
      if (over && active.id !== over.id) {
        const oldIndex = plugins.findIndex((p) => p.id === active.id)
        const newIndex = plugins.findIndex((p) => p.id === over.id)
        if (oldIndex !== -1 && newIndex !== -1) {
          const nextIds = arrayMove(
            plugins.map((p) => p.id),
            oldIndex,
            newIndex
          )
          void applyReorder(nextIds, plugins[oldIndex].displayName, newIndex)
        }
      }
      setTimeout(() => setActiveId(null), 0)
    },
    [plugins, applyReorder]
  )

  const handleDragCancel = useCallback(() => {
    setTimeout(() => setActiveId(null), 0)
  }, [])

  if (loading && plugins.length === 0) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', padding: 80 }}>
        <Spin size="large" />
      </div>
    )
  }

  return (
    <div>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: 20,
          gap: 16,
          flexWrap: 'wrap'
        }}
      >
        <div style={{ fontSize: 13, color: token.colorTextSecondary }}>
          {plugins.length} 个工具 · {runningCount} 个运行中
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <Button
            icon={<ReloadOutlined />}
            loading={refreshing}
            aria-label="刷新插件列表"
            onClick={handleRefresh}
          >
            刷新
          </Button>
          <Button
            data-ob-kind="primary"
            type="primary"
            icon={<ImportOutlined />}
            onClick={() => setImportOpen(true)}
          >
            导入插件
          </Button>
        </div>
      </div>

      <button
        className="ob-search-btn"
        onClick={() => setCommandOpen(true)}
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
        <SearchOutlined />
        <span>搜索插件…（Ctrl K）</span>
      </button>

      {error && (
        <Alert
          className="ob-alert-error"
          type="error"
          showIcon
          role="alert"
          aria-live="assertive"
          aria-atomic="true"
          title="插件加载失败"
          description={error}
          style={{ marginBottom: 20 }}
          action={
            <Button
              icon={<ReloadOutlined />}
              loading={refreshing}
              aria-label="重试加载插件"
              onClick={handleRefresh}
            >
              重试
            </Button>
          }
        />
      )}

      {plugins.length === 0 ? (
        <div
          style={{
            background: token.colorBgContainer,
            borderRadius: token.borderRadius,
            border: `1px solid ${token.colorBorder}`,
            padding: 60,
            textAlign: 'center',
            marginTop: 12
          }}
        >
          <Empty description="还没有安装任何插件">
            <Button
              data-ob-kind="primary"
              type="primary"
              icon={<ImportOutlined />}
              onClick={() => setImportOpen(true)}
            >
              导入第一个插件
            </Button>
          </Empty>
        </div>
      ) : (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          onDragStart={handleDragStart}
          onDragEnd={handleDragEnd}
          onDragCancel={handleDragCancel}
        >
          <div
            role="list"
            aria-label="已安装插件列表"
            aria-describedby="sort-instructions"
            className="ob-sortable-list"
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))',
              gap: 16
            }}
          >
            <SortableContext items={pluginIds} strategy={rectSortingStrategy}>
              {plugins.map((plugin, index) => (
                <SortableLauncherCard
                  key={plugin.id}
                  plugin={plugin}
                  index={index}
                  total={plugins.length}
                  status={activePlugins[plugin.id]}
                  isSorting={isSorting}
                  onToggle={handleToggle}
                  onDelete={handleDelete}
                  onConfigure={setConfigPlugin}
                  onOpen={handleOpen}
                  onMove={handleMove}
                />
              ))}
            </SortableContext>
          </div>
          <DragOverlay>
            {activePlugin ? (
              <div
                className="ob-launcher-drag-overlay"
                style={{
                  cursor: 'grabbing',
                  transform: 'scale(1.04)',
                  boxShadow: token.boxShadowSecondary
                }}
              >
                <LauncherCard
                  plugin={activePlugin}
                  status={activePlugins[activePlugin.id]}
                  onOpen={handleOpen}
                  sortable={{
                    index: plugins.findIndex((p) => p.id === activePlugin.id),
                    total: plugins.length,
                    isDragging: false,
                    isSorting: true
                  }}
                />
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      )}

      <div id="sort-instructions" className="ob-sr-only">
        长按卡片约半秒可拖动排序，或使用每张卡片操作区的上下按钮调整顺序。
      </div>
      <div aria-live="polite" aria-atomic="true" className="ob-sr-only">
        {announcement}
      </div>

      <PluginConfig
        plugin={configPlugin}
        open={configPlugin !== null}
        onClose={() => setConfigPlugin(null)}
      />
      <PluginImport open={importOpen} onClose={() => setImportOpen(false)} />
    </div>
  )
}

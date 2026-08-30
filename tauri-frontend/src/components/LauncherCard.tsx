import type { CSSProperties } from 'react'
import { useState } from 'react'
import { Button, Popconfirm, Tooltip, theme } from 'antd'
import {
  CheckCircleFilled,
  SettingOutlined,
  DeleteOutlined,
  PlayCircleOutlined,
  ArrowUpOutlined,
  ArrowDownOutlined
} from '@ant-design/icons'
import type { PluginMeta } from '../../../shared/types/plugin.types'
import { PluginLifecycleStatus } from '../../../shared/types/plugin.types'
import { isOfficialPlugin, pluginIdentity } from '../plugin-identity'
import PluginGlyph from './PluginGlyph'

interface SortableState {
  index: number
  total: number
  isDragging?: boolean
  isSorting?: boolean
  setNodeRef?: (node: HTMLElement | null) => void
  dragAttributes?: Record<string, unknown>
  dragListeners?: Record<string, unknown>
  dragStyle?: CSSProperties
  onMoveUp?: () => void
  onMoveDown?: () => void
}

interface LauncherCardProps {
  plugin: PluginMeta
  status?: PluginLifecycleStatus
  onToggle?: (id: string, enabled: boolean) => void
  onDelete?: (id: string) => void
  onConfigure?: (plugin: PluginMeta) => void
  onOpen: (plugin: PluginMeta) => void
  sortable?: SortableState
  /** 批量管理模式：卡片进入勾选态（点击=切换选中，隐藏操作区与打开行为） */
  selectable?: boolean
  selected?: boolean
  onSelectToggle?: (id: string) => void
}

type StatusTone = 'success' | 'warning' | 'error' | 'neutral'

const STATUS_META: Record<string, { tone: StatusTone; text: string }> = {
  [PluginLifecycleStatus.Active]: { tone: 'success', text: '运行中' },
  [PluginLifecycleStatus.Activating]: { tone: 'warning', text: '启动中' },
  [PluginLifecycleStatus.Deactivating]: { tone: 'warning', text: '停用中' },
  [PluginLifecycleStatus.Error]: { tone: 'error', text: '异常' },
  [PluginLifecycleStatus.Inactive]: { tone: 'neutral', text: '已停止' }
}

export default function LauncherCard({
  plugin,
  status,
  onToggle,
  onDelete,
  onConfigure,
  onOpen,
  sortable,
  selectable = false,
  selected = false,
  onSelectToggle
}: LauncherCardProps) {
  const { token } = theme.useToken()
  const [hovered, setHovered] = useState(false)
  const identity = pluginIdentity(plugin.name, plugin.author)

  const statusMeta = status
    ? (STATUS_META[status] ?? { tone: 'neutral' as const, text: status })
    : plugin.enabled
      ? STATUS_META[PluginLifecycleStatus.Active]
      : STATUS_META[PluginLifecycleStatus.Inactive]
  const statusColor =
    statusMeta.tone === 'success'
      ? token.colorSuccess
      : statusMeta.tone === 'warning'
        ? token.colorWarning
        : statusMeta.tone === 'error'
          ? token.colorError
          : token.colorTextTertiary

  const isDragState = sortable?.isDragging || sortable?.isSorting
  const effectiveHovered = hovered && !isDragState
  const transition = sortable?.isDragging
    ? 'none'
    : (sortable?.dragStyle?.transition ?? 'all 0.2s ease')
  const cardStyle: CSSProperties = {
    cursor: sortable?.isDragging ? 'grabbing' : 'pointer',
    position: 'relative',
    padding: 18,
    borderRadius: token.borderRadius,
    background: token.colorBgContainer,
    border: `1px solid ${effectiveHovered ? token.colorPrimary : token.colorBorder}`,
    boxShadow: 'none',
    transform: effectiveHovered ? 'translateY(-1px)' : 'translateY(0)',
    transition,
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    overflow: 'hidden',
    opacity: sortable?.isDragging ? 0.4 : 1,
    pointerEvents: sortable?.isDragging ? 'none' : 'auto',
    ...sortable?.dragStyle
  }

  const handleOpen = () => {
    if (sortable?.isDragging || sortable?.isSorting) return
    if (selectable) {
      onSelectToggle?.(plugin.id)
      return
    }
    onOpen(plugin)
  }

  return (
    <article
      ref={sortable?.setNodeRef}
      className="ob-launcher"
      data-module={plugin.name.toUpperCase().slice(0, 12)}
      data-dragging={sortable?.isDragging}
      data-sorting={sortable?.isSorting}
      data-selected={selectable && selected}
      {...(selectable ? {} : (sortable?.dragAttributes ?? {}))}
      {...(selectable ? {} : (sortable?.dragListeners ?? {}))}
      role="listitem"
      aria-selected={selectable ? selected : undefined}
      aria-posinset={sortable ? sortable.index + 1 : undefined}
      aria-setsize={sortable ? sortable.total : undefined}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={cardStyle}
    >
      {/* 批量勾选标记 */}
      {selectable && (
        <div
          aria-hidden="true"
          style={{
            position: 'absolute',
            top: 8,
            left: 8,
            zIndex: 3,
            color: selected ? token.colorPrimary : token.colorTextTertiary,
            fontSize: 20,
            lineHeight: 1
          }}
        >
          {selected ? (
            <CheckCircleFilled />
          ) : (
            <span
              style={{
                display: 'inline-block',
                width: 16,
                height: 16,
                border: `2px solid ${token.colorBorder}`,
                borderRadius: '50%'
              }}
            />
          )}
        </div>
      )}
      <button
        className="ob-launcher-open"
        type="button"
        aria-label={
          selectable
            ? `${selected ? '取消选择' : '选择'} ${plugin.displayName}`
            : `打开 ${plugin.displayName}，${statusMeta.text}`
        }
        aria-pressed={selectable ? selected : undefined}
        onClick={handleOpen}
        style={{
          position: 'absolute',
          inset: 0,
          zIndex: 1,
          border: 0,
          background: 'transparent',
          cursor: sortable?.isDragging ? 'grabbing' : 'pointer'
        }}
      />
      {(onToggle || onConfigure || onDelete || sortable) && !selectable && (
        <div
          className="ob-launcher-actions"
          onPointerDown={(e) => e.stopPropagation()}
          style={{
            position: 'absolute',
            top: 8,
            right: 8,
            display: 'flex',
            gap: 2,
            zIndex: 2
          }}
        >
          {sortable && (
            <>
              <Tooltip title="上移">
                <Button
                  className="ob-launcher-reorder-btn ob-launcher-reorder-up"
                  data-ob-kind="text"
                  aria-label={`将 ${plugin.displayName} 上移`}
                  type="text"
                  size="small"
                  disabled={sortable.index === 0}
                  icon={<ArrowUpOutlined />}
                  onClick={(e) => {
                    e.stopPropagation()
                    sortable.onMoveUp?.()
                  }}
                />
              </Tooltip>
              <Tooltip title="下移">
                <Button
                  className="ob-launcher-reorder-btn ob-launcher-reorder-down"
                  data-ob-kind="text"
                  aria-label={`将 ${plugin.displayName} 下移`}
                  type="text"
                  size="small"
                  disabled={sortable.index === sortable.total - 1}
                  icon={<ArrowDownOutlined />}
                  onClick={(e) => {
                    e.stopPropagation()
                    sortable.onMoveDown?.()
                  }}
                />
              </Tooltip>
            </>
          )}
          {onToggle && (
            <Tooltip title={plugin.enabled ? '禁用' : '启用'}>
              <Button
                data-ob-kind="text"
                aria-label={
                  plugin.enabled ? `禁用 ${plugin.displayName}` : `启用 ${plugin.displayName}`
                }
                type="text"
                size="small"
                icon={<PlayCircleOutlined />}
                onClick={(e) => {
                  e.stopPropagation()
                  onToggle(plugin.id, !plugin.enabled)
                }}
              />
            </Tooltip>
          )}
          {onConfigure && (
            <Tooltip title="配置">
              <Button
                data-ob-kind="text"
                aria-label={`配置 ${plugin.displayName}`}
                type="text"
                size="small"
                icon={<SettingOutlined />}
                onClick={(e) => {
                  e.stopPropagation()
                  onConfigure(plugin)
                }}
              />
            </Tooltip>
          )}
          {onDelete && (
            <Popconfirm
              title="确认删除此插件？"
              onConfirm={(e) => {
                e?.stopPropagation()
                onDelete(plugin.id)
              }}
              okText="确认"
              cancelText="取消"
            >
              <Tooltip title="删除">
                <Button
                  data-ob-kind="danger"
                  aria-label={`删除 ${plugin.displayName}`}
                  type="text"
                  size="small"
                  danger
                  icon={<DeleteOutlined />}
                  onClick={(e) => e.stopPropagation()}
                />
              </Tooltip>
            </Popconfirm>
          )}
        </div>
      )}

      <div
        aria-hidden="true"
        style={{
          display: 'flex',
          flex: 1,
          flexDirection: 'column',
          minHeight: 0,
          pointerEvents: 'none'
        }}
      >
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
          <PluginGlyph
            pluginId={plugin.name}
            name={plugin.displayName || plugin.name}
            icon={plugin.icon}
            size={54}
          />
          <div style={{ minWidth: 0, flex: 1, paddingTop: 2 }}>
            <div
              style={{
                fontWeight: 650,
                fontSize: 15,
                color: token.colorText,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap'
              }}
            >
              {plugin.displayName}
            </div>
            <div style={{ display: 'flex', gap: 6, marginTop: 5, flexWrap: 'wrap' }}>
              <span
                style={{
                  padding: '2px 7px',
                  borderRadius: 999,
                  color: identity.accent,
                  background: `color-mix(in srgb, ${identity.accent} 11%, transparent)`,
                  fontSize: 10,
                  fontWeight: 600
                }}
              >
                {identity.category}
              </span>
              {isOfficialPlugin(plugin.name) && (
                <span style={{ color: token.colorTextTertiary, fontSize: 10 }}>官方</span>
              )}
            </div>
          </div>
        </div>
        <div style={{ fontSize: 11, color: token.colorTextTertiary, marginTop: 12 }}>
          {identity.publisher} · v{plugin.version}
        </div>
        <div
          style={{
            marginTop: 8,
            fontSize: 12,
            color: token.colorTextSecondary,
            lineHeight: 1.5,
            display: '-webkit-box',
            WebkitLineClamp: 2,
            WebkitBoxOrient: 'vertical',
            overflow: 'hidden',
            flex: 1,
            // 悬浮显示完整简介（1.9.12）：简介区提升为可交互层（父层 pointerEvents:none）
            position: 'relative',
            zIndex: 2,
            pointerEvents: 'auto'
          }}
          onClick={() => handleOpen()}
        >
          <Tooltip
            title={plugin.description || '暂无描述'}
            placement="bottomLeft"
            mouseEnterDelay={0.35}
            overlayStyle={{ maxWidth: 340 }}
          >
            <span style={{ cursor: 'help' }}>{plugin.description || '暂无描述'}</span>
          </Tooltip>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 12 }}>
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              background: statusColor,
              border: `1px solid color-mix(in srgb, ${statusColor} 70%, transparent)`
            }}
          />
          <span style={{ fontSize: 12, color: token.colorTextTertiary }}>{statusMeta.text}</span>
        </div>
      </div>
    </article>
  )
}

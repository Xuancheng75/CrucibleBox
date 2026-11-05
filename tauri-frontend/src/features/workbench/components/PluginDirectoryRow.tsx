import { Checkbox, Dropdown } from 'antd'
import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'

export interface PluginDirectoryRowProps {
  id: string
  reorderDisabled: boolean
  displayName: string
  enabled: boolean
  previewed: boolean
  batchMode: boolean
  selected: boolean
  onSelect: () => void
  onPreview: () => void
  onOpen: () => void
  onToggle: () => void
  onUninstall: () => void
}

export default function PluginDirectoryRow({
  id,
  reorderDisabled,
  displayName,
  enabled,
  previewed,
  batchMode,
  selected,
  onSelect,
  onPreview,
  onOpen,
  onToggle,
  onUninstall
}: PluginDirectoryRowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id,
    disabled: reorderDisabled
  })
  return (
    <Dropdown
      trigger={['contextMenu']}
      menu={{
        items: [
          {
            key: 'toggle',
            label: enabled ? '停用' : '启用',
            disabled: reorderDisabled,
            onClick: onToggle
          },
          { type: 'divider' },
          {
            key: 'uninstall',
            label: '卸载',
            danger: true,
            disabled: reorderDisabled,
            onClick: onUninstall
          }
        ]
      }}
    >
      <div
        ref={setNodeRef}
        style={{
          transform: CSS.Transform.toString(transform),
          transition,
          position: 'relative',
          zIndex: isDragging ? 2 : undefined,
          opacity: isDragging ? 0.75 : 1
        }}
        className={previewed ? 'cbx-directory-row is-previewed' : 'cbx-directory-row'}
        role="listitem"
      >
        {batchMode && (
          <Checkbox checked={selected} onChange={onSelect} aria-label={'选择 ' + displayName} />
        )}
        <button
          {...attributes}
          {...listeners}
          type="button"
          className="cbx-directory-name"
          title="单击预览，双击打开，长按拖动排序"
          onClick={() => {
            if (!isDragging) onPreview()
          }}
          onDoubleClick={() => {
            if (!isDragging) onOpen()
          }}
          onKeyDown={(event) => {
            listeners?.onKeyDown?.(event)
            if (!event.defaultPrevented && event.key === 'Enter') onOpen()
          }}
          aria-label={'预览 ' + displayName + '，双击打开'}
          aria-pressed={previewed}
        >
          <span>{displayName}</span>
          {!enabled && <small>已停用</small>}
        </button>
      </div>
    </Dropdown>
  )
}

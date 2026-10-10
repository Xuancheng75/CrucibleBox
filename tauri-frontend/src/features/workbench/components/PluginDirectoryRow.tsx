import { Checkbox } from 'antd'
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
  onOpen
}: PluginDirectoryRowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id,
    disabled: reorderDisabled
  })
  return (
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
      >
        <span>{displayName}</span>
        {!enabled && <small>已停用</small>}
      </button>
    </div>
  )
}

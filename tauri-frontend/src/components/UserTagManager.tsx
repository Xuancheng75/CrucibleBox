import { useCallback, useEffect, useState } from 'react'
import { App, Button, Checkbox, Input, Modal, Select, Space, Tag } from 'antd'
import { tauriApi, type UserPluginTag } from '../api/tauriApi'
import type { PluginMeta } from '../../../shared/types/plugin.types'

interface Props {
  plugins: PluginMeta[]
  selectedPluginIds: string[]
  onFilterChange: (pluginIds: string[] | null) => void
}

export default function UserTagManager({ plugins, selectedPluginIds, onFilterChange }: Props) {
  const { message } = App.useApp()
  const [tags, setTags] = useState<UserPluginTag[]>([])
  const [selectedTagIds, setSelectedTagIds] = useState<number[]>([])
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [editingId, setEditingId] = useState<number | null>(null)
  const [targetIds, setTargetIds] = useState<string[]>([])
  const [assignedIds, setAssignedIds] = useState<number[]>([])

  const reload = useCallback(async () => {
    setTags(await tauriApi.plugin.tags.list())
  }, [])
  useEffect(() => { void reload().catch(() => message.error('读取标签失败')) }, [reload, message])
  useEffect(() => {
    if (selectedTagIds.length === 0) {
      onFilterChange(null)
      return
    }
    const ids = new Set(tags.filter((tag) => selectedTagIds.includes(tag.id)).flatMap((tag) => tag.pluginIds))
    onFilterChange([...ids])
  }, [tags, selectedTagIds, onFilterChange])

  const run = async (operation: () => Promise<unknown>) => {
    try {
      await operation()
      await reload()
      setName('')
      setEditingId(null)
    } catch (error) {
      message.error(error instanceof Error ? error.message : String(error))
    }
  }

  return (
    <div style={{ marginBottom: 16 }}>
      <Space wrap>
        <span>标签</span>
        {tags.map((tag) => (
          <Tag.CheckableTag
            key={tag.id}
            checked={selectedTagIds.includes(tag.id)}
            onChange={(checked) => setSelectedTagIds((current) =>
              checked ? [...current, tag.id] : current.filter((id) => id !== tag.id)
            )}
          >
            {tag.name} ({tag.pluginIds.length})
          </Tag.CheckableTag>
        ))}
        {selectedTagIds.length > 0 && <Button size="small" onClick={() => setSelectedTagIds([])}>清除筛选</Button>}
        <Button size="small" onClick={() => { setTargetIds(selectedPluginIds); setOpen(true) }}>管理标签</Button>
      </Space>
      <Modal title="管理插件标签" open={open} onCancel={() => setOpen(false)} footer={null}>
        <p>标签用于分类和筛选插件，不会修改插件名称。可为一个插件分配多个标签。</p>
        <Space.Compact style={{ width: '100%', marginBottom: 14 }}>
          <Input value={name} onChange={(event) => setName(event.target.value)} placeholder={editingId ? '新的标签名称' : '新建标签'} maxLength={32} />
          <Button type="primary" disabled={!name.trim()} onClick={() => void run(() => editingId === null ? tauriApi.plugin.tags.create(name) : tauriApi.plugin.tags.rename(editingId, name))}>
            {editingId === null ? '创建' : '保存改名'}
          </Button>
        </Space.Compact>
        <div style={{ maxHeight: 180, overflow: 'auto', marginBottom: 16 }}>
          {tags.map((tag) => (
            <Space key={tag.id} style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
              <span>{tag.name} · {tag.pluginIds.length} 个插件</span>
              <Space>
                <Button size="small" onClick={() => { setEditingId(tag.id); setName(tag.name) }}>改名</Button>
                <Button size="small" danger onClick={() => void run(() => tauriApi.plugin.tags.delete(tag.id))}>删除</Button>
              </Space>
            </Space>
          ))}
        </div>
        <Select
          mode="multiple"
          style={{ width: '100%', marginBottom: 12 }}
          placeholder="选择需要设置标签的插件"
          value={targetIds}
          onChange={setTargetIds}
          options={plugins.map((plugin) => ({ value: plugin.id, label: plugin.displayName }))}
        />
        <Checkbox.Group
          value={assignedIds}
          onChange={(values) => setAssignedIds(values.map(Number))}
          options={tags.map((tag) => ({ label: tag.name, value: tag.id }))}
        />
        <div style={{ marginTop: 16 }}>
          <Button type="primary" disabled={targetIds.length === 0} onClick={() => void run(() => tauriApi.plugin.tags.assign(targetIds, assignedIds))}>
            应用到所选插件
          </Button>
        </div>
      </Modal>
    </div>
  )
}

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Table, App, Tag, Typography, Select, Space, Button, Popconfirm, Tooltip } from 'antd'
import { ClearOutlined } from '@ant-design/icons'
import { theme } from 'antd'
import { tauriApi } from '../api/tauriApi'
import { usePlugins } from '../hooks/usePlugins'
import { usePluginLog } from '../hooks/useIpc'
import type { PluginLogEntry } from '../../../shared/types/plugin.types'

const { Title, Text } = Typography

export default function PluginLogs() {
  const { token } = theme.useToken()
  const { message } = App.useApp()
  const { plugins } = usePlugins()
  const [logs, setLogs] = useState<PluginLogEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [pluginId, setPluginId] = useState<string | undefined>()
  const [level, setLevel] = useState<string | undefined>()
  const nextIdRef = useRef(Date.now())

  const fetchLogs = useCallback(async () => {
    setLoading(true)
    try {
      const res = await tauriApi.plugin.getLogs({ pluginId, level, limit: 500 })
      if (res.success && res.data) {
        setLogs(res.data)
      } else {
        message.error(res.error ?? '加载日志失败')
      }
    } catch (err) {
      message.error(`加载日志失败：${err instanceof Error ? err.message : String(err)}`)
    } finally {
      setLoading(false)
    }
  }, [pluginId, level, message])

  useEffect(() => {
    fetchLogs()
  }, [fetchLogs])

  // 实时日志订阅（plugin:log 事件）
  usePluginLog(
    useCallback(
      (data) => {
        if (pluginId && data.pluginId !== pluginId) return
        if (level && data.level !== level) return
        const entry: PluginLogEntry = {
          id: nextIdRef.current++,
          pluginId: data.pluginId,
          level: data.level as PluginLogEntry['level'],
          message: data.message,
          timestamp: new Date().toLocaleString()
        }
        setLogs((prev) => [entry, ...prev].slice(0, 500))
      },
      [pluginId, level]
    )
  )

  const pluginName = useMemo(() => {
    const map = new Map(plugins.map((p) => [p.id, p.displayName]))
    return (id: string) => map.get(id) ?? id
  }, [plugins])

  const handleClear = async (id?: string) => {
    try {
      const res = await tauriApi.plugin.clearLogs(id)
      if (res.success) {
        message.success('日志已清除')
        fetchLogs()
      } else {
        message.error(res.error ?? '清除失败')
      }
    } catch {
      message.error('清除日志失败')
    }
  }

  const columns = [
    {
      title: '时间',
      dataIndex: 'timestamp',
      width: 160,
      render: (v: string) => (
        <Text style={{ fontSize: 12, color: token.colorTextSecondary }}>{v}</Text>
      )
    },
    {
      title: '插件',
      dataIndex: 'pluginId',
      width: 160,
      render: (id: string) => (
        <Text strong style={{ fontSize: 13 }}>
          {pluginName(id)}
        </Text>
      )
    },
    {
      title: '级别',
      dataIndex: 'level',
      width: 90,
      render: (v: string) => {
        const color =
          v === 'error'
            ? token.colorError
            : v === 'warn'
              ? token.colorWarning
              : v === 'info'
                ? token.colorPrimary
                : token.colorTextTertiary
        const background =
          v === 'error'
            ? token.colorErrorBg
            : v === 'warn'
              ? token.colorWarningBg
              : v === 'info'
                ? token.colorPrimaryBg
                : token.colorFillTertiary
        return (
          <Tag
            className={`ob-tone-${v}`}
            style={{ fontSize: 11, color, background, borderColor: color }}
          >
            {v.toUpperCase()}
          </Tag>
        )
      }
    },
    {
      title: '消息',
      dataIndex: 'message',
      render: (v: string) => (
        <Tooltip title={v}>
          <span
            style={{
              fontFamily: 'monospace',
              fontSize: 12,
              color: token.colorText,
              display: 'block',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              maxWidth: '100%'
            }}
          >
            {v}
          </span>
        </Tooltip>
      )
    }
  ]

  return (
    <div>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: 24,
          flexWrap: 'wrap',
          gap: 12
        }}
      >
        <div>
          <Title level={4} style={{ margin: 0, fontWeight: 600 }}>
            插件日志
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            查看和筛选插件运行日志
          </Text>
        </div>
        <Space wrap>
          <Select
            allowClear
            placeholder="全部插件"
            style={{ width: 180 }}
            value={pluginId}
            onChange={setPluginId}
            options={plugins.map((p) => ({ label: p.displayName, value: p.id }))}
          />
          <Select
            allowClear
            placeholder="全部级别"
            style={{ width: 130 }}
            value={level}
            onChange={setLevel}
            options={['debug', 'info', 'warn', 'error'].map((l) => ({
              label: l.toUpperCase(),
              value: l
            }))}
          />
          <Button onClick={fetchLogs}>刷新</Button>
          <Popconfirm
            title={pluginId ? '清除当前插件日志？' : '清除全部日志？'}
            onConfirm={() => handleClear(pluginId)}
          >
            <Button data-ob-kind="danger" danger icon={<ClearOutlined />}>
              清除日志
            </Button>
          </Popconfirm>
        </Space>
      </div>

      <Table
        rowKey="id"
        columns={columns}
        dataSource={logs}
        loading={loading}
        size="small"
        pagination={{ pageSize: 50, showSizeChanger: false, showTotal: (t) => `共 ${t} 条` }}
        locale={{ emptyText: '暂无日志' }}
        style={{ borderTop: `1px solid ${token.colorBorderSecondary}` }}
      />
    </div>
  )
}
import { Button, Empty, Progress, Space, Tabs, Tag, Typography, theme } from 'antd'
import { CheckCircleOutlined, ClearOutlined, ClockCircleOutlined } from '@ant-design/icons'
import { useTaskStore, type HostTaskStatus } from '../store/task.store'
import { useAppStore } from '../store/app.store'
import PluginLogs from './PluginLogs'

const { Title, Text } = Typography

const STATUS_META: Record<HostTaskStatus, { label: string; color: string }> = {
  queued: { label: '等待中', color: 'default' },
  running: { label: '进行中', color: 'processing' },
  completed: { label: '已完成', color: 'success' },
  failed: { label: '失败', color: 'error' },
  cancelled: { label: '已取消', color: 'warning' }
}

export default function TaskCenter() {
  const { token } = theme.useToken()
  const tasks = useTaskStore((state) => state.tasks)
  const clearCompleted = useTaskStore((state) => state.clearCompleted)
  const activityTab = useAppStore((state) => state.activityTab)
  const setActivityTab = useAppStore((state) => state.setActivityTab)

  const taskList = tasks.length === 0 ? (
    <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="当前没有后台任务" />
  ) : (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      {tasks.map((task) => {
        const meta = STATUS_META[task.status]
        return (
          <article
            key={task.id}
            className="ob-task-card ob-surface-card"
            style={{
              padding: 16,
              border: `1px solid ${token.colorBorder}`,
              borderRadius: token.borderRadius,
              background: token.colorBgContainer
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              {task.status === 'completed' ? (
                <CheckCircleOutlined style={{ color: token.colorSuccess }} />
              ) : (
                <ClockCircleOutlined style={{ color: token.colorPrimary }} />
              )}
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontWeight: 600 }}>{task.title}</div>
                {task.detail && (
                  <div style={{ color: token.colorTextSecondary, fontSize: 12, marginTop: 3 }}>
                    {task.detail}
                  </div>
                )}
              </div>
              <Tag color={meta.color}>{meta.label}</Tag>
            </div>
            {typeof task.progress === 'number' && (
              <Progress
                percent={Math.max(0, Math.min(100, task.progress))}
                status={task.status === 'failed' ? 'exception' : undefined}
                size="small"
                style={{ marginTop: 12 }}
              />
            )}
            {task.error && (
              <div style={{ color: token.colorError, fontSize: 12, marginTop: 8 }}>
                {task.error}
              </div>
            )}
          </article>
        )
      })}
    </Space>
  )

  return (
    <div>
      <div className="ob-page-heading">
        <div>
          <Title level={3} style={{ margin: 0 }}>
            任务中心
          </Title>
          <Text type="secondary">安装、更新和插件长任务集中在这里</Text>
        </div>
        <Button icon={<ClearOutlined />} onClick={clearCompleted} disabled={tasks.length === 0}>
          清理已完成
        </Button>
      </div>

      <Tabs
        activeKey={activityTab}
        onChange={(key) => setActivityTab(key as 'tasks' | 'logs')}
        items={[
          { key: 'tasks', label: `任务${tasks.length > 0 ? ` (${tasks.length})` : ''}`, children: taskList },
          { key: 'logs', label: '运行日志', children: <PluginLogs embedded /> }
        ]}
      />
    </div>
  )
}

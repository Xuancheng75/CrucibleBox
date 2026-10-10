import { useEffect, useState } from 'react'
import { App, Button, Empty, Progress, Space, Tabs, Tag, Typography, theme } from 'antd'
import { CheckCircleOutlined, ClearOutlined, ClockCircleOutlined } from '@ant-design/icons'
import { useTaskStore, type HostTaskStatus } from '../store/task.store'
import { useAppStore } from '../store/app.store'
import { usePluginStore } from '../store/plugin.store'
import PluginLogs from './PluginLogs'
import { tauriApi } from '../api/tauriApi'

const { Title, Text } = Typography

const STATUS_META: Record<HostTaskStatus, { label: string; color: string }> = {
  queued: { label: '等待中', color: 'default' },
  running: { label: '进行中', color: 'processing' },
  paused: { label: '已暂停', color: 'warning' },
  'waiting-user': { label: '等待确认', color: 'gold' },
  completed: { label: '已完成', color: 'success' },
  failed: { label: '失败', color: 'error' },
  cancelled: { label: '已取消', color: 'warning' }
}

export default function TaskCenter() {
  const { token } = theme.useToken()
  const { message } = App.useApp()
  const [cancelling, setCancelling] = useState<Set<string>>(new Set())
  const tasks = useTaskStore((state) => state.tasks)
  const clearCompleted = useTaskStore((state) => state.clearCompleted)
  const removeTask = useTaskStore((state) => state.removeTask)
  const activityTab = useAppStore((state) => state.activityTab)
  const setActivityTab = useAppStore((state) => state.setActivityTab)
  const plugins = usePluginStore((state) => state.plugins)
  const goToTaskOwner = (owner?: string, source?: string) => {
    if (source === 'marketplace') {
      useAppStore.getState().setCurrentPage('marketplace')
      return
    }
    const plugin = plugins.find((item) => item.id === owner || item.name === owner)
    if (!plugin) {
      message.info('对应工具已移除，请先到插件目录重新安装')
      return
    }
    useAppStore.getState().setActivePluginId(plugin.id)
    useAppStore.getState().setCurrentPage('pluginView')
  }

  useEffect(() => {
    setCancelling((current) => {
      const next = new Set(
        [...current].filter((id) =>
          tasks.some(
            (task) => task.id === id && ['queued', 'running', 'paused'].includes(task.status)
          )
        )
      )
      return next.size === current.size ? current : next
    })
  }, [tasks])

  const cancelTask = async (id: string) => {
    setCancelling((current) => new Set(current).add(id))
    let accepted = false
    try {
      accepted = await tauriApi.hostTasks.cancel(id)
      if (!accepted) {
        const snapshots = await tauriApi.hostTasks.list()
        for (const snapshot of snapshots) useTaskStore.getState().mergeSnapshot(snapshot)
        message.info('任务已结束或无法取消，请检查最新状态')
      }
    } catch (error) {
      message.error(error instanceof Error ? error.message : String(error))
    } finally {
      if (!accepted)
        setCancelling((current) => {
          const next = new Set(current)
          next.delete(id)
          return next
        })
    }
  }

  const taskList =
    tasks.length === 0 ? (
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
                {['failed', 'paused', 'waiting-user'].includes(task.status) &&
                  (task.source === 'marketplace' ||
                    (task.owner &&
                      plugins.some(
                        (plugin) => plugin.id === task.owner || plugin.name === task.owner
                      ))) && (
                    <Button size="small" onClick={() => goToTaskOwner(task.owner, task.source)}>
                      前往处理
                    </Button>
                  )}
                {['completed', 'failed', 'cancelled'].includes(task.status) && (
                  <Button size="small" onClick={() => removeTask(task.id)}>
                    删除
                  </Button>
                )}
                {['queued', 'running', 'paused'].includes(task.status) &&
                  task.stage !== 'recoverable' &&
                  (task.source === 'marketplace' ||
                    ['document-engine', 'unienv'].includes(task.owner ?? '')) && (
                    <Button
                      size="small"
                      danger
                      disabled={cancelling.has(task.id)}
                      onClick={() => void cancelTask(task.id)}
                    >
                      {cancelling.has(task.id) ? '正在取消…' : '取消'}
                    </Button>
                  )}
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
              {!!task.resultRefs?.length && (
                <Space wrap style={{ marginTop: 8 }}>
                  {task.resultRefs.map((path) => (
                    <Button
                      key={path}
                      size="small"
                      title={path}
                      onClick={() =>
                        void tauriApi.hostTasks
                          .revealResult(task.id, path)
                          .catch((error: unknown) =>
                            message.error(error instanceof Error ? error.message : String(error))
                          )
                      }
                    >
                      查看结果位置
                    </Button>
                  ))}
                </Space>
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
        <Button
          icon={<ClearOutlined />}
          onClick={clearCompleted}
          disabled={
            !tasks.some((task) => ['completed', 'failed', 'cancelled'].includes(task.status))
          }
        >
          清理已结束
        </Button>
      </div>

      <Tabs
        activeKey={activityTab}
        onChange={(key) => setActivityTab(key as 'tasks' | 'logs')}
        items={[
          {
            key: 'tasks',
            label: `任务${tasks.length > 0 ? ` (${tasks.length})` : ''}`,
            children: taskList
          },
          { key: 'logs', label: '运行日志', children: <PluginLogs embedded /> }
        ]}
      />
    </div>
  )
}

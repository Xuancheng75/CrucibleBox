import { Button, Typography, Alert, Tag, theme } from 'antd'
import { ArrowLeftOutlined } from '@ant-design/icons'
import { PluginHost } from './PluginHost'
import { usePluginStore } from '../store/plugin.store'
import { useAppStore } from '../store/app.store'
import { PluginLifecycleStatus } from '@shared/types/plugin.types'

const { Title } = Typography

type StatusTone = 'success' | 'warning' | 'error' | 'neutral'

const STATUS_TEXT: Record<string, { text: string; tone: StatusTone }> = {
  [PluginLifecycleStatus.Active]: { text: '运行中', tone: 'success' },
  [PluginLifecycleStatus.Activating]: { text: '启动中', tone: 'warning' },
  [PluginLifecycleStatus.Deactivating]: { text: '停用中', tone: 'warning' },
  [PluginLifecycleStatus.Error]: { text: '异常', tone: 'error' },
  [PluginLifecycleStatus.Inactive]: { text: '已停止', tone: 'neutral' }
}

export default function PluginView() {
  const { token } = theme.useToken()
  const { activePluginId, setActivePluginId, setCurrentPage } = useAppStore()
  const plugins = usePluginStore((s) => s.plugins)
  const activePlugins = usePluginStore((s) => s.activePlugins)
  const updatePluginConfig = usePluginStore((s) => s.updatePluginConfig)

  const plugin = plugins.find((p) => p.id === activePluginId)

  const handleBack = () => {
    setActivePluginId(null)
    setCurrentPage('home')
  }

  if (!plugin) {
    return (
      <Alert
        className="ob-alert-warning"
        type="warning"
        title="插件未找到"
        description="该插件可能已被删除"
        showIcon
        action={<Button onClick={handleBack}>返回</Button>}
      />
    )
  }

  if (!plugin.enabled) {
    return (
      <div>
        <Button
          type="text"
          icon={<ArrowLeftOutlined />}
          onClick={handleBack}
          style={{ marginBottom: 16, color: token.colorTextSecondary }}
        >
          返回
        </Button>
        <Alert
          className="ob-alert-info"
          type="info"
          title={`"${plugin.displayName}" 未启用`}
          description="请先在插件列表中启用该插件"
          showIcon
        />
      </div>
    )
  }

  const statusMeta = STATUS_TEXT[activePlugins[plugin.id] as string] ?? {
    text: '',
    tone: 'neutral' as const
  }
  const statusColor =
    statusMeta.tone === 'success'
      ? token.colorSuccess
      : statusMeta.tone === 'warning'
        ? token.colorWarning
        : statusMeta.tone === 'error'
          ? token.colorError
          : token.colorTextTertiary
  const statusBackground =
    statusMeta.tone === 'success'
      ? token.colorSuccessBg
      : statusMeta.tone === 'warning'
        ? token.colorWarningBg
        : statusMeta.tone === 'error'
          ? token.colorErrorBg
          : token.colorFillTertiary

  return (
    <div>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          marginBottom: 20,
          paddingBottom: 16,
          borderBottom: `1px solid ${token.colorBorderSecondary}`
        }}
      >
        <Button
          type="text"
          icon={<ArrowLeftOutlined />}
          onClick={handleBack}
          style={{ color: token.colorTextSecondary, marginRight: 12 }}
        >
          返回
        </Button>
        <Title level={4} style={{ margin: 0, fontWeight: 600, color: token.colorText }}>
          {plugin.displayName}
        </Title>
        {statusMeta.text && (
          <Tag
            className={`ob-tone-${statusMeta.tone}`}
            style={{
              marginLeft: 12,
              color: statusColor,
              background: statusBackground,
              borderColor: statusColor
            }}
          >
            {statusMeta.text}
          </Tag>
        )}
      </div>

      <PluginHost
        pluginId={plugin.id}
        pluginName={plugin.name}
        rendererEntry={plugin.entryRenderer}
        config={plugin.configData || {}}
        permissions={plugin.permissions}
        onConfigChange={(config) => updatePluginConfig(plugin.id, config)}
      />
    </div>
  )
}

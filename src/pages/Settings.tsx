import { useEffect, useState } from 'react'
import {
  Typography,
  Descriptions,
  Tag,
  Card,
  theme as antdTheme,
  Button,
  Select,
  Progress,
  Space,
  Alert
} from 'antd'
import { SettingOutlined } from '@ant-design/icons'
import type { AppUpdateChannel, AppUpdateState } from '@shared/types/ipc.types'

const { Title, Text } = Typography

export default function Settings() {
  const { token } = antdTheme.useToken()
  const [version, setVersion] = useState('')
  const [platform, setPlatform] = useState('')
  const [updateState, setUpdateState] = useState<AppUpdateState | null>(null)
  const descriptionStyles = {
    label: {
      background: token.colorBgLayout,
      width: 140,
      color: token.colorTextSecondary,
      fontWeight: 500
    },
    content: { background: token.colorBgContainer, color: token.colorText }
  }
  const descriptionItems = [
    { key: 'name', label: '应用名称', children: <Text strong>CrucibleBox</Text> },
    { key: 'version', label: '版本', children: <Tag>{version || '...'}</Tag> },
    { key: 'platform', label: '运行平台', children: <Tag>{platform || '...'}</Tag> },
    {
      key: 'plugins',
      label: '插件目录',
      children: (
        <Text type="secondary" style={{ fontSize: 12 }}>
          %APPDATA%/OpenBox/plugins/
        </Text>
      )
    }
  ]

  useEffect(() => {
    window.electronAPI?.app
      .getVersion()
      .then(setVersion)
      .catch(() => {})
    window.electronAPI?.app
      .getPlatform()
      .then(setPlatform)
      .catch(() => {})
    window.electronAPI?.app.update
      .getState()
      .then(setUpdateState)
      .catch(() => {})
    return window.electronAPI?.app.update?.onChanged(setUpdateState)
  }, [])

  const runUpdateAction = async (action: () => Promise<AppUpdateState>) => {
    try {
      setUpdateState(await action())
    } catch (error) {
      setUpdateState((current) =>
        current
          ? {
              ...current,
              phase: 'error',
              message: error instanceof Error ? error.message : String(error)
            }
          : current
      )
    }
  }

  const updateBusy = updateState?.phase === 'checking' || updateState?.phase === 'downloading'
  const updateStatus =
    updateState &&
    ({
      disabled: '在线更新未启用',
      idle: '尚未检查',
      checking: '正在检查更新',
      available: `发现 ${updateState.availableVersion ?? '新版本'}`,
      downloading: '正在下载',
      downloaded: `${updateState.availableVersion ?? '新版本'} 已就绪`,
      'not-available': '当前已是最新版本',
      error: '更新失败'
    } satisfies Record<AppUpdateState['phase'], string>)[updateState.phase]

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', marginBottom: 24 }}>
        <div
          style={{
            width: 40,
            height: 40,
            borderRadius: token.borderRadius,
            background: token.colorPrimaryBg,
            color: token.colorPrimary,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: 18,
            marginRight: 12
          }}
        >
          <SettingOutlined />
        </div>
        <div>
          <Title level={4} style={{ margin: 0, fontWeight: 600 }}>
            设置
          </Title>
          <Text type="secondary" style={{ fontSize: 13 }}>
            应用信息与运行环境
          </Text>
        </div>
      </div>

      <Card
        style={{
          border: `1px solid ${token.colorBorder}`,
          borderRadius: token.borderRadius,
          boxShadow: token.boxShadow
        }}
        styles={{ body: { padding: 24 } }}
      >
        <Descriptions
          column={1}
          bordered
          size="small"
          styles={descriptionStyles}
          items={descriptionItems}
        />
      </Card>

      <Card
        title="应用更新"
        style={{
          marginTop: 16,
          border: `1px solid ${token.colorBorder}`,
          borderRadius: token.borderRadius,
          boxShadow: token.boxShadow
        }}
        styles={{ body: { padding: 24 } }}
      >
        <Space orientation="vertical" size="middle" style={{ width: '100%' }}>
          <Alert
            type="warning"
            showIcon
            title="当前 Windows 安装包未使用 Authenticode 证书签名"
            description="首次安装或升级时，Windows 可能显示“未知发布者”或 SmartScreen 提示；更新仍会通过 HTTPS、版本元数据和 SHA-512 校验。"
          />
          <Space wrap>
            <Text>更新通道</Text>
            <Select<AppUpdateChannel>
              aria-label="更新通道"
              className="ob-update-channel-select"
              value={updateState?.channel ?? 'stable'}
              disabled={!updateState || updateState.phase === 'disabled' || updateBusy}
              options={[
                { value: 'stable', label: '稳定版' },
                { value: 'beta', label: '测试版' }
              ]}
              onChange={(channel) => {
                void runUpdateAction(() => window.electronAPI.app.update.setChannel(channel))
              }}
            />
            <Tag color={updateState?.phase === 'error' ? 'error' : 'blue'}>
              {updateStatus ?? '读取中'}
            </Tag>
          </Space>

          {updateState?.rollbackEligible && (
            <Alert
              type="warning"
              showIcon
              title="已从测试版切回稳定版；下一次检查允许降级到稳定通道。"
            />
          )}
          {updateState?.message && (
            <Alert type={updateState.phase === 'error' ? 'error' : 'info'} title={updateState.message} />
          )}
          {updateState?.phase === 'downloading' && (
            <Progress percent={Math.round(updateState.progressPercent ?? 0)} />
          )}

          <Space wrap>
            <Button
              loading={updateState?.phase === 'checking'}
              disabled={!updateState || updateState.phase === 'disabled' || updateBusy}
              onClick={() => void runUpdateAction(() => window.electronAPI.app.update.check())}
            >
              检查更新
            </Button>
            {updateState?.phase === 'available' && (
              <Button
                type="primary"
                onClick={() => void runUpdateAction(() => window.electronAPI.app.update.download())}
              >
                下载更新
              </Button>
            )}
            {updateState?.phase === 'downloaded' && (
              <Button
                type="primary"
                danger
                onClick={() => void runUpdateAction(() => window.electronAPI.app.update.install())}
              >
                重启并安装
              </Button>
            )}
          </Space>
        </Space>
      </Card>
    </div>
  )
}

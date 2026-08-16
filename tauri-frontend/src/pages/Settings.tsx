import { useCallback, useEffect, useState } from 'react'
import {
  Typography,
  Descriptions,
  Tag,
  Card,
  theme as antdTheme,
  Button,
  Select,
  Space,
  Alert
} from 'antd'
import { SettingOutlined, CheckCircleOutlined } from '@ant-design/icons'
import { check as checkUpdate } from '@tauri-apps/plugin-updater'
import { tauriApi } from '../api/tauriApi'

const { Title, Text } = Typography

type AppUpdateChannel = 'stable' | 'beta'

type UpdatePhase = 'idle' | 'checking' | 'available' | 'not-available' | 'error'

interface UpdateState {
  phase: UpdatePhase
  availableVersion: string | null
  message: string | null
}

const UPDATE_CHANNEL_KEY = 'ob-update-channel'

function loadChannel(): AppUpdateChannel {
  try {
    const saved = localStorage.getItem(UPDATE_CHANNEL_KEY)
    return saved === 'beta' ? 'beta' : 'stable'
  } catch {
    return 'stable'
  }
}

export default function Settings() {
  const { token } = antdTheme.useToken()
  const [version, setVersion] = useState('')
  const [platform, setPlatform] = useState('')
  const [updateState, setUpdateState] = useState<UpdateState>({
    phase: 'idle',
    availableVersion: null,
    message: null
  })
  const [channel, setChannel] = useState<AppUpdateChannel>(loadChannel)
  const [checking, setChecking] = useState(false)

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
          %APPDATA%/CrucibleBox/plugins/
        </Text>
      )
    }
  ]

  useEffect(() => {
    tauriApi.app
      .getVersion()
      .then(setVersion)
      .catch(() => {})
    tauriApi.app
      .getPlatform()
      .then(setPlatform)
      .catch(() => {})
  }, [])

  const handleCheck = useCallback(async () => {
    setChecking(true)
    setUpdateState({ phase: 'checking', availableVersion: null, message: null })
    try {
      const update = await checkUpdate()
      if (update === null) {
        setUpdateState({ phase: 'not-available', availableVersion: null, message: null })
      } else {
        setUpdateState({
          phase: 'available',
          availableVersion: update.version,
          message: update.date ? `发布于 ${update.date}` : null
        })
        // 1.9.3 最小接入：仅提示；完整下载/安装 UI 随 1.9.4 落地
        // await update.downloadAndInstall()
      }
    } catch (e) {
      setUpdateState({
        phase: 'error',
        availableVersion: null,
        message: e instanceof Error ? e.message : String(e)
      })
    } finally {
      setChecking(false)
    }
  }, [])

  const updateStatus =
    ({
      idle: '尚未检查',
      checking: '正在检查更新',
      available: `发现新版本 v${updateState.availableVersion ?? ''}`,
      'not-available': '当前已是最新版本',
      error: '更新检查失败'
    } satisfies Record<UpdatePhase, string>)[updateState.phase] ?? '读取中'

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
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Alert
            type="warning"
            showIcon
            message="当前 Windows 安装包未使用 Authenticode 证书签名"
            description="首次安装或升级时，Windows 可能显示“未知发布者”或 SmartScreen 提示；更新仍会通过 HTTPS、版本元数据和 SHA-512 校验。"
          />
          <Space wrap>
            <Text>更新通道</Text>
            <Select<AppUpdateChannel>
              aria-label="更新通道"
              className="ob-update-channel-select"
              value={channel}
              disabled={checking}
              options={[
                { value: 'stable', label: '稳定版' },
                { value: 'beta', label: '测试版' }
              ]}
              onChange={(next) => {
                setChannel(next)
                try {
                  localStorage.setItem(UPDATE_CHANNEL_KEY, next)
                } catch {
                  // localStorage 不可用时静默降级
                }
              }}
            />
            <Tag color={updateState.phase === 'error' ? 'error' : 'blue'}>
              {updateStatus}
            </Tag>
          </Space>

          {updateState.phase === 'available' && (
            <Alert
              type="success"
              showIcon
              icon={<CheckCircleOutlined />}
              message={`发现新版本 v${updateState.availableVersion ?? ''}`}
              description={
                updateState.message ?? '下载与安装流程将在 1.9.4 提供，当前仅提示可用更新。'
              }
            />
          )}
          {updateState.message && updateState.phase === 'error' && (
            <Alert type="error" message={updateState.message} />
          )}

          <Space wrap>
            <Button
              loading={checking}
              disabled={checking}
              onClick={() => void handleCheck()}
            >
              检查更新
            </Button>
          </Space>
        </Space>
      </Card>
    </div>
  )
}
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Typography,
  Descriptions,
  Tag,
  Card,
  theme as antdTheme,
  Button,
  Select,
  Space,
  Alert,
  Progress
} from 'antd'
import { SettingOutlined, CheckCircleOutlined } from '@ant-design/icons'
import { check as checkUpdate, Update, type DownloadEvent } from '@tauri-apps/plugin-updater'
import { tauriApi } from '../api/tauriApi'
import { formatUpdateError, retryUpdateCheck } from '../utils/update-check'

const { Title, Text } = Typography

type AppUpdateChannel = 'stable' | 'beta'

type UpdatePhase =
  'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'not-available' | 'error'

interface UpdateState {
  phase: UpdatePhase
  availableVersion: string | null
  progressPercent: number | null
  message: string | null
}

const UPDATE_CHANNEL_KEY = 'ob-update-channel'
const UPDATE_CHECK_TIMEOUT_MS = 12_000
const UPDATE_CHECK_ATTEMPTS = 2
const UPDATE_CHECK_RETRY_DELAY_MS = 800
const UPDATE_DOWNLOAD_TIMEOUT_MS = 60_000

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
    progressPercent: null,
    message: null
  })
  const [channel, setChannel] = useState<AppUpdateChannel>(loadChannel)
  const [checking, setChecking] = useState(false)
  const [downloading, setDownloading] = useState(false)
  const updateRef = useRef<Update | null>(null)
  const checkInFlightRef = useRef(false)

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

  useEffect(() => {
    return () => {
      const update = updateRef.current
      updateRef.current = null
      void update?.close().catch(() => {})
    }
  }, [])

  const handleCheck = useCallback(async () => {
    if (checkInFlightRef.current || downloading) return
    checkInFlightRef.current = true
    setChecking(true)
    const previousUpdate = updateRef.current
    updateRef.current = null
    void previousUpdate?.close().catch(() => {})
    setUpdateState({
      phase: 'checking',
      availableVersion: null,
      progressPercent: null,
      message: null
    })
    try {
      const update = await retryUpdateCheck(
        () => checkUpdate({ timeout: UPDATE_CHECK_TIMEOUT_MS }),
        {
          timeoutMs: UPDATE_CHECK_TIMEOUT_MS,
          maxAttempts: UPDATE_CHECK_ATTEMPTS,
          retryDelayMs: UPDATE_CHECK_RETRY_DELAY_MS
        }
      )
      if (update === null) {
        setUpdateState({
          phase: 'not-available',
          availableVersion: null,
          progressPercent: null,
          message: null
        })
      } else {
        updateRef.current = update
        setUpdateState({
          phase: 'available',
          availableVersion: update.version,
          progressPercent: null,
          message: update.date ? `发布于 ${update.date}` : null
        })
      }
    } catch (e) {
      setUpdateState({
        phase: 'error',
        availableVersion: null,
        progressPercent: null,
        message: formatUpdateError(e)
      })
    } finally {
      checkInFlightRef.current = false
      setChecking(false)
    }
  }, [downloading])

  const handleDownload = useCallback(async () => {
    const update = updateRef.current
    if (!update || downloading) return
    setDownloading(true)
    setUpdateState((current) => ({
      ...current,
      phase: 'downloading',
      progressPercent: 0,
      message: null
    }))
    try {
      let contentLength = 0
      let downloaded = 0
      await update.download(
        (event: DownloadEvent) => {
          if (event.event === 'Started') {
            contentLength = event.data.contentLength ?? 0
          } else if (event.event === 'Progress') {
            downloaded += event.data.chunkLength
            const percent =
              contentLength > 0
                ? Math.min(100, Math.round((downloaded / contentLength) * 100))
                : null
            setUpdateState((current) => ({ ...current, progressPercent: percent }))
          }
        },
        { timeout: UPDATE_DOWNLOAD_TIMEOUT_MS }
      )
      setUpdateState((current) => ({
        ...current,
        phase: 'downloaded',
        progressPercent: 100,
        message: null
      }))
    } catch (e) {
      setUpdateState((current) => ({
        ...current,
        phase: 'error',
        message: formatUpdateError(e)
      }))
    } finally {
      setDownloading(false)
    }
  }, [downloading])

  const handleInstall = useCallback(async () => {
    const update = updateRef.current
    if (!update) return
    try {
      await update.install()
    } catch (e) {
      setUpdateState((current) => ({
        ...current,
        phase: 'error',
        message: e instanceof Error ? e.message : String(e)
      }))
    }
  }, [])

  const updateBusy = checking || downloading
  const updateStatus =
    (
      {
        idle: '尚未检查',
        checking: '正在检查更新',
        available: `发现新版本 v${updateState.availableVersion ?? ''}`,
        downloading: '正在下载',
        downloaded: `${updateState.availableVersion ?? '新版本'} 已就绪`,
        'not-available': '当前已是最新版本',
        error: '更新检查失败'
      } satisfies Record<UpdatePhase, string>
    )[updateState.phase] ?? '读取中'

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
              disabled={updateBusy}
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
            <Tag color={updateState.phase === 'error' ? 'error' : 'blue'}>{updateStatus}</Tag>
          </Space>

          {updateState.phase === 'available' && (
            <Alert
              type="success"
              showIcon
              icon={<CheckCircleOutlined />}
              message={`发现新版本 v${updateState.availableVersion ?? ''}`}
              description={updateState.message ?? '可下载并安装到最新版本。'}
            />
          )}
          {updateState.message && updateState.phase === 'error' && (
            <Alert type="error" message={updateState.message} />
          )}
          {updateState.phase === 'downloading' && (
            <Progress percent={Math.round(updateState.progressPercent ?? 0)} />
          )}

          <Space wrap>
            <Button loading={checking} disabled={updateBusy} onClick={() => void handleCheck()}>
              检查更新
            </Button>
            {updateState.phase === 'available' && (
              <Button type="primary" loading={downloading} onClick={() => void handleDownload()}>
                下载更新
              </Button>
            )}
            {updateState.phase === 'downloaded' && (
              <Button type="primary" danger onClick={() => void handleInstall()}>
                重启并安装
              </Button>
            )}
          </Space>
        </Space>
      </Card>
    </div>
  )
}

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
import { Update, type DownloadEvent } from '@tauri-apps/plugin-updater'
import { tauriApi } from '../api/tauriApi'
import { formatUpdateError, retryUpdateCheck, retryUpdateDownload } from '../utils/update-check'

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
const UPDATE_CHECK_TIMEOUT_MS = 30_000
const UPDATE_CHECK_ATTEMPTS = 3
const UPDATE_CHECK_RETRY_DELAY_MS = 1_000
const UPDATE_DOWNLOAD_TIMEOUT_MS = 5 * 60_000

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
  const downloadProgressRef = useRef(0)

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
    tauriApi.settings
      .get('updateChannel')
      .then((saved) => {
        if (saved === 'beta' || saved === 'stable') setChannel(saved)
      })
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
        async () => {
          const metadata = await tauriApi.app.checkUpdate(channel, UPDATE_CHECK_TIMEOUT_MS)
          return metadata ? new Update(metadata) : null
        },
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
  }, [channel, downloading])

  const handleDownload = useCallback(async () => {
    const update = updateRef.current
    if (!update || downloading) return
    setDownloading(true)
    downloadProgressRef.current = 0
    let activeUpdate = update
    setUpdateState((current) => ({
      ...current,
      phase: 'downloading',
      progressPercent: 0,
      message: null
    }))
    try {
      await retryUpdateDownload(
        async (attempt) => {
          // A failed native stream cannot always be reused (notably after
          // `error decoding response body`).  Re-fetch the signed manifest
          // before retrying so each attempt owns a fresh response stream.
          if (attempt > 1) {
            await activeUpdate.close().catch(() => {})
            const metadata = await tauriApi.app.checkUpdate(channel, UPDATE_CHECK_TIMEOUT_MS)
            const refreshed = metadata ? new Update(metadata) : null
            if (!refreshed) throw new Error('更新在重试期间已不可用')
            activeUpdate = refreshed
            updateRef.current = refreshed
          }
          let contentLength = 0
          let downloaded = 0
          setUpdateState((current) => ({
            ...current,
            // Keep the last visible byte percentage while retrying.  The
            // native updater restarts a failed stream, so resetting to zero
            // made a healthy retry look like a download regression.
            progressPercent: attempt === 1 ? 0 : current.progressPercent,
            message: attempt === 1 ? null : `网络抖动，正在重试下载（第 ${attempt} 次）…`
          }))
          await activeUpdate.download(
            (event: DownloadEvent) => {
              if (event.event === 'Started') {
                contentLength = event.data.contentLength ?? 0
              } else if (event.event === 'Progress') {
                downloaded += event.data.chunkLength
                const percent =
                  contentLength > 0
                    ? Math.min(100, Math.round((downloaded / contentLength) * 100))
                    : null
                if (percent !== null) {
                  downloadProgressRef.current = Math.max(downloadProgressRef.current, percent)
                }
                setUpdateState((current) => ({
                  ...current,
                  progressPercent:
                    percent === null
                      ? current.progressPercent
                      : Math.max(current.progressPercent ?? 0, downloadProgressRef.current)
                }))
              }
            },
            {
              timeout: UPDATE_DOWNLOAD_TIMEOUT_MS,
              headers: {
                Accept: 'application/octet-stream',
                // Avoid content-encoding transformations that have caused
                // response stream decoding failures on some domestic proxies.
                'Accept-Encoding': 'identity'
              }
            }
          )
        },
        { timeoutMs: UPDATE_DOWNLOAD_TIMEOUT_MS, maxAttempts: 3, retryDelayMs: 1_200 }
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
  }, [channel, downloading])

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
        className="ob-surface-card ob-settings-card"
        style={{
          border: `1px solid ${token.colorBorder}`,
          borderRadius: token.borderRadius,
          boxShadow: 'none'
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
        className="ob-surface-card ob-settings-card"
        title="应用更新"
        style={{
          marginTop: 16,
          border: `1px solid ${token.colorBorder}`,
          borderRadius: token.borderRadius,
          boxShadow: 'none'
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
                void tauriApi.settings.set('updateChannel', next).catch(() => {})
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

import { useEffect, useRef, useState } from 'react'
import { Alert, Button, Modal, Progress, Space, Tag, Typography, theme } from 'antd'
import { DownloadOutlined, RocketOutlined } from '@ant-design/icons'
import { Update, type DownloadEvent } from '@tauri-apps/plugin-updater'
import { tauriApi } from '../api/tauriApi'
import { formatUpdateError, retryUpdateCheck, retryUpdateDownload } from '../utils/update-check'
import { useTaskStore } from '../store/task.store'

const STARTUP_CHECK_KEY = 'ob-startup-stable-update-check-v1'
const UPDATE_CHECK_TIMEOUT_MS = 30_000
const UPDATE_DOWNLOAD_TIMEOUT_MS = 5 * 60_000
const UPDATE_TASK_ID = 'app-update-download'

type PromptPhase = 'available' | 'downloading' | 'downloaded' | 'error'

function canCheckThisSession(): boolean {
  try {
    if (sessionStorage.getItem(STARTUP_CHECK_KEY) === '1') return false
    sessionStorage.setItem(STARTUP_CHECK_KEY, '1')
    return true
  } catch {
    return true
  }
}

/**
 * Check the stable channel once after the shell is ready. The update prompt
 * is intentionally rendered inside ThemeProvider so every theme controls its
 * modal surface, buttons, borders and typography through the same tokens.
 */
export default function StartupUpdatePrompt() {
  const { token } = theme.useToken()
  const [update, setUpdate] = useState<Update | null>(null)
  const [phase, setPhase] = useState<PromptPhase | null>(null)
  const [progress, setProgress] = useState(0)
  const [message, setMessage] = useState<string | null>(null)
  const progressRef = useRef(0)
  const activeUpdateRef = useRef<Update | null>(null)
  const upsertTask = useTaskStore((state) => state.upsertTask)
  const patchTask = useTaskStore((state) => state.patchTask)

  useEffect(() => {
    if (!canCheckThisSession()) return
    let cancelled = false

    void (async () => {
      try {
        const metadata = await retryUpdateCheck(
          () => tauriApi.app.checkUpdate('stable', UPDATE_CHECK_TIMEOUT_MS),
          { timeoutMs: UPDATE_CHECK_TIMEOUT_MS, maxAttempts: 2, retryDelayMs: 1_000 }
        )
        if (!metadata || cancelled) return
        const stableUpdate = new Update(metadata)
        activeUpdateRef.current = stableUpdate
        setUpdate(stableUpdate)
        setPhase('available')
      } catch {
        // Startup update checks are best-effort. Settings remains the explicit
        // recovery path when the network or the stable endpoint is unavailable.
      }
    })()

    return () => {
      cancelled = true
      const active = activeUpdateRef.current
      activeUpdateRef.current = null
      void active?.close().catch(() => {})
    }
  }, [])

  const closePrompt = () => {
    const active = activeUpdateRef.current
    activeUpdateRef.current = null
    setUpdate(null)
    setPhase(null)
    void active?.close().catch(() => {})
  }

  const handleDownload = async () => {
    const initialUpdate = activeUpdateRef.current
    if (!initialUpdate || phase === 'downloading') return
    setPhase('downloading')
    progressRef.current = 0
    setProgress(0)
    setMessage(null)
    upsertTask({
      id: UPDATE_TASK_ID,
      title: '下载 CrucibleBox 更新',
      detail: '更新通道：稳定版',
      source: 'update',
      status: 'running',
      progress: 0
    })

    let activeUpdate = initialUpdate
    try {
      await retryUpdateDownload(
        async (attempt) => {
          if (attempt > 1) {
            await activeUpdate.close().catch(() => {})
            const metadata = await tauriApi.app.checkUpdate('stable', UPDATE_CHECK_TIMEOUT_MS)
            if (!metadata) throw new Error('更新在重试期间已不可用')
            activeUpdate = new Update(metadata)
            activeUpdateRef.current = activeUpdate
          }
          let contentLength = 0
          let downloaded = 0
          await activeUpdate.download(
            (event: DownloadEvent) => {
              if (event.event === 'Started') {
                contentLength = event.data.contentLength ?? 0
              } else if (event.event === 'Progress') {
                downloaded += event.data.chunkLength
                if (contentLength > 0) {
                  const next = Math.min(100, Math.round((downloaded / contentLength) * 100))
                  const value = Math.max(progressRef.current, next)
                  progressRef.current = value
                  setProgress(value)
                  patchTask(UPDATE_TASK_ID, { progress: value, detail: `已下载 ${value}%` })
                }
              }
            },
            {
              timeout: UPDATE_DOWNLOAD_TIMEOUT_MS,
              headers: { Accept: 'application/octet-stream', 'Accept-Encoding': 'identity' }
            }
          )
        },
        { timeoutMs: UPDATE_DOWNLOAD_TIMEOUT_MS, maxAttempts: 3, retryDelayMs: 1_200 }
      )
      progressRef.current = 100
      setProgress(100)
      setPhase('downloaded')
      patchTask(UPDATE_TASK_ID, { status: 'completed', progress: 100, detail: '下载完成，等待安装' })
    } catch (error) {
      const detail = formatUpdateError(error)
      setMessage(detail)
      setPhase('error')
      patchTask(UPDATE_TASK_ID, { status: 'failed', error: detail, detail: '下载失败' })
    }
  }

  const handleInstall = async () => {
    const active = activeUpdateRef.current
    if (!active) return
    try {
      await active.install()
    } catch (error) {
      setMessage(formatUpdateError(error))
      setPhase('error')
    }
  }

  return (
    <Modal
      open={phase !== null && update !== null}
      title={
        <Space size={8}>
          <RocketOutlined style={{ color: token.colorPrimary }} />
          <span>发现稳定版更新</span>
        </Space>
      }
      centered
      width={460}
      maskClosable={false}
      closable={phase !== 'downloading'}
      onCancel={closePrompt}
      footer={
        phase === 'available' ? (
          <Space>
            <Button onClick={closePrompt}>稍后提醒</Button>
            <Button type="primary" icon={<DownloadOutlined />} onClick={() => void handleDownload()}>
              立即更新
            </Button>
          </Space>
        ) : phase === 'downloaded' ? (
          <Button type="primary" onClick={() => void handleInstall()}>
            重启并安装
          </Button>
        ) : phase === 'error' ? (
          <Space>
            <Button onClick={closePrompt}>关闭</Button>
            <Button type="primary" onClick={() => void handleDownload()}>
              重试下载
            </Button>
          </Space>
        ) : null
      }
    >
      {update && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <div>
            <Tag color="gold">稳定版</Tag>
            <Typography.Text strong style={{ marginLeft: 8 }}>
              v{update.version}
            </Typography.Text>
          </div>
          {phase === 'available' && (
            <Typography.Text type="secondary">
              CrucibleBox 已检测到新的稳定版本。现在更新会保留你的插件、设置和任务状态。
            </Typography.Text>
          )}
          {phase === 'downloading' && (
            <div>
              <Typography.Text type="secondary">正在下载稳定版更新，请勿关闭应用。</Typography.Text>
              <Progress percent={progress} status="active" style={{ marginTop: 8 }} />
            </div>
          )}
          {phase === 'downloaded' && (
            <Alert type="success" showIcon message="更新已下载完成，重启后安装。" />
          )}
          {phase === 'error' && <Alert type="error" showIcon message={message ?? '更新下载失败'} />}
        </div>
      )}
    </Modal>
  )
}

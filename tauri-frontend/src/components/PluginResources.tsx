import { useEffect, useRef, useState } from 'react'
import { Alert, Button, Checkbox, Modal, Popconfirm, Space, Spin, Typography } from 'antd'
import {
  tauriApi,
  type PluginResourceGroup,
  type ResourcePreview,
  type ResourceReleasePreview
} from '../api/tauriApi'

type Review =
  | (ResourcePreview & { kind: 'import' })
  | (Omit<ResourcePreview, 'token'> & {
      kind: 'download'
      releaseId: string
      title: string
      sources: string[]
    })
const size = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MiB`
const bytes = (manifest: ResourcePreview['manifest']) =>
  manifest.files.reduce((sum, file) => sum + file.bytes, 0)

export default function PluginResources({ pluginId }: { pluginId: string }) {
  const [groups, setGroups] = useState<PluginResourceGroup[]>([])
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [review, setReview] = useState<Review | null>(null)
  const [consent, setConsent] = useState({ local: false, native: false })
  const pending = useRef<string | null>(null)
  const active = useRef(true)
  useEffect(() => {
    active.current = true
    void tauriApi.resources
      .list(pluginId)
      .then((items) => {
        if (active.current) setGroups(items)
      })
      .catch((reason) => {
        if (active.current) setError(String(reason))
      })
      .finally(() => {
        if (active.current) setLoading(false)
      })
    return () => {
      active.current = false
      if (pending.current) void tauriApi.resources.discard(pending.current).catch(console.error)
    }
  }, [pluginId])
  const refresh = async () => {
    const items = await tauriApi.resources.list(pluginId)
    if (active.current) setGroups(items)
  }
  const run = async (action: () => Promise<void>) => {
    setError(null)
    setBusy(true)
    try {
      await action()
    } catch (reason) {
      if (active.current) setError(String(reason))
    } finally {
      if (active.current) setBusy(false)
    }
  }
  const importResource = (type: 'zip' | 'directory') =>
    run(async () => {
      const path =
        type === 'zip' ? await tauriApi.dialog.openFile() : await tauriApi.dialog.openDirectory()
      if (!path) return
      await tauriApi.plugin.registerImportPath(path)
      const next = await tauriApi.resources.preview(pluginId, { type, path })
      if (!active.current) {
        await tauriApi.resources.discard(next.token)
        return
      }
      pending.current = next.token
      setConsent({ local: false, native: false })
      setReview({ ...next, kind: 'import' })
    })
  const selectDownload = (candidate: ResourceReleasePreview) => {
    setConsent({ local: false, native: false })
    setReview({
      kind: 'download',
      manifest: candidate.release.manifest,
      manifestSha256: candidate.manifestSha256,
      unsignedLocal: candidate.unsignedLocal,
      releaseId: candidate.release.id,
      title: candidate.release.title,
      sources: [...new Set(candidate.release.downloads.map((file) => new URL(file.url).host))]
    })
  }
  const closeReview = async () => {
    if (pending.current) {
      await tauriApi.resources.discard(pending.current)
      pending.current = null
    }
    setReview(null)
  }
  const confirm = () =>
    run(async () => {
      if (!review) return
      const approval = {
        manifestSha256: review.manifestSha256,
        acceptPermissions: true,
        acceptUnsignedLocal: consent.local,
        acceptNativeCode: consent.native
      }
      if (review.kind === 'download') {
        await tauriApi.resources.download(pluginId, review.manifest.id, review.releaseId, approval)
        setNotice('下载任务已创建，可在任务中心查看进度或取消。完成后刷新资源列表。')
      } else {
        await tauriApi.resources.commit(review.token, approval)
        pending.current = null
        setNotice('资源已安装，插件将在下次启动后端时加载兼容版本。')
      }
      setReview(null)
      await refresh()
    })
  if (loading) return <Spin size="small" />
  if (!groups.length && !error) return null
  const ready =
    !!review &&
    (!review.unsignedLocal || consent.local) &&
    (!review.manifest.containsNativeCode || consent.native)
  return (
    <section aria-label="插件资源" style={{ marginTop: 20 }}>
      <Typography.Title level={5}>模型与运行资源</Typography.Title>
      <Typography.Paragraph type="secondary">
        按需下载或导入，安装后可离线使用。运行时加载最高兼容版本；卸载资源会先停止此插件的任务。
      </Typography.Paragraph>
      {error && (
        <Alert role="alert" type="error" showIcon message={error} style={{ marginBottom: 12 }} />
      )}
      {notice && (
        <Alert role="status" type="info" showIcon message={notice} style={{ marginBottom: 12 }} />
      )}
      <Space wrap style={{ marginBottom: 12 }}>
        <Button disabled={busy} onClick={() => void importResource('zip')}>
          导入资源 ZIP
        </Button>
        <Button disabled={busy} onClick={() => void importResource('directory')}>
          导入资源目录
        </Button>
        <Button disabled={busy} onClick={() => void run(refresh)}>
          刷新
        </Button>
      </Space>
      {groups.map((group) => (
        <div key={group.id} style={{ marginBottom: 16 }}>
          <Typography.Text strong>{group.id}</Typography.Text>
          <Typography.Paragraph type="secondary">
            {group.optional ? '可选资源' : '必需资源'} · ≥ {group.version.min}，&lt;{' '}
            {group.version.maxExclusive}
          </Typography.Paragraph>
          {group.installed.length === 0 && <Typography.Paragraph>尚未安装</Typography.Paragraph>}
          {group.installed.map((manifest) => (
            <div
              key={manifest.version}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                gap: 8,
                marginBottom: 8
              }}
            >
              <span>
                已安装 {manifest.version} · {size(bytes(manifest))}
              </span>
              <Popconfirm
                title={`卸载 ${group.id} ${manifest.version}？`}
                description="插件数据会保留；再次使用此资源需要重新下载或导入。"
                onConfirm={() =>
                  run(async () => {
                    await tauriApi.resources.remove(pluginId, group.id, manifest.version)
                    await refresh()
                  })
                }
              >
                <Button danger disabled={busy}>
                  卸载资源
                </Button>
              </Popconfirm>
            </div>
          ))}
          {group.available.map((candidate) => (
            <div key={candidate.release.id} style={{ marginTop: 12 }}>
              <Typography.Text strong>{candidate.release.title}</Typography.Text>
              <Typography.Paragraph>{candidate.release.description}</Typography.Paragraph>
              <Typography.Paragraph type="secondary">
                {candidate.release.manifest.version} · {size(bytes(candidate.release.manifest))} ·
                许可：{candidate.release.manifest.license}
              </Typography.Paragraph>
              <Button
                disabled={
                  busy ||
                  group.installed.some(
                    (manifest) => manifest.version === candidate.release.manifest.version
                  )
                }
                onClick={() => selectDownload(candidate)}
              >
                下载此资源
              </Button>
            </div>
          ))}
        </div>
      ))}
      <Modal
        title={review?.kind === 'download' ? `下载 ${review.title}` : '确认导入资源'}
        open={review !== null}
        onCancel={() => {
          if (!busy) void run(closeReview)
        }}
        onOk={() => void confirm()}
        okText={review?.kind === 'download' ? '下载并安装' : '安装资源'}
        okButtonProps={{ disabled: !ready }}
        confirmLoading={busy}
        maskClosable={false}
      >
        {review && (
          <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <Typography.Text>
              {review.manifest.id} · {review.manifest.version} · {size(bytes(review.manifest))}
            </Typography.Text>
            <Typography.Text>
              发布者：{review.manifest.publisher.displayName}（{review.manifest.publisher.id}）
            </Typography.Text>
            <Typography.Text>
              来源：
              {review.kind === 'download' ? review.sources.join('、') : review.manifest.source}
            </Typography.Text>
            <Typography.Text>许可：{review.manifest.license}</Typography.Text>
            <Typography.Paragraph type="secondary">
              下载和安装需要暂存空间。文件大小与摘要全部校验通过后才启用；失败保留已有版本。
            </Typography.Paragraph>
            {review.unsignedLocal && (
              <Checkbox
                checked={consent.local}
                onChange={(event) => setConsent({ ...consent, local: event.target.checked })}
              >
                我信任此资源清单和来源（必选；无独立签名）
              </Checkbox>
            )}
            {review.manifest.containsNativeCode && (
              <>
                <Alert
                  type="warning"
                  showIcon
                  message="此资源包含可信本地原生代码，以当前用户权限运行，不具备 OS 安全沙箱。"
                />
                <Checkbox
                  checked={consent.native}
                  onChange={(event) => setConsent({ ...consent, native: event.target.checked })}
                >
                  我允许运行其中的原生代码（必选）
                </Checkbox>
              </>
            )}
          </Space>
        )}
      </Modal>
    </section>
  )
}

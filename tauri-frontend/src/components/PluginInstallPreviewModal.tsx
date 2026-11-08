import { useState } from 'react'
import { Alert, Button, Modal, Tag, Typography, theme } from 'antd'
import { CheckCircleOutlined, WarningOutlined } from '@ant-design/icons'
import type { PluginInstallPreview } from '../api/tauriApi'
import { usePluginStore } from '../store/plugin.store'

const { Text } = Typography

/**
 * 安装确认弹窗（1.9.12 提升为全局组件）：
 * - 单个导入：与原 PluginImport 内嵌预览一致（确认安装 / 取消）
 * - 批量导入（全局拖拽多文件 / 弹窗批量选择）：显示进度「第 k/n 个」，
 *   确认后自动驱动队列下一项；支持跳过单个、取消全部；结束时展示汇总
 */
export default function PluginInstallPreviewModal() {
  const installPreview = usePluginStore((s) => s.installPreview)
  const activeInstallPath = usePluginStore((s) => s.activeInstallPath)
  const queueRemaining = usePluginStore((s) => s.installQueue.length)
  const batchTotal = usePluginStore((s) => s.batchTotal)
  const succeeded = usePluginStore((s) => s.batchSucceeded)
  const skipped = usePluginStore((s) => s.batchSkipped)
  const failures = usePluginStore((s) => s.batchFailures)
  const processing = usePluginStore((s) => s.queueProcessing)
  const commitInstall = usePluginStore((s) => s.commitInstall)
  const discardInstall = usePluginStore((s) => s.discardInstall)
  const clearInstallQueue = usePluginStore((s) => s.clearInstallQueue)
  const loading = usePluginStore((s) => s.loading)
  const { token } = theme.useToken()

  const [importing, setImporting] = useState(false)
  const failedInstalls = usePluginStore((s) => s.failedInstalls)
  const enqueueInstalls = usePluginStore((s) => s.enqueueInstalls)
  const showSummary =
    batchTotal > 0 &&
    !installPreview &&
    queueRemaining === 0 &&
    !processing &&
    (batchTotal > 1 || failures.length > 0)
  const retryFailedInstalls = () => {
    const requests = failedInstalls.map(({ source, path, title, marketplaceSourceId }) => ({
      source,
      path,
      title,
      marketplaceSourceId
    }))
    clearInstallQueue()
    enqueueInstalls(requests)
  }

  const handleCommit = async () => {
    if (importing || loading) return
    setImporting(true)
    await commitInstall(
      preview?.next2
        ? {
            manifestSha256: preview.next2.manifestSha256,
            acceptNativeCode: preview.next2.nativeCode,
            acceptUnsignedLocal: preview.next2.unsignedLocal,
            acceptPermissions: true
          }
        : undefined
    )
    setImporting(false)
  }

  const handleSkip = async () => {
    setImporting(true)
    await discardInstall()
    setImporting(false)
  }

  const handleCancelAll = async () => {
    clearInstallQueue()
    if (installPreview) await discardInstall()
  }

  const preview = installPreview?.data
  const fileName = activeInstallPath?.split(/[\\/]/).pop() ?? ''
  const batchActive =
    batchTotal > 1 && (queueRemaining > 0 || installPreview !== null || processing)
  const processed = succeeded + skipped + failures.length
  const currentIndex = Math.min(processed + 1, batchTotal)

  return (
    <>
      <Modal
        title={batchActive ? `确认安装插件（第 ${currentIndex}/${batchTotal} 个）` : '确认安装插件'}
        open={installPreview !== null}
        onCancel={() => {
          if (!importing && !loading) void (batchActive ? handleCancelAll() : discardInstall())
        }}
        closable={!importing && !loading}
        cancelButtonProps={{ disabled: importing || loading }}
        onOk={() => void handleCommit()}
        okText={batchActive && queueRemaining > 0 ? '确认并继续' : '确认安装'}
        cancelText={batchActive ? '取消全部' : '取消'}
        confirmLoading={importing || loading}
        width={520}
        centered
        maskClosable={false}
        footer={
          batchActive ? (
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <Button
                type="text"
                style={{ color: token.colorTextSecondary, paddingLeft: 0 }}
                disabled={importing || loading}
                onClick={() => void handleSkip()}
              >
                跳过此个
              </Button>
              <div style={{ display: 'flex', gap: 8 }}>
                <Button disabled={importing || loading} onClick={() => void handleCancelAll()}>
                  取消全部
                </Button>
                <Button
                  data-ob-kind="primary"
                  type="primary"
                  loading={importing || loading}
                  disabled={importing || loading}
                  onClick={() => void handleCommit()}
                >
                  {queueRemaining > 0 ? '确认并继续' : '确认安装'}
                </Button>
              </div>
            </div>
          ) : undefined
        }
      >
        {batchActive && (
          <Alert
            type="info"
            showIcon
            message={`批量导入：已处理 ${processed}/${batchTotal}${fileName ? ` · 当前：${fileName}` : ''}`}
            style={{ marginBottom: 12 }}
          />
        )}
        <Text strong>{preview?.displayName || fileName}</Text>
        {preview && <InstallPreviewDetail preview={preview} />}
        {preview?.next2 && (
          <div style={{ display: 'grid', gap: 12, marginTop: 16 }}>
            <Text>
              发布者：{preview.next2.publisher.displayName}（{preview.next2.publisher.id}）
            </Text>
            <Text type="secondary">
              {preview.next2.unsignedLocal
                ? '本地未签名包，发布者身份尚未验证。'
                : '签名已由已信任发布者密钥验证。'}
            </Text>
            <Text>
              点击“确认安装”或“确认并继续”，即授权：{permissionSummary(preview.permissions)}
              {preview.next2.nativeCode ? '，并以你的 Windows 用户权限运行本地代码' : ''}。
            </Text>
            <details>
              <summary>查看权限与来源详情</summary>
              {preview.next2.nativeCode && (
                <p>本地代码仅适用于可信来源。进程隔离与生命周期管理不构成操作系统安全沙箱。</p>
              )}
              <InstallPermissionDetail preview={preview} />
            </details>
          </div>
        )}
      </Modal>

      <Modal
        title={failures.length > 0 ? '插件安装结果' : '批量导入完成'}
        open={showSummary}
        onCancel={clearInstallQueue}
        footer={
          <div style={{ display: 'flex', justifyContent: 'end', gap: 8 }}>
            {failedInstalls.length > 0 && <Button onClick={retryFailedInstalls}>重试失败项</Button>}
            <Button type="primary" onClick={clearInstallQueue}>
              完成
            </Button>
          </div>
        }
        width={420}
        centered
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8, fontSize: 13 }}>
          <span>✅ 成功安装：{succeeded} 个</span>
          {skipped > 0 && <span>⏭️ 已跳过：{skipped} 个</span>}
          {failures.length > 0 && (
            <>
              <span style={{ color: 'var(--ob-color-error, #ff4d4f)' }}>
                ❌ 失败：{failures.length} 个
              </span>
              <ul style={{ margin: '4px 0 0', paddingLeft: 18 }}>
                {failures.map((name, index) => (
                  <li key={`${index}:${name}`}>
                    <Text type="secondary" style={{ fontSize: 12 }}>
                      {name}
                    </Text>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      </Modal>
    </>
  )
}

export function InstallPreviewDetail({ preview }: { preview: PluginInstallPreview }) {
  return (
    <div style={{ padding: '8px 0' }}>
      {preview.isUpgrade ? (
        <Alert
          type="warning"
          showIcon
          icon={<WarningOutlined />}
          message={`检测到已安装版本，将升级到 v${preview.version}`}
          description={
            preview.previousVersion
              ? `当前版本 v${preview.previousVersion} → 新版本 v${preview.version}`
              : `新版本 v${preview.version}`
          }
          style={{ marginBottom: 16 }}
        />
      ) : (
        <Alert
          type="success"
          showIcon
          icon={<CheckCircleOutlined />}
          message={`将安装 v${preview.version}`}
          style={{ marginBottom: 16 }}
        />
      )}

      {preview.isUpgrade && preview.next2?.dataMigration && (
        <Alert
          type="info"
          showIcon
          message="插件数据迁移说明"
          description={
            <>
              <div>{preview.next2.dataMigration.summary}</div>
              <div>
                适用旧版本 {preview.next2.dataMigration.fromPluginVersion.min} 至低于{' '}
                {preview.next2.dataMigration.fromPluginVersion.maxExclusive}
              </div>
            </>
          }
          style={{ marginBottom: 16 }}
        />
      )}
      {preview.legacyFullTrust && (
        <Alert
          type="warning"
          showIcon
          message="该插件声明了完整信任（legacyFullTrust）"
          description="插件将获得宿主全部能力，请确认来源可信。"
          style={{ marginBottom: 16 }}
        />
      )}
      {preview.fullTrust && (
        <Alert
          type="error"
          showIcon
          message="该插件申请完整宿主权限"
          description="完整信任插件可以调用全部宿主能力。请确认来源和用途后再继续安装。"
        />
      )}
    </div>
  )
}

function InstallPermissionDetail({ preview }: { preview: PluginInstallPreview }) {
  const { token } = theme.useToken()
  return (
    <div style={{ marginTop: 12 }}>
      <div style={{ fontSize: 13, color: token.colorTextSecondary, marginBottom: 8 }}>权限清单</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
        {preview.permissions?.length ? (
          preview.permissions.map((permission) => (
            <Tag key={permission} style={{ fontSize: 11 }}>
              {permission}
            </Tag>
          ))
        ) : (
          <Text type="secondary" style={{ fontSize: 12 }}>
            无
          </Text>
        )}
      </div>

      {(preview.addedPermissions?.length > 0 || preview.removedPermissions?.length > 0) && (
        <div style={{ marginTop: 16 }}>
          {preview.addedPermissions?.length > 0 && (
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 12, color: token.colorSuccess, marginBottom: 4 }}>
                新增权限
              </div>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                {preview.addedPermissions.map((permission) => (
                  <Tag key={permission} color="success" style={{ fontSize: 11 }}>
                    {permission}
                  </Tag>
                ))}
              </div>
            </div>
          )}
          {preview.removedPermissions?.length > 0 && (
            <div>
              <div style={{ fontSize: 12, color: token.colorError, marginBottom: 4 }}>移除权限</div>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                {preview.removedPermissions.map((permission) => (
                  <Tag key={permission} color="error" style={{ fontSize: 11 }}>
                    {permission}
                  </Tag>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

const PERMISSION_SUMMARIES: Record<string, string> = {
  files: '读取选定文件或保存输出',
  storage: '保存插件数据',
  logging: '写入插件日志',
  events: '发布或订阅插件事件',
  tasks: '执行和管理任务',
  network: '访问网络',
  process: '运行已批准资源包内的本地工具',
  runtime: '执行本地运行环境任务，可访问当前用户文件和网络',
  native: '由可信本地插件提交文件或目录输出',
  browser: '下载文件',
  theme: '调整界面主题',
  dialog: '显示选择或确认窗口',
  notification: '发送通知'
}

function permissionSummary(permissions: string[]): string {
  const labels = permissions.map((permission) =>
    permission === 'theme:assets'
      ? '读取和导入主题素材'
      : (PERMISSION_SUMMARIES[permission.split(':')[0]] ?? '其他扩展权限')
  )
  return [...new Set(labels)].join('、') || '仅使用基础插件能力'
}

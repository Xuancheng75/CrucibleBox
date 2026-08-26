import { useEffect, useRef, useState } from 'react'
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
  // 批量会话结束后的汇总可见性：total>1 且队列耗尽且无待确认项时出现一次
  const [showSummary, setShowSummary] = useState(false)
  const seenTotalRef = useRef(0)

  useEffect(() => {
    if (batchTotal > seenTotalRef.current) {
      seenTotalRef.current = batchTotal
      setShowSummary(false)
    }
    if (
      batchTotal > 1 &&
      batchTotal === seenTotalRef.current &&
      !installPreview &&
      queueRemaining === 0 &&
      !processing
    ) {
      setShowSummary(true)
    }
  }, [batchTotal, installPreview, queueRemaining, processing])

  const handleCommit = async () => {
    setImporting(true)
    await commitInstall()
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
  const batchActive = batchTotal > 1 && (queueRemaining > 0 || installPreview !== null || processing)
  const processed = succeeded + skipped + failures.length
  const currentIndex = Math.min(processed + 1, batchTotal)

  return (
    <>
      <Modal
        title={batchActive ? `确认安装插件（第 ${currentIndex}/${batchTotal} 个）` : '确认安装插件'}
        open={installPreview !== null}
        onCancel={() => void (batchActive ? handleCancelAll() : discardInstall())}
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
        {preview && <InstallPreviewDetail preview={preview} />}
      </Modal>

      <Modal
        title="批量导入完成"
        open={showSummary}
        onOk={() => {
          setShowSummary(false)
          clearInstallQueue()
        }}
        cancelButtonProps={{ style: { display: 'none' } }}
        width={420}
        centered
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8, fontSize: 13 }}>
          <span>✅ 成功安装：{succeeded} 个</span>
          {skipped > 0 && <span>⏭️ 已跳过：{skipped} 个</span>}
          {failures.length > 0 && (
            <>
              <span style={{ color: 'var(--ob-color-error, #ff4d4f)' }}>❌ 失败：{failures.length} 个</span>
              <ul style={{ margin: '4px 0 0', paddingLeft: 18 }}>
                {failures.map((name) => (
                  <li key={name}>
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
  const { token } = theme.useToken()

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

      {preview.legacyFullTrust && (
        <Alert
          type="warning"
          showIcon
          message="该插件声明了完整信任（legacyFullTrust）"
          description="插件将获得宿主全部能力，请确认来源可信。"
          style={{ marginBottom: 16 }}
        />
      )}

      <div style={{ fontSize: 13, color: token.colorTextSecondary, marginBottom: 8 }}>
        权限清单
      </div>
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
              <div style={{ fontSize: 12, color: token.colorError, marginBottom: 4 }}>
                移除权限
              </div>
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

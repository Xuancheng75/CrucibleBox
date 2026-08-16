import React, { useEffect, useState } from 'react'
import { App, Modal, Button, Upload, Space, Typography, Divider, theme, Tag, Alert } from 'antd'
import {
  UploadOutlined,
  FolderOpenOutlined,
  InboxOutlined,
  CheckCircleOutlined,
  WarningOutlined
} from '@ant-design/icons'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { usePluginStore } from '../store/plugin.store'
import { tauriApi, type PluginInstallPreview } from '../api/tauriApi'

const { Dragger } = Upload
const { Text } = Typography

interface PluginImportProps {
  open: boolean
  onClose: () => void
}

export default function PluginImport({ open, onClose }: PluginImportProps) {
  const { token } = theme.useToken()
  const { message } = App.useApp()
  const installPlugin = usePluginStore((s) => s.installPlugin)
  const commitInstall = usePluginStore((s) => s.commitInstall)
  const discardInstall = usePluginStore((s) => s.discardInstall)
  const installError = usePluginStore((s) => s.error)
  const installPreview = usePluginStore((s) => s.installPreview)
  const [importing, setImporting] = useState(false)

  // 拖拽 .zip 插件包（Tauri onDragDropEvent 拿 paths，替代 Electron webUtils.getPathForFile）
  useEffect(() => {
    if (!open) return
    let unlisten: (() => void) | undefined
    getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        if (event.payload.type === 'drop') {
          const path = event.payload.paths[0]
          if (path) void installZipPath(path)
        }
      })
      .then((fn) => {
        unlisten = fn
      })
    return () => unlisten?.()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const showInstallError = () => {
    const detail = installError?.trim()
    if (!detail) {
      message.error('插件安装失败')
      return
    }
    const missingDistEntry =
      /manifest\.entry:.*(?:does not exist|is not a regular file): dist\//.test(detail)
    message.error(
      missingDistEntry ? (
        <div>
          <div>{detail}</div>
          <div style={{ marginTop: 8 }}>
            提示：所选目录缺少 dist/ 构建产物（例如 dist/main.js）。请先在该插件工程内执行{' '}
            <Text code>npm run build</Text> 构建后再导入。
          </div>
        </div>
      ) : (
        detail
      )
    )
  }

  const installZipPath = async (path: string) => {
    if (!path.toLowerCase().endsWith('.zip')) {
      message.warning('请选择 .zip 插件包')
      return
    }

    try {
      setImporting(true)
      const success = await installPlugin('zip', path)
      if (success) {
        // 预览弹窗由下方 installPreview Modal 呈现
      } else {
        showInstallError()
      }
    } catch (err) {
      message.error(`导入失败: ${(err as Error).message}`)
    } finally {
      setImporting(false)
    }
  }

  const handleSelectZip = async () => {
    try {
      const path = await tauriApi.dialog.openFile()
      if (path) {
        await installZipPath(path)
      }
    } catch (err) {
      message.error(`导入失败: ${(err as Error).message}`)
      setImporting(false)
    }
  }

  const handleSelectDirectory = async () => {
    try {
      const path = await tauriApi.dialog.openDirectory()
      if (path) {
        setImporting(true)
        const success = await installPlugin('directory', path)
        if (success) {
          // 预览弹窗由下方 installPreview Modal 呈现
        } else {
          showInstallError()
        }
        setImporting(false)
      }
    } catch (err) {
      message.error(`导入失败: ${(err as Error).message}`)
      setImporting(false)
    }
  }

  const handleCommit = async () => {
    setImporting(true)
    const success = await commitInstall()
    setImporting(false)
    if (success) {
      message.success('插件安装成功')
      onClose()
    } else {
      showInstallError()
    }
  }

  const handleDiscard = async () => {
    await discardInstall()
    onClose()
  }

  const preview = installPreview?.data

  return (
    <>
      <Modal title="导入插件" open={open} onCancel={onClose} footer={null} width={480} centered>
        <div style={{ padding: '12px 0' }}>
          <Dragger
            disabled={importing}
            style={{
              background: token.colorBgLayout,
              border: `2px dashed ${token.colorBorder}`,
              borderRadius: token.borderRadius
            }}
            beforeUpload={() => Upload.LIST_IGNORE}
          >
            <p className="ob-upload-drag-icon">
              <InboxOutlined />
            </p>
            <Text style={{ color: token.colorTextSecondary }}>拖拽 .zip 插件包到此处</Text>
          </Dragger>

          <Divider plain>
            <Text type="secondary">或者</Text>
          </Divider>

          <Space direction="vertical" style={{ width: '100%' }} size={12}>
            <Button
              block
              size="large"
              icon={<UploadOutlined />}
              onClick={handleSelectZip}
              loading={importing}
              style={{ height: 48, borderRadius: 8 }}
            >
              选择 .zip 插件包
            </Button>
            <Button
              block
              size="large"
              icon={<FolderOpenOutlined />}
              onClick={handleSelectDirectory}
              loading={importing}
              style={{ height: 48, borderRadius: 8 }}
            >
              选择插件目录
            </Button>
          </Space>
        </div>
      </Modal>

      <Modal
        title="确认安装插件"
        open={installPreview !== null}
        onCancel={handleDiscard}
        onOk={handleCommit}
        okText="确认安装"
        cancelText="取消"
        confirmLoading={importing}
        width={520}
        centered
      >
        {preview && <InstallPreviewDetail preview={preview} />}
      </Modal>
    </>
  )
}

function InstallPreviewDetail({ preview }: { preview: PluginInstallPreview }) {
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
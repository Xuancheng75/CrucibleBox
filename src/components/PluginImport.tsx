import React, { useState } from 'react'
import { App, Modal, Button, Upload, Space, Typography, Divider, theme } from 'antd'
import { UploadOutlined, FolderOpenOutlined, InboxOutlined } from '@ant-design/icons'
import { usePluginStore } from '../store/plugin.store'

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
  const installError = usePluginStore((s) => s.error)
  const [importing, setImporting] = useState(false)

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
        message.success('插件安装成功')
        onClose()
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
      const path = await window.electronAPI?.dialog.openFile()
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
      const path = await window.electronAPI?.dialog.openDirectory()
      if (path) {
        setImporting(true)
        const success = await installPlugin('directory', path)
        if (success) {
          message.success('插件安装成功')
          onClose()
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

  return (
    <Modal title="导入插件" open={open} onCancel={onClose} footer={null} width={480} centered>
      <div style={{ padding: '12px 0' }}>
        <Dragger
          disabled={importing}
          style={{
            background: token.colorBgLayout,
            border: `2px dashed ${token.colorBorder}`,
            borderRadius: token.borderRadius
          }}
          beforeUpload={async (file) => {
            const path = window.electronAPI?.file.getPath(file as unknown as File)
            if (path) {
              await window.electronAPI?.plugin.registerImportPath(path)
              await installZipPath(path)
            } else {
              message.error('无法读取拖入文件路径')
            }
            return Upload.LIST_IGNORE
          }}
        >
          <p className="ob-upload-drag-icon">
            <InboxOutlined />
          </p>
          <Text style={{ color: token.colorTextSecondary }}>拖拽 .zip 插件包到此处</Text>
        </Dragger>

        <Divider plain>
          <Text type="secondary">或者</Text>
        </Divider>

        <Space orientation="vertical" style={{ width: '100%' }} size={12}>
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
  )
}

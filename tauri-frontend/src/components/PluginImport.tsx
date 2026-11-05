import { useState } from 'react'
import { App, Modal, Button, Space, Typography } from 'antd'
import { AppstoreAddOutlined, FolderOpenOutlined, UploadOutlined } from '@ant-design/icons'
import { usePluginStore } from '../store/plugin.store'
import { tauriApi } from '../api/tauriApi'

const { Text } = Typography

interface PluginImportProps {
  open: boolean
  onClose: () => void
}

/**
 * 导入插件弹窗（1.9.12 重构）：
 * - 移除装饰性 Dragger 与弹窗内拖拽监听（全局窗口级拖拽见 useGlobalPluginDrop）
 * - 安装确认预览提升为全局组件 PluginInstallPreviewModal（App 根渲染）
 * - 新增批量导入入口（多选 .zip → 逐个预览确认）
 */
export default function PluginImport({ open, onClose }: PluginImportProps) {
  const { message } = App.useApp()
  const installPlugin = usePluginStore((s) => s.installPlugin)
  const enqueueInstalls = usePluginStore((s) => s.enqueueInstalls)
  const [importing, setImporting] = useState(false)

  const errorText = (err: unknown): string =>
    err instanceof Error ? err.message : typeof err === 'string' ? err : String(err)

  const showInstallError = () => {
    const detail = usePluginStore.getState().error
    message.error(detail ? `插件导入失败：${detail}` : '插件导入失败，请检查包内容后重试')
  }

  const handleSelect = async (source: 'zip' | 'directory') => {
    setImporting(true)
    try {
      const path =
        source === 'zip' ? await tauriApi.dialog.openFile() : await tauriApi.dialog.openDirectory()
      if (!path) return
      if (source === 'zip' && !path.toLowerCase().endsWith('.zip')) {
        message.warning('请选择 .zip 插件包')
        return
      }
      if (await installPlugin(source, path)) onClose()
      else showInstallError()
    } catch (error) {
      message.error(`导入失败：${errorText(error)}`)
    } finally {
      setImporting(false)
    }
  }

  const handleSelectBatch = async () => {
    setImporting(true)
    try {
      const paths = await tauriApi.dialog.openFiles()
      const zips = paths.filter((path) => path.toLowerCase().endsWith('.zip'))
      if (zips.length === 0) {
        if (paths.length > 0) message.warning('请选择 .zip 插件包')
        return
      }
      enqueueInstalls(zips.map((path) => ({ source: 'zip' as const, path })))
      onClose()
    } catch (error) {
      message.error(`批量导入失败：${errorText(error)}`)
    } finally {
      setImporting(false)
    }
  }

  return (
    <Modal title="导入插件" open={open} onCancel={onClose} footer={null} width={420} centered>
      <div style={{ padding: '12px 0', display: 'flex', flexDirection: 'column', gap: 12 }}>
        <Button
          block
          size="large"
          icon={<UploadOutlined />}
          loading={importing}
          style={{ height: 48, borderRadius: 8 }}
          onClick={() => void handleSelect('zip')}
        >
          选择 .zip 插件包
        </Button>
        <Button
          block
          size="large"
          icon={<FolderOpenOutlined />}
          loading={importing}
          style={{ height: 48, borderRadius: 8 }}
          onClick={() => void handleSelect('directory')}
        >
          选择插件目录
        </Button>
        <Button
          block
          size="large"
          icon={<AppstoreAddOutlined />}
          loading={importing}
          style={{ height: 48, borderRadius: 8 }}
          onClick={() => void handleSelectBatch()}
        >
          批量导入（多选 .zip）
        </Button>

        <Space direction="vertical" size={2} style={{ marginTop: 4 }}>
          <Text type="secondary" style={{ fontSize: 12 }}>
            提示：也可以把 .zip 压缩包直接拖入工具箱窗口任意位置完成导入。
          </Text>
        </Space>
      </div>
    </Modal>
  )
}

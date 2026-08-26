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
    // 错误详情由全局确认弹窗的安装结果与 message 提示呈现；此处兜底提示
    message.error('插件导入失败，请检查包内容后重试')
  }

  const installZipPath = async (path: string): Promise<boolean> => {
    if (!path.toLowerCase().endsWith('.zip')) {
      message.warning('请选择 .zip 插件包')
      return false
    }
    try {
      setImporting(true)
      return await installPlugin('zip', path)
    } catch (err) {
      message.error(`导入失败: ${errorText(err)}`)
      return false
    } finally {
      setImporting(false)
    }
  }

  const handleSelectZip = async (): Promise<boolean> => {
    try {
      const path = await tauriApi.dialog.openFile()
      if (!path) return false
      return await installZipPath(path)
    } catch (err) {
      message.error(`导入失败: ${errorText(err)}`)
      return false
    }
  }

  const handleSelectDirectory = async (): Promise<boolean> => {
    try {
      const path = await tauriApi.dialog.openDirectory()
      if (!path) return false
      try {
        setImporting(true)
        return await installPlugin('directory', path)
      } finally {
        setImporting(false)
      }
    } catch (err) {
      message.error(`导入失败: ${errorText(err)}`)
      return false
    }
  }

  /** 批量导入：多选 .zip → 入队 → 全局确认弹窗逐个预览 */
  const handleSelectBatch = async (): Promise<boolean> => {
    try {
      const paths = await tauriApi.dialog.openFiles()
      const zips = paths.filter((p) => p.toLowerCase().endsWith('.zip'))
      if (zips.length === 0) {
        if (paths.length > 0) message.warning('请选择 .zip 插件包')
        return false
      }
      enqueueInstalls(zips.map((path) => ({ source: 'zip' as const, path })))
      onClose()
      return true
    } catch (err) {
      message.error(`批量导入失败: ${errorText(err)}`)
      return false
    }
  }

  // 单个导入成功（installPreview 就绪）时关闭本弹窗，交由全局确认弹窗接管
  const runAndCloseIfReady = async (action: () => Promise<boolean>) => {
    const ok = await action()
    if (ok) onClose()
    else if (!usePluginStore.getState().installPreview) showInstallError()
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
          onClick={() => void runAndCloseIfReady(handleSelectZip)}
        >
          选择 .zip 插件包
        </Button>
        <Button
          block
          size="large"
          icon={<FolderOpenOutlined />}
          loading={importing}
          style={{ height: 48, borderRadius: 8 }}
          onClick={() => void runAndCloseIfReady(handleSelectDirectory)}
        >
          选择插件目录
        </Button>
        <Button
          block
          size="large"
          icon={<AppstoreAddOutlined />}
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

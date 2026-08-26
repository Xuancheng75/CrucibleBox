import { Typography, theme } from 'antd'
import { InboxOutlined } from '@ant-design/icons'

const { Text } = Typography

/**
 * 全窗口拖拽导入的视觉遮罩（1.9.12）。
 * 纯展示：pointer-events 关闭，OS 级 drop 不受 DOM 影响。
 */
export default function PluginDropOverlay({ active }: { active: boolean }) {
  const { token } = theme.useToken()
  if (!active) return null
  return (
    <div
      aria-hidden="true"
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 3000,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        pointerEvents: 'none',
        background: `${token.colorBgMask}`,
        backdropFilter: 'blur(2px)'
      }}
    >
      <div
        style={{
          position: 'absolute',
          inset: 16,
          border: `2px dashed ${token.colorPrimary}`,
          borderRadius: token.borderRadiusLG,
          background: token.colorPrimaryBg,
          opacity: 0.9
        }}
      />
      <div style={{ textAlign: 'center', position: 'relative' }}>
        <InboxOutlined style={{ fontSize: 48, color: token.colorPrimary }} />
        <div style={{ marginTop: 12 }}>
          <Text strong style={{ fontSize: 16, color: token.colorText }}>
            松开鼠标，导入插件包
          </Text>
        </div>
        <div style={{ marginTop: 4 }}>
          <Text type="secondary" style={{ fontSize: 12 }}>
            支持 .zip 插件包与插件目录（可多选）
          </Text>
        </div>
      </div>
    </div>
  )
}

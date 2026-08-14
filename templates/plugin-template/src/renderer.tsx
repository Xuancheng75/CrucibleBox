import type { PluginRenderProps } from 'cruciblebox-plugin-api'

export default function MyPlugin({ config, api }: PluginRenderProps) {
  return (
    <div style={{ padding: 16 }}>
      <h3 style={{ margin: '0 0 12px', color: '#333' }}>{config.displayName || '我的插件'}</h3>
      <p style={{ color: '#666', fontSize: 13, lineHeight: 1.6 }}>
        编辑 src/renderer.tsx 构建您的插件界面
      </p>
      <button
        onClick={() => api.notify('来自插件的通知！')}
        style={{
          padding: '8px 16px',
          background: '#555',
          color: '#fff',
          border: 'none',
          borderRadius: 6,
          cursor: 'pointer'
        }}
      >
        发送通知
      </button>
    </div>
  )
}

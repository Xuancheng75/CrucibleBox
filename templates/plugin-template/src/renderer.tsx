import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import { useEffect, useState } from 'react'

export default function MyPlugin({ config, api }: PluginRenderProps) {
  const [files, setFiles] = useState<string[]>([])
  useEffect(() => api.onFilesDropped?.(setFiles), [api])
  return (
    <div style={{ padding: 16 }}>
      <h3 style={{ margin: '0 0 12px', color: '#333' }}>
        {typeof config.displayName === 'string' ? config.displayName : '我的插件'}
      </h3>
      <p style={{ color: '#666', fontSize: 13, lineHeight: 1.6 }}>
        编辑 src/renderer.tsx 构建您的插件界面
      </p>
      {files.length > 0 && <p>已接收文件：{files.join('、')}</p>}
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

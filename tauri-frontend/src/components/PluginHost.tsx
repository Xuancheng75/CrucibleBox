import { invoke } from '@tauri-apps/api/core'
import { requestNextSession } from '../features/plugins/runtime/NextSessionRequest'
import { NextPluginFrame } from './NextPluginFrame'
import type { NextSession } from '../features/plugins/runtime/NextFrameBridge'
import { useEffect, useState } from 'react'
import { Alert, Spin } from 'antd'
import type { PluginConfig } from '../../../shared/types/plugin.types'
import { Permission } from '../../../shared/types/permissions'
import { tauriApi } from '../api/tauriApi'

interface PluginHostProps {
  pluginId: string
  pluginName: string
  rendererEntry: string
  config: PluginConfig
  permissions?: Permission[]
  onConfigChange: (config: PluginConfig) => void
}

export function PluginHost(props: PluginHostProps) {
  const [route, setRoute] = useState<{ pluginId: string; next: NextSession | null } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const permissionKey = [...(props.permissions ?? [])].sort().join('\n')
  useEffect(() => {
    let active = true
    const controller = new AbortController()
    let token: string | undefined
    setRoute(null)
    setError(null)
    void requestNextSession(
      () =>
        invoke<NextSession>(
          'create_next_renderer_session',
          { id: props.pluginId },
          { headers: { Origin: window.location.origin } }
        ),
      controller.signal
    )
      .then((session) => {
        token = session.token
        if (active) setRoute({ pluginId: props.pluginId, next: session })
        else void tauriApi.plugin.disposeRendererSession(session.token)
      })
      .catch((reason) => {
        if (!active) return
        setError(
          String(reason) === 'SESSION_DENIED'
            ? '插件会话被拒绝，请检查插件版本和权限。原插件包和数据已保留。'
            : `加载插件界面失败：${String(reason)}`
        )
      })
    return () => {
      active = false
      controller.abort()
      if (token) void tauriApi.plugin.disposeRendererSession(token)
    }
  }, [props.pluginId, props.rendererEntry, permissionKey])
  if (error) return <Alert type="error" message={error} />
  if (!route || route.pluginId !== props.pluginId) return <Spin tip="正在加载插件..." />
  return route.next ? (
    <NextPluginFrame
      key={route.next.token}
      session={route.next}
      name={props.pluginName}
      pluginId={props.pluginId}
    />
  ) : (
    <Alert type="warning" message="插件会话被拒绝，请检查插件版本和权限。原插件包和数据已保留。" />
  )
}

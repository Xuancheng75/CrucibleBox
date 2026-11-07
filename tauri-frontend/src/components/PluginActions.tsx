import { App, Button, Dropdown, Space } from 'antd'
import { useEffect, useState, type ReactNode } from 'react'
import { tauriApi, type PluginCommandContribution } from '../api/tauriApi'
import type { PluginMeta } from '../../../shared/types/plugin.types'
import PluginConfig from './PluginConfig'

export function PluginActions({ plugin, children }: { plugin: PluginMeta; children: ReactNode }) {
  const { message } = App.useApp()
  const [commands, setCommands] = useState<PluginCommandContribution[]>([])
  const [pending, setPending] = useState<string>()
  const [configOpen, setConfigOpen] = useState(false)
  const id = plugin.id
  useEffect(() => {
    let active = true
    void tauriApi.plugin.commandContributions(id).then(
      (value) => {
        if (active) setCommands(value)
      },
      () => {
        if (active) void message.error('无法加载插件操作')
      }
    )
    return () => {
      active = false
    }
  }, [id, message])

  async function run(command: PluginCommandContribution) {
    setPending(command.id)
    try {
      await tauriApi.plugin.executeCommand(id, command.id)
      void message.success(`${command.title}已完成`)
    } catch (error) {
      void message.error(error instanceof Error ? error.message : String(error))
    } finally {
      setPending(undefined)
    }
  }

  function buttons(placement: 'toolbar' | 'sidebar' | 'status') {
    return commands
      .filter((command) => command.placements.includes(placement))
      .map((command) => (
        <Button
          key={command.id}
          loading={pending === command.id}
          disabled={pending !== undefined}
          onClick={() => void run(command)}
        >
          {command.title}
        </Button>
      ))
  }

  const toolbar = buttons('toolbar')
  const sidebar = buttons('sidebar')
  const status = buttons('status')
  const menu = commands.filter((command) => command.placements.includes('contextMenu'))
  const settings = Object.keys(plugin.configSchema ?? {}).length > 0
  return (
    <>
      {toolbar.length || menu.length || settings ? (
        <Space wrap aria-label="插件操作" className="ob-plugin-contribution-strip">
          {toolbar}
          {settings ? <Button onClick={() => setConfigOpen(true)}>插件设置</Button> : null}
          {menu.length ? (
            <Dropdown
              trigger={['click', 'contextMenu']}
              menu={{
                items: menu.map((command) => ({
                  key: command.id,
                  label: command.title,
                  disabled: pending !== undefined,
                  onClick: () => void run(command)
                }))
              }}
            >
              <Button>更多操作</Button>
            </Dropdown>
          ) : null}
        </Space>
      ) : null}
      <div className="ob-plugin-contribution-layout">
        {sidebar.length ? (
          <aside aria-label="插件侧栏" className="ob-plugin-contribution-sidebar">
            {sidebar}
          </aside>
        ) : null}
        <div className="ob-plugin-contribution-content">{children}</div>
      </div>
      {status.length ? (
        <footer aria-label="插件状态栏">
          <Space wrap className="ob-plugin-contribution-strip">
            {status}
          </Space>
        </footer>
      ) : null}
      {configOpen ? (
        <PluginConfig plugin={plugin} open onClose={() => setConfigOpen(false)} />
      ) : null}
    </>
  )
}

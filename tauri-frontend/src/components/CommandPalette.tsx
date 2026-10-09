import { useEffect, useMemo, useState } from 'react'
import { AppstoreOutlined, ClockCircleOutlined, FileTextOutlined, SettingOutlined, ShopOutlined } from '@ant-design/icons'
import { message } from 'antd'
import { useAppStore } from '../store/app.store'
import { usePluginStore } from '../store/plugin.store'
import { tauriApi, type UserPluginTag, type PluginCommandContribution } from '../api/tauriApi'
import SearchDialog, { type SearchResult } from './SearchDialog'

export default function CommandPalette() {
  const open = useAppStore((state) => state.commandOpen)
  const setOpen = useAppStore((state) => state.setCommandOpen)
  const setCurrentPage = useAppStore((state) => state.setCurrentPage)
  const setActivePluginId = useAppStore((state) => state.setActivePluginId)
  const setActivityTab = useAppStore((state) => state.setActivityTab)
  const plugins = usePluginStore((state) => state.plugins)
  const [query, setQuery] = useState('')
  const [userTags, setUserTags] = useState<UserPluginTag[]>([])
  const [pluginCommands, setPluginCommands] = useState<PluginCommandContribution[]>([])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setOpen(!open)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [open, setOpen])

  useEffect(() => { if (open) setQuery('') }, [open])

  useEffect(() => {
    if (!open) return
    let active = true
    void tauriApi.plugin.tags.list().then((tags) => { if (active) setUserTags(tags) }).catch(() => { if (active) setUserTags([]) })
    void tauriApi.plugin.commandContributions().then((commands) => { if (active) setPluginCommands(commands) }).catch(() => { if (active) setPluginCommands([]) })
    return () => { active = false }
  }, [open])

  const results = useMemo((): SearchResult[] => {
    const pages = [
      { key: 'home', label: '工作台', icon: <AppstoreOutlined />, run: () => setCurrentPage('home') },
      { key: 'marketplace', label: '插件市场', icon: <ShopOutlined />, run: () => setCurrentPage('marketplace') },
      { key: 'tasks', label: '任务中心', icon: <ClockCircleOutlined />, run: () => { setActivityTab('tasks'); setCurrentPage('tasks') } },
      { key: 'logs', label: '插件日志', icon: <FileTextOutlined />, run: () => { setActivityTab('logs'); setCurrentPage('tasks') } },
      { key: 'settings', label: '设置', icon: <SettingOutlined />, run: () => setCurrentPage('settings') }
    ]
    const needle = query.trim().toLowerCase()
    return [
      ...plugins.filter((plugin) => !needle || `${plugin.displayName} ${plugin.name} ${plugin.description} ${userTags.filter((tag) => tag.pluginIds.includes(plugin.id)).map((tag) => tag.name).join(' ')}`.toLowerCase().includes(needle))
        .map((plugin) => ({ key: `plugin-${plugin.id}`, label: plugin.displayName, icon: <AppstoreOutlined />, kind: '插件', run: () => { setActivePluginId(plugin.id); setCurrentPage('pluginView') } })),
      ...pages.filter((page) => !needle || page.label.toLowerCase().includes(needle))
        .map((page) => ({ ...page, key: `page-${page.key}`, kind: '页面' })),
      ...pluginCommands.filter((command) => !needle || `${command.title} ${command.keywords.join(' ')}`.toLowerCase().includes(needle))
        .map((command) => ({
          key: `command-${command.pluginId}-${command.id}`, label: command.title,
          icon: <AppstoreOutlined />, kind: '插件命令',
          run: () => { void tauriApi.plugin.sendMessage(command.pluginId, { type: 'command.execute', commandId: command.id }).catch((error) => message.error(String(error))) }
        }))
    ]
  }, [query, plugins, userTags, pluginCommands, setActivePluginId, setActivityTab, setCurrentPage])

  return <SearchDialog open={open} onClose={() => setOpen(false)} query={query} onQueryChange={setQuery} results={results} title="命令面板" />
}

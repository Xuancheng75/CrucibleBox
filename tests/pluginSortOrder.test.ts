import { readFileSync, readdirSync } from 'node:fs'
import { extname, join, resolve } from 'node:path'
import { describe, expect, it, beforeEach, vi } from 'vitest'
import { pluginApi } from '../src/api/plugin.api'
import { usePluginStore } from '../src/store/plugin.store'
import type { PluginMeta } from '../shared/types/plugin.types'

const projectRoot = resolve(import.meta.dirname, '..')

function readSrc(...segments: string[]): string {
  return readFileSync(join(projectRoot, 'src', ...segments), 'utf8')
}

function collectSourceFiles(directory: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...collectSourceFiles(path))
    else if (['.ts', '.tsx', '.css'].includes(extname(entry.name))) files.push(path)
  }
  return files
}

const mockPlugins: PluginMeta[] = [
  {
    id: 'plugin-a',
    name: 'alpha',
    version: '1.0.0',
    displayName: 'Alpha',
    description: 'first',
    author: 'a',
    entryMain: '',
    entryRenderer: '',
    permissions: [],
    configSchema: {},
    configData: {},
    enabled: true,
    installedAt: '',
    updatedAt: ''
  },
  {
    id: 'plugin-b',
    name: 'beta',
    version: '1.0.0',
    displayName: 'Beta',
    description: 'second',
    author: 'b',
    entryMain: '',
    entryRenderer: '',
    permissions: [],
    configSchema: {},
    configData: {},
    enabled: false,
    installedAt: '',
    updatedAt: ''
  },
  {
    id: 'plugin-c',
    name: 'gamma',
    version: '1.0.0',
    displayName: 'Gamma',
    description: 'third',
    author: 'c',
    entryMain: '',
    entryRenderer: '',
    permissions: [],
    configSchema: {},
    configData: {},
    enabled: true,
    installedAt: '',
    updatedAt: ''
  }
]

describe('plugin sort order frontend contract', () => {
  it('exposes a reorder API on the plugin API client', () => {
    const apiSource = readSrc('api', 'plugin.api.ts')
    expect(apiSource).toMatch(/reorder\(\s*orderedIds:\s*string\[\]\s*\)/)
    expect(apiSource).toMatch(/reorder:\s*async\s*\(\s*orderedIds:\s*string\[\]\s*\)/)
    expect(typeof pluginApi.reorder).toBe('function')
  })

  it('declares the store reorder action and wires it to the API', () => {
    const storeSource = readSrc('store', 'plugin.store.ts')
    expect(storeSource).toContain('reorderPlugins:')
    expect(storeSource).toContain('pluginApi.reorder')
  })

  it('keeps the Home grid sortable with long-press pointer activation', () => {
    const homeSource = readSrc('pages', 'Home.tsx')
    expect(homeSource).toContain('DndContext')
    expect(homeSource).toContain('SortableContext')
    expect(homeSource).toContain('DragOverlay')
    expect(homeSource).toContain('PointerSensor')
    expect(homeSource).toContain('delay: 500')
    expect(homeSource).toContain('tolerance: 8')
    expect(homeSource).toContain('role="list"')
    expect(homeSource).toContain('aria-live="polite"')
    expect(homeSource).toContain('aria-atomic="true"')
    expect(homeSource).toContain('sort-instructions')
  })

  it('gives LauncherCard drag state, list semantics and keyboard reorder buttons', () => {
    const cardSource = readSrc('components', 'LauncherCard.tsx')
    expect(cardSource).toContain('role="listitem"')
    expect(cardSource).toContain('data-dragging')
    expect(cardSource).toContain('data-sorting')
    expect(cardSource).toContain('ArrowUpOutlined')
    expect(cardSource).toContain('ArrowDownOutlined')
    expect(cardSource).toContain('ob-launcher-reorder-up')
    expect(cardSource).toContain('ob-launcher-reorder-down')
    expect(cardSource).toContain('将 ')
    expect(cardSource).toContain(' 上移')
    expect(cardSource).toContain(' 下移')
  })

  it('styles sortable states with ob- classes and respects reduced motion', () => {
    const styles = ['base', 'cyber', 'neon']
      .map((part) => readSrc('styles', `${part}.css`))
      .join('\n')
    expect(styles).toContain('.ob-sr-only')
    expect(styles).toContain('.ob-sortable-list')
    expect(styles).toContain('.ob-launcher[data-dragging')
    expect(styles).toContain('.ob-launcher-drag-overlay')
    expect(styles).toContain('.ob-sortable-list *')
    expect(styles).toContain('prefers-reduced-motion')
  })

  it('does not leak internal Ant Design DOM selectors in new sortable code', () => {
    const antdInternal = /\.ant-[a-z0-9-]+/i
    for (const file of collectSourceFiles(join(projectRoot, 'src'))) {
      const source = readFileSync(file, 'utf8')
      expect(source, file).not.toMatch(antdInternal)
    }
  })
})

describe('plugin reorder store action', () => {
  beforeEach(() => {
    usePluginStore.setState({
      plugins: mockPlugins.map((p) => ({ ...p })),
      loading: false,
      error: null,
      activePlugins: {}
    })
    vi.restoreAllMocks()
  })

  it('optimistically updates plugin order and keeps it on success', async () => {
    vi.spyOn(pluginApi, 'reorder').mockResolvedValue({ success: true })
    const store = usePluginStore.getState()

    const ok = await store.reorderPlugins(['plugin-c', 'plugin-a', 'plugin-b'])

    expect(ok).toBe(true)
    expect(usePluginStore.getState().plugins.map((p) => p.id)).toEqual([
      'plugin-c',
      'plugin-a',
      'plugin-b'
    ])
    expect(usePluginStore.getState().error).toBeNull()
    expect(pluginApi.reorder).toHaveBeenCalledWith(['plugin-c', 'plugin-a', 'plugin-b'])
  })

  it('reverts the order and surfaces the error when the backend fails', async () => {
    vi.spyOn(pluginApi, 'reorder').mockResolvedValue({ success: false, error: 'network error' })
    const store = usePluginStore.getState()
    const originalOrder = store.plugins.map((p) => p.id)

    const ok = await store.reorderPlugins(['plugin-c', 'plugin-a', 'plugin-b'])

    expect(ok).toBe(false)
    expect(usePluginStore.getState().plugins.map((p) => p.id)).toEqual(originalOrder)
    expect(usePluginStore.getState().error).toBe('network error')
  })

  it('reverts the order when the API throws', async () => {
    vi.spyOn(pluginApi, 'reorder').mockRejectedValue(new Error('ipc timeout'))
    const store = usePluginStore.getState()
    const originalOrder = store.plugins.map((p) => p.id)

    const ok = await store.reorderPlugins(['plugin-c', 'plugin-a', 'plugin-b'])

    expect(ok).toBe(false)
    expect(usePluginStore.getState().plugins.map((p) => p.id)).toEqual(originalOrder)
    expect(usePluginStore.getState().error).toBe('ipc timeout')
  })

  it('rejects unknown plugin ids without calling the backend', async () => {
    vi.spyOn(pluginApi, 'reorder').mockResolvedValue({ success: true })
    const store = usePluginStore.getState()
    const originalOrder = store.plugins.map((p) => p.id)

    const ok = await store.reorderPlugins(['plugin-z', 'plugin-a', 'plugin-b'])

    expect(ok).toBe(false)
    expect(usePluginStore.getState().plugins.map((p) => p.id)).toEqual(originalOrder)
    expect(pluginApi.reorder).not.toHaveBeenCalled()
  })
})

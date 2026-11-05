import type { ReactNode } from 'react'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  buttons: new Map<string, () => void>(),
  openFile: vi.fn(),
  openDirectory: vi.fn(),
  openFiles: vi.fn(),
  installPlugin: vi.fn(),
  enqueueInstalls: vi.fn(),
  error: vi.fn(),
  warning: vi.fn()
}))

vi.mock('../tauri-frontend/src/api/tauriApi', () => ({ tauriApi: { dialog: mocks } }))
vi.mock('../tauri-frontend/src/store/plugin.store', () => {
  const state = {
    installPlugin: mocks.installPlugin,
    enqueueInstalls: mocks.enqueueInstalls,
    error: '包清单错误'
  }
  return {
    usePluginStore: Object.assign((selector: (value: typeof state) => unknown) => selector(state), {
      getState: () => state
    })
  }
})
vi.mock('antd', async () => {
  const { createElement: element } = await import('react')
  const container = ({ children }: { children?: ReactNode }) => element('div', null, children)
  return {
    App: { useApp: () => ({ message: { error: mocks.error, warning: mocks.warning } }) },
    Modal: container,
    Space: container,
    Typography: { Text: container },
    Button: ({ children, onClick }: { children?: ReactNode; onClick: () => void }) => {
      mocks.buttons.set(String(children), onClick)
      return null
    }
  }
})

import PluginImport from '../tauri-frontend/src/components/PluginImport'

describe('plugin import dialog', () => {
  const onClose = vi.fn()
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.buttons.clear()
    mocks.openFile.mockResolvedValue(null)
    mocks.openDirectory.mockResolvedValue(null)
    mocks.openFiles.mockResolvedValue([])
    renderToStaticMarkup(createElement(PluginImport, { open: true, onClose }))
  })

  it.each(['选择 .zip 插件包', '选择插件目录'])(
    'treats cancelling %s as a normal exit',
    async (label) => {
      mocks.buttons.get(label)!()
      await new Promise<void>((resolve) => setImmediate(resolve))
      expect(mocks.installPlugin).not.toHaveBeenCalled()
      expect(mocks.error).not.toHaveBeenCalled()
      expect(onClose).not.toHaveBeenCalled()
    }
  )

  it('reports an actual dialog failure once', async () => {
    mocks.openFile.mockRejectedValue(new Error('对话框不可用'))
    mocks.buttons.get('选择 .zip 插件包')!()
    await new Promise<void>((resolve) => setImmediate(resolve))
    expect(mocks.error).toHaveBeenCalledExactlyOnceWith('导入失败：对话框不可用')
  })

  it('reports a failed package preview without closing the import dialog', async () => {
    mocks.openFile.mockResolvedValue('sample.zip')
    mocks.installPlugin.mockResolvedValue(false)
    mocks.buttons.get('选择 .zip 插件包')!()
    await new Promise<void>((resolve) => setImmediate(resolve))
    expect(mocks.error).toHaveBeenCalledExactlyOnceWith('插件导入失败：包清单错误')
    expect(onClose).not.toHaveBeenCalled()
  })
})

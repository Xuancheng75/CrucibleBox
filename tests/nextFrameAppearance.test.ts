import { readFileSync } from 'node:fs'
import { runInNewContext } from 'node:vm'
import { expect, test, vi } from 'vitest'
import { validateAppearance, validateThemeStyle } from '../shared/frame-validation.mjs'
import { themeToCssVars } from '../shared/themes/css-vars'
import { PRESET_THEMES } from '../shared/themes/presets'
const appearance = (index: number) => ({
  mode: PRESET_THEMES[index].mode,
  cssVars: themeToCssVars(PRESET_THEMES[index])
})
function fixture() {
  const source = readFileSync('src-tauri/src/next_frame_runtime.js', 'utf8')
    .replace(
      /import\(\s*new URL\('\.\/frame-validation\.mjs', location\.href\)\.href\s*\)/,
      "__import('api')"
    )
    .replace("import(new URL('./renderer.js', location.href).href)", "__import('renderer')")
  const listeners = new Map<string, (event: unknown) => unknown>()
  const css = new Map<string, string>()
  const elements: Array<{ tag: string; textContent: string; removed: boolean }> = []
  const createElement = (tag: string) => {
    const element = {
      tag,
      textContent: '',
      removed: false,
      dataset: {},
      className: '',
      setAttribute: vi.fn(),
      append: vi.fn(),
      remove() {
        this.removed = true
      }
    }
    elements.push(element)
    return element
  }
  let release!: () => void
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  const cleanup = vi.fn()
  const mounted = vi.fn((_context: unknown) => cleanup)
  const root = {
    dataset: {
      sessionToken: 'b'.repeat(64),
      maxInflight: '32',
      requestTimeout: '10000',
      handshakeTimeout: '10000'
    },
    textContent: ''
  }
  const parent = { postMessage: vi.fn() }
  const doc = {
    style: { setProperty: (key: string, value: string) => css.set(key, value), colorScheme: '' },
    dataset: { obTheme: '' }
  }
  const port = {
    onmessage: null as null | ((event: { data: unknown }) => void),
    onmessageerror: null,
    closed: false,
    postMessage: vi.fn(),
    start: vi.fn(),
    close: vi.fn(function () {
      port.closed = true
    })
  }
  runInNewContext(source, {
    document: {
      getElementById: () => root,
      documentElement: doc,
      createElement,
      head: { append: vi.fn() },
      body: { prepend: vi.fn() }
    },
    parent,
    console,
    setTimeout,
    clearTimeout,
    addEventListener: (kind: string, listener: (event: unknown) => unknown) =>
      listeners.set(kind, listener),
    removeEventListener: (kind: string) => listeners.delete(kind),
    __import: async (kind: string) => {
      if (kind === 'api') {
        await gate
        return { validateAppearance, validateThemeStyle }
      }
      return { mount: mounted }
    }
  })
  return {
    release,
    root,
    doc,
    port,
    css,
    elements,
    mounted,
    cleanup,
    connect: (value: unknown) =>
      listeners.get('message')?.({
        source: parent,
        data: {
          kind: 'next-connect',
          nonce: 'b'.repeat(64),
          wireVersion: 4,
          session: 'a'.repeat(64),
          appearance: value
        },
        ports: [port]
      }),
    close: () => listeners.get('pagehide')?.({})
  }
}
test('appearance module loads before port starts; valid updates notify and invalid update disposes once', async () => {
  const f = fixture()
  const connecting = f.connect(appearance(0))
  expect(f.port.onmessage).toBeNull()
  expect(f.port.start).not.toHaveBeenCalled()
  f.release()
  await connecting
  expect(f.doc.style.colorScheme).toBe(PRESET_THEMES[0].mode)
  expect(f.css.get('background')).toBe(PRESET_THEMES[0].tokens.colorBgLayout)
  expect(f.mounted).toHaveBeenCalledOnce()
  const context = f.mounted.mock.calls[0] as unknown as [
    { onAppearanceChanged: (listener: (v: unknown) => void) => () => void }
  ]
  const listener = vi.fn()
  context[0].onAppearanceChanged(listener)
  f.port.onmessage?.({ data: { kind: 'next-appearance', appearance: appearance(1) } })
  expect(listener).toHaveBeenCalledOnce()
  expect(f.css.get('--ob-color-primary')).toBe(PRESET_THEMES[1].tokens.colorPrimary)
  expect(f.css.get('background')).toBe(PRESET_THEMES[1].tokens.colorBgLayout)
  f.port.onmessage?.({
    data: {
      kind: 'next-appearance',
      appearance: { ...appearance(1), cssVars: { ...appearance(1).cssVars, '--foreign': 'bad' } }
    }
  })
  expect(f.port.closed).toBe(true)
  expect(f.css.has('--foreign')).toBe(false)
  expect(f.cleanup).toHaveBeenCalledOnce()
  f.close()
  expect(f.cleanup).toHaveBeenCalledOnce()
})
test('invalid initial appearance closes connection before plugin mount', async () => {
  const f = fixture()
  const connecting = f.connect({ mode: 'dark', cssVars: {} })
  f.release()
  await connecting
  expect(f.port.closed).toBe(true)
  expect(f.mounted).not.toHaveBeenCalled()
  f.close()
})

test('retained renderer receives current visibility and can unsubscribe', async () => {
  const f = fixture()
  f.release()
  await f.connect(appearance(0))
  const context = f.mounted.mock.calls[0][0] as unknown as {
    onVisibilityChanged: (listener: (visible: boolean) => void) => () => void
  }
  const listener = vi.fn()
  const stop = context.onVisibilityChanged(listener)
  expect(listener).toHaveBeenLastCalledWith(true)
  f.port.onmessage?.({ data: { kind: 'next-visibility', visible: false } })
  expect(listener).toHaveBeenLastCalledWith(false)
  stop()
  f.port.onmessage?.({ data: { kind: 'next-visibility', visible: true } })
  expect(listener).toHaveBeenCalledTimes(2)
  f.close()
})
test('v2 styles require negotiation, replace safely and clear on v1 switch and disposal', async () => {
  const f = fixture()
  const connecting = f.connect(appearance(0))
  f.release()
  await connecting
  expect(f.port.postMessage).not.toHaveBeenCalledWith(
    expect.objectContaining({ kind: 'next-theme-style-subscribe' })
  )
  const context = (
    f.mounted.mock.calls[0] as unknown as [{ enableThemeStyle: (version: number) => boolean }]
  )[0]
  expect(context.enableThemeStyle(3)).toBe(false)
  expect(context.enableThemeStyle(2)).toBe(true)
  context.enableThemeStyle(2)
  expect(
    f.port.postMessage.mock.calls.filter(([value]) => value.kind === 'next-theme-style-subscribe')
  ).toHaveLength(1)
  const style = {
    version: 2,
    css: '.sample{color:red}',
    motion: 'off',
    layers: [{ id: 'caption', scope: 'workspace', text: '<script>plain text</script>' }]
  }
  f.port.onmessage?.({ data: { kind: 'next-theme-style', style } })
  expect(f.elements.find((item) => item.tag === 'style')?.textContent).toBe(style.css)
  expect(f.elements.some((item) => item.textContent === style.layers[0].text)).toBe(true)
  f.port.onmessage?.({ data: { kind: 'next-visibility', visible: false } })
  expect((f.doc.dataset as Record<string, string>).obPageVisible).toBe('false')
  f.port.onmessage?.({
    data: { kind: 'next-theme-style', style: { ...style, css: '', layers: [] } }
  })
  expect(f.elements.filter((item) => item.tag === 'style').every((item) => item.removed)).toBe(true)
  expect((f.doc.dataset as Record<string, string>).obThemeVersion).toBeUndefined()
  f.port.onmessage?.({ data: { kind: 'next-theme-style', style } })
  f.close()
  expect(f.elements.filter((item) => item.tag === 'style').every((item) => item.removed)).toBe(true)
})

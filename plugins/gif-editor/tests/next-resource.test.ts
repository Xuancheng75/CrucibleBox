import { afterEach, describe, expect, it, vi } from 'vitest'
import { createRoot } from 'react-dom/client'
import type { ReactElement, ReactNode } from 'react'
import { mount } from '../src/renderer-entry'
import { fetchResidueWorkerSource } from '../src/residue-worker'
vi.mock('react-dom/client', () => ({ createRoot: vi.fn() }))
afterEach(() => vi.unstubAllGlobals())
describe('GIF residue worker resource loader', () => {
  it('fetches the declared same-session classic worker resource', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => new Response('self.onmessage = null'))
    vi.stubGlobal('fetch', fetchMock)
    const controller = new AbortController()

    await expect(fetchResidueWorkerSource(controller.signal)).resolves.toBe('self.onmessage = null')

    const [resource, options] = fetchMock.mock.calls[0]
    expect(String(resource)).toMatch(/\/dist\/workers\/residue\.js$/)
    expect(options?.credentials).toBe('same-origin')
    expect(options?.redirect).toBe('error')
    expect(options?.signal).toBe(controller.signal)
  })

  it('rejects non-success HTTP responses and worker sources above 2 MiB', async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(new Response('', { status: 404 }))
      .mockResolvedValueOnce(
        new Response('too large', {
          headers: { 'content-length': String(2 * 1024 * 1024 + 1) }
        })
      )
      .mockResolvedValueOnce(
        new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              controller.enqueue(new Uint8Array(2 * 1024 * 1024))
              controller.enqueue(new Uint8Array(1))
              controller.close()
            }
          })
        )
      )
    vi.stubGlobal('fetch', fetchMock)

    await expect(fetchResidueWorkerSource()).rejects.toThrow('HTTP 404')
    await expect(fetchResidueWorkerSource()).rejects.toThrow('exceeds 2 MiB')
    await expect(fetchResidueWorkerSource()).rejects.toThrow('exceeds 2 MiB')
  })

  it('honors abort before and during fetch', async () => {
    const controller = new AbortController()
    const preAbortedFetch = vi.fn<typeof fetch>()
    vi.stubGlobal('fetch', preAbortedFetch)
    controller.abort()
    await expect(fetchResidueWorkerSource(controller.signal)).rejects.toMatchObject({
      name: 'AbortError'
    })
    expect(preAbortedFetch).not.toHaveBeenCalled()

    const inFlight = new AbortController()
    const pendingFetch = vi.fn<typeof fetch>(
      (_resource, options) =>
        new Promise<Response>((_resolve, reject) => {
          options?.signal?.addEventListener(
            'abort',
            () => reject(new DOMException('aborted', 'AbortError')),
            { once: true }
          )
        })
    )
    vi.stubGlobal('fetch', pendingFetch)
    const pending = fetchResidueWorkerSource(inFlight.signal)
    inFlight.abort()
    await expect(pending).rejects.toMatchObject({ name: 'AbortError' })
    expect(pendingFetch.mock.calls[0][1]?.signal).toBe(inFlight.signal)
  })
})

describe('GIF renderer UI adapter', () => {
  it('maps notifications to the visible in-frame live status and cleans up', () => {
    type FakeElement = {
      className: string
      style: Record<string, string>
      textContent: string | null
      attributes: Record<string, string>
      setAttribute(name: string, value: string): void
    }

    const elements: FakeElement[] = []
    const fakeDocument = {
      createElement: vi.fn((_tag: string) => {
        const element: FakeElement = {
          className: '',
          style: {},
          textContent: '',
          attributes: {},
          setAttribute(name, value) {
            this.attributes[name] = value
          }
        }
        elements.push(element)
        return element
      })
    }
    vi.stubGlobal('document', fakeDocument as unknown as Document)

    const render = vi.fn<(node: ReactNode) => void>()
    const unmount = vi.fn<() => void>()
    vi.mocked(createRoot).mockReset()
    vi.mocked(createRoot).mockReturnValue({ render, unmount } as unknown as ReturnType<
      typeof createRoot
    >)

    const replaceChildren = vi.fn<(...nodes: unknown[]) => void>()
    const context = {
      root: { replaceChildren }
    } as unknown as Parameters<typeof mount>[0]
    const cleanup = mount(context)

    expect(fakeDocument.createElement).toHaveBeenCalledTimes(2)
    const statusRegion = elements[0]
    expect(statusRegion.className).toBe('gif-editor-runtime-status')
    expect(statusRegion.textContent).toBe('就绪')
    expect(statusRegion.attributes).toMatchObject({
      role: 'status',
      'aria-live': 'polite',
      'aria-atomic': 'true'
    })
    expect(statusRegion.style.color).toBe('var(--ob-color-text)')
    expect(statusRegion.style.backgroundColor).toBe('var(--ob-color-bg-container)')

    const rendered = render.mock.calls[0]?.[0]
    expect(rendered).toBeDefined()
    const props = (
      rendered as ReactElement<{ api: { notify(title: string, message: string): void } }>
    ).props
    props.api.notify('导出完成', 'edited.gif')
    expect(statusRegion.textContent).toBe('导出完成：edited.gif')

    cleanup()
    cleanup()
    expect(unmount).toHaveBeenCalledOnce()
    expect(replaceChildren).toHaveBeenCalledTimes(2)
  })
})

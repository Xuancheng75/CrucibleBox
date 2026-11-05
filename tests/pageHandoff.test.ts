import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { cardVariants } from '../tauri-frontend/src/theme/card-variant'

vi.mock('react-dom', () => ({ flushSync: (update: () => void) => update() }))

let content: { inert: boolean }
let transitions: Array<{
  update: () => void
  finish: () => void
  skipTransition: ReturnType<typeof vi.fn>
}>
let start: ReturnType<typeof vi.fn>

beforeEach(() => {
  vi.resetModules()
  content = { inert: false }
  transitions = []
  start = vi.fn((update: () => void) => {
    let finish!: () => void
    const finished = new Promise<void>((resolve) => {
      finish = resolve
    })
    const skipTransition = vi.fn()
    transitions.push({ update, finish, skipTransition })
    return { ready: Promise.resolve(), finished, skipTransition }
  })
  vi.stubGlobal('document', {
    documentElement: {},
    querySelector: () => content,
    startViewTransition: start
  })
  vi.stubGlobal('window', { matchMedia: () => ({ matches: false }) })
  vi.stubGlobal('getComputedStyle', () => ({
    getPropertyValue: (key: string) => (key === '--ob-page-handoff' ? '1' : 'block')
  }))
})

afterEach(async () => {
  const { discardPageChange } = await import('../tauri-frontend/src/theme/page-handoff')
  discardPageChange()
  await Promise.resolve()
  transitions.forEach((item) => item.finish())
  await Promise.resolve()
  vi.unstubAllGlobals()
})

test('batches plugin and page changes after the browser captures the outgoing view', async () => {
  const { changePage } = await import('../tauri-frontend/src/theme/page-handoff')
  const changes: string[] = []
  changePage(() => changes.push('plugin'))
  changePage(() => changes.push('page'))
  expect(changes).toEqual([])
  await Promise.resolve()
  expect(start).toHaveBeenCalledOnce()
  expect(content.inert).toBe(true)
  transitions[0].update()
  expect(changes).toEqual(['plugin', 'page'])
  transitions[0].finish()
  await Promise.resolve()
  expect(content.inert).toBe(false)
})

test('rapid navigation uses the last target and an older finish cannot unlock the new transition', async () => {
  const { changePage } = await import('../tauri-frontend/src/theme/page-handoff')
  const changes: string[] = []
  changePage(() => changes.push('old'))
  await Promise.resolve()
  changePage(() => changes.push('latest'))
  await Promise.resolve()
  expect(transitions[0].skipTransition).toHaveBeenCalled()
  transitions[0].update()
  transitions[1].update()
  expect(changes).toEqual(['latest'])
  transitions[0].finish()
  await Promise.resolve()
  expect(content.inert).toBe(true)
  transitions[1].finish()
  await Promise.resolve()
  expect(content.inert).toBe(false)
})

test('selecting the original page cancels an unapplied navigation', async () => {
  const { changePage } = await import('../tauri-frontend/src/theme/page-handoff')
  const changes: string[] = []
  changePage(() => changes.push('away'))
  await Promise.resolve()
  changePage(() => changes.push('stay'), false)
  transitions[0].update()
  expect(changes).toEqual(['stay'])
})

test('removing the active plugin invalidates an unapplied destination', async () => {
  const { changePage, discardPageChange } = await import('../tauri-frontend/src/theme/page-handoff')
  const update = vi.fn()
  changePage(update)
  await Promise.resolve()
  discardPageChange()
  transitions[0].update()
  expect(update).not.toHaveBeenCalled()
})

test('reduced motion commits immediately without a browser transition', async () => {
  vi.stubGlobal('window', { matchMedia: () => ({ matches: true }) })
  const { changePage } = await import('../tauri-frontend/src/theme/page-handoff')
  const update = vi.fn()
  changePage(update)
  expect(update).toHaveBeenCalledOnce()
  await Promise.resolve()
  expect(start).not.toHaveBeenCalled()
})

test('card variants cover four designs and remain stable when items reorder', () => {
  const ids = ['a', 'b', 'c', 'd']
  const before = cardVariants(ids)
  expect(new Set(before.values()).size).toBe(4)
  const after = cardVariants(ids.reverse())
  for (const id of ids) expect(after.get(id)).toBe(before.get(id))
})

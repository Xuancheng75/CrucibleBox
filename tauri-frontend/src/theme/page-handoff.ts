import { flushSync } from 'react-dom'

let pending: Array<() => void> = []
let scheduled = false
let generation = 0
let active: ViewTransition | undefined

export function skipPageHandoff() {
  active?.skipTransition()
}

export function discardPageChange() {
  generation++
  pending = []
  skipPageHandoff()
}

export function changePage(update: () => void, changed = true) {
  const supported =
    typeof document !== 'undefined' && typeof document.startViewTransition === 'function'
  const enabled =
    supported &&
    !window.matchMedia('(prefers-reduced-motion: reduce)').matches &&
    getComputedStyle(document.documentElement).getPropertyValue('--ob-page-handoff').trim() ===
      '1' &&
    getComputedStyle(document.documentElement)
      .getPropertyValue('--ob-page-transition-display')
      .trim() === 'block'
  if (!enabled) {
    generation++
    skipPageHandoff()
    const queued = pending
    pending = []
    for (const change of queued) change()
    update()
    return
  }
  if (!changed && !scheduled) {
    generation++
    skipPageHandoff()
    update()
    return
  }
  pending.push(update)
  if (scheduled) return
  scheduled = true
  queueMicrotask(() => {
    scheduled = false
    const changes = pending
    pending = []
    if (!changes.length) return
    const ticket = ++generation
    skipPageHandoff()
    const content = document.querySelector<HTMLElement>('.ob-main-content')!
    content.inert = true
    const transition = document.startViewTransition(() => {
      if (ticket !== generation) return
      flushSync(() => {
        for (const change of changes) change()
      })
    })
    active = transition
    const release = () => {
      if (active !== transition) return
      active = undefined
      content.inert = false
    }
    void transition.ready.catch((error: unknown) => {
      if (!(error instanceof DOMException && error.name === 'AbortError'))
        console.warn('Page handoff skipped', error)
    })
    void transition.finished.then(release, (error: unknown) => {
      release()
      console.error('Page navigation failed', error)
    })
  })
}

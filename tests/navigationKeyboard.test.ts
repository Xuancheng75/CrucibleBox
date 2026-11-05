import { describe, expect, it, vi } from 'vitest'
import type { KeyboardEvent } from 'react'
import IconRail from '../tauri-frontend/src/components/IconRail'

vi.mock('antd', () => ({ Tooltip: 'span' }))

function navigation(orientation: 'rail' | 'top' | 'menu') {
  const change = vi.fn()
  const element = IconRail({
    appVersion: 'test',
    selectedKey: 'home',
    onChange: change,
    compact: false,
    onCompactChange: vi.fn(),
    orientation
  })
  const handler = element.props.onKeyDown as (event: KeyboardEvent<HTMLElement>) => void
  const buttons = Array.from({ length: 4 }, () => ({ focus: vi.fn() }))
  const key = (name: string, index: number, overrides = {}) => {
    const preventDefault = vi.fn()
    handler({
      key: name,
      target: buttons[index],
      currentTarget: { querySelectorAll: () => buttons },
      nativeEvent: { isComposing: false },
      preventDefault,
      ...overrides
    } as unknown as KeyboardEvent<HTMLElement>)
    return preventDefault
  }
  return { buttons, key, change }
}

describe('generic application navigation keyboard focus', () => {
  it.each(['rail', 'menu'] as const)(
    'moves vertically and wraps in %s navigation',
    (orientation) => {
      const { buttons, key, change } = navigation(orientation)
      expect(key('ArrowDown', 3)).toHaveBeenCalledOnce()
      expect(buttons[0].focus).toHaveBeenCalledOnce()
      key('ArrowUp', 0)
      expect(buttons[3].focus).toHaveBeenCalledOnce()
      key('Home', 2)
      key('End', 1)
      expect(buttons[0].focus).toHaveBeenCalledTimes(2)
      expect(buttons[3].focus).toHaveBeenCalledTimes(2)
      expect(change).not.toHaveBeenCalled()
    }
  )
  it('moves horizontally for top navigation and leaves vertical keys unchanged', () => {
    const { buttons, key } = navigation('top')
    key('ArrowRight', 0)
    key('ArrowLeft', 0)
    expect(buttons[1].focus).toHaveBeenCalledOnce()
    expect(buttons[3].focus).toHaveBeenCalledOnce()
    expect(key('ArrowDown', 0)).not.toHaveBeenCalled()
  })
  it('preserves activation, tab order, input composition and modified shortcuts', () => {
    const { buttons, key } = navigation('menu')
    for (const name of ['Tab', 'Enter', ' ', 'Escape']) expect(key(name, 0)).not.toHaveBeenCalled()
    for (const override of [
      { ctrlKey: true },
      { altKey: true },
      { metaKey: true },
      { shiftKey: true },
      { nativeEvent: { isComposing: true } }
    ])
      expect(key('ArrowDown', 0, override)).not.toHaveBeenCalled()
    expect(key('ArrowDown', -1)).not.toHaveBeenCalled()
    expect(buttons.every((button) => button.focus.mock.calls.length === 0)).toBe(true)
  })
})

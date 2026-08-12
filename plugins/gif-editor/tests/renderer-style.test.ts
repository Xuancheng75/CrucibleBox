import { describe, expect, it } from 'vitest'
import { retainDynamicStyle } from '../src/renderer-style'

interface FakeStyle {
  dataset: Record<string, string>
  id: string
  isConnected: boolean
  remove: () => void
  textContent: string | null
}

function createFakeDocument(): { document: Document; styles: Map<string, FakeStyle> } {
  const styles = new Map<string, FakeStyle>()
  const document_ = {
    createElement: () => {
      const style: FakeStyle = {
        dataset: {},
        id: '',
        isConnected: false,
        remove: () => {
          style.isConnected = false
          styles.delete(style.id)
        },
        textContent: null
      }
      return style
    },
    getElementById: (id: string) => styles.get(id) ?? null,
    head: {
      appendChild: (style: FakeStyle) => {
        style.isConnected = true
        styles.set(style.id, style)
        return style
      }
    }
  }

  return { document: document_ as unknown as Document, styles }
}

describe('renderer dynamic style lifecycle', () => {
  it('updates existing css and removes it after the final consumer unmounts', () => {
    const { document, styles } = createFakeDocument()
    const releaseFirst = retainDynamicStyle(document, 'plugin-style', '.old {}')
    const releaseSecond = retainDynamicStyle(document, 'plugin-style', '.new {}')
    const style = styles.get('plugin-style')!

    expect(style.textContent).toBe('.new {}')
    expect(style.dataset.pluginRefCount).toBe('2')

    releaseFirst()
    expect(styles.has('plugin-style')).toBe(true)
    expect(style.dataset.pluginRefCount).toBe('1')

    releaseSecond()
    expect(styles.has('plugin-style')).toBe(false)
  })
})

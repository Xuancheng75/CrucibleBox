import { describe, expect, it, vi } from 'vitest'
import {
  appendHistoryEntry,
  applyHistoryDelta,
  applyFilterValues,
  cloneHistoryEntry,
  createHistoryEntry,
  hasPendingFilters,
  reconcileThumbnailCache,
  type FrameState,
  type HistoryEntry
} from '../src/renderer-state'

type TestImage = { pixels: number[]; bytes: number }

function frame(id: string, imageData: TestImage): FrameState<TestImage> {
  return { id, imageData, delay: 100 }
}

function entry(id: string, byteLength: number): HistoryEntry<TestImage> {
  return {
    kind: 'snapshot',
    frames: [frame(id, { pixels: [byteLength], bytes: byteLength })],
    width: 1,
    height: 1,
    byteLength
  }
}

describe('renderer history state', () => {
  it('deep-clones image data once while preserving frame ids', () => {
    const originalImage = { pixels: [1, 2, 3, 4], bytes: 4 }
    const cloneImage = vi.fn((image: TestImage) => ({ ...image, pixels: [...image.pixels] }))

    const snapshot = cloneHistoryEntry(
      { frames: [frame('frame-a', originalImage)], width: 1, height: 1 },
      cloneImage,
      (image) => image.bytes
    )

    expect(cloneImage).toHaveBeenCalledTimes(1)
    expect(snapshot.frames[0].id).toBe('frame-a')
    expect(snapshot.frames[0].imageData).not.toBe(originalImage)
    expect(snapshot.frames[0].imageData.pixels).toEqual(originalImage.pixels)
    expect(snapshot.byteLength).toBe(4)
  })

  it('evicts oldest snapshots by byte budget and operation count', () => {
    const limits = { maxEntries: 3, maxBytes: 10 }
    let stack: HistoryEntry<TestImage>[] = []
    stack = appendHistoryEntry(stack, entry('a', 4), limits)
    stack = appendHistoryEntry(stack, entry('b', 4), limits)
    stack = appendHistoryEntry(stack, entry('c', 4), limits)
    expect(stack.map((item) => item.frames[0].id)).toEqual(['b', 'c'])

    stack = appendHistoryEntry(stack, entry('d', 2), limits)
    stack = appendHistoryEntry(stack, entry('e', 2), limits)
    expect(stack.map((item) => item.frames[0].id)).toEqual(['c', 'd', 'e'])
  })

  it('keeps one latest snapshot when it alone exceeds the byte budget', () => {
    const stack = appendHistoryEntry([entry('old', 4)], entry('large', 20), {
      maxEntries: 50,
      maxBytes: 10
    })
    expect(stack.map((item) => item.frames[0].id)).toEqual(['large'])
  })

  it('stores localized pixel edits as a reversible XOR delta', () => {
    const beforePixels = Array.from({ length: 400 }, () => 0)
    const afterPixels = [...beforePixels]
    afterPixels[201] = 255
    const before = {
      frames: [frame('stable', { pixels: beforePixels, bytes: beforePixels.length })],
      width: 10,
      height: 10
    }
    const after = {
      frames: [frame('stable', { pixels: afterPixels, bytes: afterPixels.length })],
      width: 10,
      height: 10
    }
    const history = createHistoryEntry(
      before,
      after,
      (image) => ({ ...image, pixels: [...image.pixels] }),
      (image) => Uint8Array.from(image.pixels),
      (image) => image.bytes
    )

    expect(history?.kind).toBe('delta')
    if (!history || history.kind !== 'delta') throw new Error('expected delta history')
    expect(history.byteLength).toBeLessThan(beforePixels.length)

    const undone = applyHistoryDelta(
      after,
      history,
      'undo',
      (image) => Uint8Array.from(image.pixels),
      (image, bytes) => ({ ...image, pixels: Array.from(bytes) })
    )
    expect(undone.frames[0].imageData.pixels).toEqual(beforePixels)

    const redone = applyHistoryDelta(
      undone,
      history,
      'redo',
      (image) => Uint8Array.from(image.pixels),
      (image, bytes) => ({ ...image, pixels: Array.from(bytes) })
    )
    expect(redone.frames[0].imageData.pixels).toEqual(afterPixels)
  })

  it('falls back to a snapshot for structural edits and ignores no-op copies', () => {
    const image = { pixels: [1, 2, 3, 4], bytes: 4 }
    const before = { frames: [frame('a', image)], width: 1, height: 1 }
    const structural = { frames: [frame('b', image)], width: 1, height: 1 }
    const clone = (value: TestImage) => ({ ...value, pixels: [...value.pixels] })
    const bytes = (value: TestImage) => Uint8Array.from(value.pixels)

    expect(createHistoryEntry(before, structural, clone, bytes, (value) => value.bytes)?.kind).toBe(
      'snapshot'
    )
    expect(
      createHistoryEntry(before, { ...before }, clone, bytes, (value) => value.bytes)
    ).toBeNull()
  })
})

describe('renderer thumbnail state', () => {
  it('reuses unchanged frames and refreshes added or edited frame sources', () => {
    const first = { pixels: [1], bytes: 1 }
    const second = { pixels: [2], bytes: 1 }
    const render = vi.fn((item: FrameState<TestImage>) => `${item.id}:${item.imageData.pixels[0]}`)

    const initial = reconcileThumbnailCache([frame('a', first)], new Map(), render)
    expect(render).toHaveBeenCalledTimes(1)

    const added = reconcileThumbnailCache([frame('a', first), frame('b', second)], initial, render)
    expect(render).toHaveBeenCalledTimes(2)
    expect(added.get('a')).toBe(initial.get('a'))

    const editedImage = { pixels: [9], bytes: 1 }
    const edited = reconcileThumbnailCache(
      [frame('a', editedImage), frame('b', second)],
      added,
      render
    )
    expect(render).toHaveBeenCalledTimes(3)
    expect(edited.get('a')?.url).toBe('a:9')
    expect(edited.get('b')).toBe(added.get('b'))
  })

  it('refreshes restored image sources while retaining stable ids', () => {
    const before = { pixels: [1], bytes: 1 }
    const after = { pixels: [2], bytes: 1 }
    const render = vi.fn((item: FrameState<TestImage>) => String(item.imageData.pixels[0]))
    const current = reconcileThumbnailCache([frame('stable-id', after)], new Map(), render)
    const restored = reconcileThumbnailCache([frame('stable-id', before)], current, render)

    expect(restored.get('stable-id')?.url).toBe('1')
    expect(render).toHaveBeenCalledTimes(2)
  })
})

describe('filter preview state', () => {
  it('applies pending values once in display order and skips neutral values', () => {
    const calls: string[] = []
    const transforms = {
      brightness: (amount: number) => (value: number) => {
        calls.push(`brightness:${amount}`)
        return value + 1
      },
      contrast: (amount: number) => (value: number) => {
        calls.push(`contrast:${amount}`)
        return value * 2
      },
      saturation: (amount: number) => (value: number) => {
        calls.push(`saturation:${amount}`)
        return value - 3
      }
    }

    expect(
      applyFilterValues(2, { brightness: 0.5, contrast: 0, saturation: -0.5 }, transforms)
    ).toBe(0)
    expect(calls).toEqual(['brightness:0.5', 'saturation:-0.5'])
    expect(hasPendingFilters({ brightness: 0, contrast: 0, saturation: 0 })).toBe(false)
  })
})

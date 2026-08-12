import { describe, expect, it, vi } from 'vitest'
import {
  applyCanvasTransform,
  cropCanvasToFrameBounds,
  cropCanvasToUnionBounds
} from '../src/document-operations'
import type { GifDocument, GifFrame, ImageTransform } from '../src/types'
import { GifValidationError } from '../src/utils/gif-validation'

type Rgba = readonly [number, number, number, number]

const RED: Rgba = [255, 0, 0, 255]
const GREEN: Rgba = [0, 255, 0, 255]
const BLUE: Rgba = [0, 0, 255, 255]
const CLEAR: Rgba = [0, 0, 0, 0]

function imageData(
  width: number,
  height: number,
  pixels: readonly { x: number; y: number; rgba: Rgba }[] = []
): ImageData {
  const data = new Uint8ClampedArray(width * height * 4)
  for (const { x, y, rgba } of pixels) {
    data.set(rgba, (y * width + x) * 4)
  }
  return new ImageData(data, width, height)
}

function frame(id: string, delay: number, data: ImageData): GifFrame {
  return { id, delay, imageData: data }
}

function gifDocument(frames: GifFrame[], width: number, height: number): GifDocument {
  return { width, height, sourceName: 'animation.gif', frames }
}

function pixelAt(data: ImageData, x: number, y: number): Rgba {
  const offset = (y * data.width + x) * 4
  return [data.data[offset], data.data[offset + 1], data.data[offset + 2], data.data[offset + 3]]
}

describe('applyCanvasTransform', () => {
  it('applies the same transform to every frame and preserves frame metadata', () => {
    const first = imageData(2, 1, [{ x: 0, y: 0, rgba: RED }])
    const second = imageData(2, 1, [{ x: 0, y: 0, rgba: BLUE }])
    const document = gifDocument([frame('first', 40, first), frame('second', 90, second)], 2, 1)
    const transform = vi.fn<ImageTransform>(
      (source) => new ImageData(new Uint8ClampedArray(source.data.slice(0, 4)), 1, 1)
    )

    const result = applyCanvasTransform(document, transform)

    expect(transform.mock.calls.map(([source]) => source)).toEqual([first, second])
    expect(result).not.toBe(document)
    expect([result.width, result.height]).toEqual([1, 1])
    expect(result.frames.map(({ id, delay }) => ({ id, delay }))).toEqual([
      { id: 'first', delay: 40 },
      { id: 'second', delay: 90 }
    ])
    expect(pixelAt(result.frames[0].imageData, 0, 0)).toEqual(RED)
    expect(pixelAt(result.frames[1].imageData, 0, 0)).toEqual(BLUE)
    expect([document.width, document.height]).toEqual([2, 1])
  })

  it('checks the input invariant before invoking the transform', () => {
    const document = gifDocument(
      [frame('valid', 40, imageData(2, 1)), frame('invalid', 90, imageData(1, 1))],
      2,
      1
    )
    const transform = vi.fn<ImageTransform>((source) => source)

    expect(() => applyCanvasTransform(document, transform)).toThrowError(GifValidationError)
    expect(transform).not.toHaveBeenCalled()
  })

  it('checks the output invariant and rejects transforms with mixed result sizes', () => {
    const document = gifDocument(
      [frame('first', 40, imageData(2, 1)), frame('second', 90, imageData(2, 1))],
      2,
      1
    )
    let call = 0
    const transform: ImageTransform = (source) => {
      call += 1
      return call === 1
        ? new ImageData(new Uint8ClampedArray(4), 1, 1)
        : new ImageData(new Uint8ClampedArray(source.data), source.width, source.height)
    }

    expect(() => applyCanvasTransform(document, transform)).toThrowError(GifValidationError)
  })
})

describe('cropCanvasToFrameBounds', () => {
  it('uses the selected frame bounds as one shared crop rect for every frame', () => {
    const other = imageData(4, 3, [
      { x: 0, y: 0, rgba: GREEN },
      { x: 1, y: 1, rgba: RED },
      { x: 2, y: 2, rgba: BLUE }
    ])
    const selected = imageData(4, 3, [
      { x: 1, y: 1, rgba: GREEN },
      { x: 2, y: 2, rgba: RED }
    ])
    const document = gifDocument([frame('other', 50, other), frame('selected', 75, selected)], 4, 3)

    const result = cropCanvasToFrameBounds(document, 1, 0)

    expect([result.width, result.height]).toEqual([2, 2])
    expect(result.frames.map((item) => [item.imageData.width, item.imageData.height])).toEqual([
      [2, 2],
      [2, 2]
    ])
    expect(pixelAt(result.frames[0].imageData, 0, 0)).toEqual(RED)
    expect(pixelAt(result.frames[0].imageData, 1, 1)).toEqual(BLUE)
    expect(result.frames.map(({ id, delay }) => ({ id, delay }))).toEqual([
      { id: 'other', delay: 50 },
      { id: 'selected', delay: 75 }
    ])
  })

  it('returns the original document when the selected frame is transparent', () => {
    const document = gifDocument(
      [
        frame('clear', 50, imageData(3, 2)),
        frame('content', 75, imageData(3, 2, [{ x: 1, y: 1, rgba: RED }]))
      ],
      3,
      2
    )

    expect(cropCanvasToFrameBounds(document, 0, 0)).toBe(document)
  })

  it('rejects a frame index outside the document', () => {
    const document = gifDocument([frame('only', 50, imageData(1, 1))], 1, 1)

    expect(() => cropCanvasToFrameBounds(document, 1, 0)).toThrowError(RangeError)
  })
})

describe('cropCanvasToUnionBounds', () => {
  it('crops all frames to the union of non-empty frame bounds', () => {
    const first = imageData(5, 4, [{ x: 1, y: 1, rgba: RED }])
    const second = imageData(5, 4, [{ x: 3, y: 2, rgba: BLUE }])
    const transparent = imageData(5, 4)
    const document = gifDocument(
      [frame('first', 40, first), frame('second', 60, second), frame('clear', 80, transparent)],
      5,
      4
    )

    const result = cropCanvasToUnionBounds(document, 0)

    expect([result.width, result.height]).toEqual([3, 2])
    expect(result.frames.every((item) => item.imageData.width === 3)).toBe(true)
    expect(result.frames.every((item) => item.imageData.height === 2)).toBe(true)
    expect(pixelAt(result.frames[0].imageData, 0, 0)).toEqual(RED)
    expect(pixelAt(result.frames[1].imageData, 2, 1)).toEqual(BLUE)
    expect(pixelAt(result.frames[2].imageData, 1, 1)).toEqual(CLEAR)
    expect(result.frames.map(({ id, delay }) => ({ id, delay }))).toEqual([
      { id: 'first', delay: 40 },
      { id: 'second', delay: 60 },
      { id: 'clear', delay: 80 }
    ])
  })

  it('returns the original document when every frame is transparent', () => {
    const document = gifDocument(
      [frame('first', 40, imageData(3, 2)), frame('second', 60, imageData(3, 2))],
      3,
      2
    )

    expect(cropCanvasToUnionBounds(document, 0)).toBe(document)
  })
})

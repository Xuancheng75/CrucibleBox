import { describe, expect, it, vi } from 'vitest'
import type { ParsedFrame } from 'gifuct-js'
import type { GifDocument, GifFrame } from '../src/types'
import {
  GifValidationError,
  assertGifDocumentCanvasInvariant,
  assertGifFileWithinLimits,
  applyResidueFix,
  calculateRgbaByteLength,
  compositeGifFrames,
  decodeGifFile,
  encodeGif,
  splitConnectedObjects,
  splitGrid,
  transforms,
  validateDecodedGifFrames,
  validateGifDocumentCanvasInvariant,
  validateParsedGifMetadata
} from '../src/utils/gif'

type Rgba = readonly [number, number, number, number]

const RED: Rgba = [255, 0, 0, 255]
const GREEN: Rgba = [0, 255, 0, 255]
const BLUE: Rgba = [0, 0, 255, 255]
const CLEAR: Rgba = [0, 0, 0, 0]

function imageData(width: number, height: number, pixels: readonly Rgba[]): ImageData {
  return new ImageData(new Uint8ClampedArray(pixels.flat()), width, height)
}

function parsedFrame(
  width: number,
  height: number,
  left: number,
  top: number,
  pixels: readonly Rgba[],
  disposalType = 1
): ParsedFrame {
  return {
    dims: { width, height, left, top },
    colorTable: [],
    delay: 100,
    disposalType,
    patch: new Uint8ClampedArray(pixels.flat()),
    pixels: [],
    transparentIndex: 0
  }
}

function pixelAt(data: ImageData, x: number, y: number): Rgba {
  const offset = (y * data.width + x) * 4
  return [data.data[offset], data.data[offset + 1], data.data[offset + 2], data.data[offset + 3]]
}

function redChannelGrid(data: ImageData): number[][] {
  return Array.from({ length: data.height }, (_, y) =>
    Array.from({ length: data.width }, (_, x) => pixelAt(data, x, y)[0])
  )
}

function rawFrame(width: number, height: number, left = 0, top = 0): unknown {
  return { image: { descriptor: { width, height, left, top } } }
}

function gifFrame(id: string, data: ImageData): GifFrame {
  return { id, imageData: data, delay: 100 }
}

describe('GIF pixel transforms', () => {
  const source = imageData(2, 3, [
    [1, 0, 0, 255],
    [2, 0, 0, 255],
    [3, 0, 0, 255],
    [4, 0, 0, 255],
    [5, 0, 0, 255],
    [6, 0, 0, 255]
  ])

  it('rotates a non-square frame 90 degrees with swapped dimensions', () => {
    const result = transforms.rotate90(source)

    expect([result.width, result.height]).toEqual([3, 2])
    expect(redChannelGrid(result)).toEqual([
      [5, 3, 1],
      [6, 4, 2]
    ])
  })

  it('rotates a non-square frame 270 degrees with swapped dimensions', () => {
    const result = transforms.rotate270(source)

    expect([result.width, result.height]).toEqual([3, 2])
    expect(redChannelGrid(result)).toEqual([
      [2, 4, 6],
      [1, 3, 5]
    ])
  })
})

describe('GIF frame compositing', () => {
  it('clears only the local frame rectangle for disposal method 2', () => {
    const frames = [
      parsedFrame(3, 1, 0, 0, [RED, RED, RED]),
      parsedFrame(1, 1, 1, 0, [GREEN], 2),
      parsedFrame(1, 1, 0, 0, [BLUE])
    ]

    const rendered = compositeGifFrames(frames, 3, 1)

    expect([
      pixelAt(rendered[1], 0, 0),
      pixelAt(rendered[1], 1, 0),
      pixelAt(rendered[1], 2, 0)
    ]).toEqual([RED, GREEN, RED])
    expect([
      pixelAt(rendered[2], 0, 0),
      pixelAt(rendered[2], 1, 0),
      pixelAt(rendered[2], 2, 0)
    ]).toEqual([BLUE, CLEAR, RED])
  })

  it('restores the previous canvas for disposal method 3', () => {
    const frames = [
      parsedFrame(3, 1, 0, 0, [RED, RED, RED]),
      parsedFrame(1, 1, 1, 0, [GREEN], 3),
      parsedFrame(1, 1, 0, 0, [BLUE])
    ]

    const rendered = compositeGifFrames(frames, 3, 1)

    expect([
      pixelAt(rendered[2], 0, 0),
      pixelAt(rendered[2], 1, 0),
      pixelAt(rendered[2], 2, 0)
    ]).toEqual([BLUE, RED, RED])
  })

  it('keeps destination pixels underneath a transparent patch', () => {
    const rendered = compositeGifFrames(
      [parsedFrame(1, 1, 0, 0, [RED]), parsedFrame(1, 1, 0, 0, [CLEAR])],
      1,
      1
    )

    expect(pixelAt(rendered[1], 0, 0)).toEqual(RED)
  })
})

describe('GIF residue repair', () => {
  it('reuses the analyzed polluted-frame index without decoding the file twice', async () => {
    const bytes = Buffer.from('R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==', 'base64')
    const arrayBuffer = vi.fn(async () =>
      bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)
    )
    const file = {
      size: bytes.byteLength,
      name: 'one-pixel.gif',
      arrayBuffer
    } as unknown as File

    const frames = await applyResidueFix(file, 0)

    expect(frames).toHaveLength(1)
    expect(arrayBuffer).toHaveBeenCalledOnce()
  })
})

describe('GIF decode resource limits', () => {
  it('decodes a minimal GIF into a full-canvas document within the default budget', async () => {
    const bytes = Buffer.from('R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==', 'base64')
    const document = await decodeGifFile(new File([bytes], 'one-pixel.gif', { type: 'image/gif' }))

    expect([document.width, document.height, document.frames.length]).toEqual([1, 1, 1])
    expect(validateGifDocumentCanvasInvariant(document)).toEqual({ ok: true })
  })

  it('uses BigInt multiplication for RGBA byte lengths', () => {
    const width = Number.MAX_SAFE_INTEGER
    expect(calculateRgbaByteLength(width, 2, 3)).toBe(BigInt(width) * 24n)
  })

  it('rejects an oversized file before reading its bytes', async () => {
    const arrayBuffer = vi.fn<() => Promise<ArrayBuffer>>()
    const file = {
      size: 11,
      name: 'oversized.gif',
      arrayBuffer
    } as unknown as File

    await expect(decodeGifFile(file, { maxFileBytes: 10 })).rejects.toMatchObject({
      code: 'file-size'
    })
    expect(arrayBuffer).not.toHaveBeenCalled()
  })

  it('enforces logical edge and frame-count limits before decompression', () => {
    expect(() => validateParsedGifMetadata(3, 2, [rawFrame(3, 2)], { maxWidth: 2 })).toThrowError(
      expect.objectContaining({ code: 'dimensions' })
    )
    expect(() =>
      validateParsedGifMetadata(2, 2, [rawFrame(2, 2), rawFrame(2, 2)], {
        maxFrames: 1
      })
    ).toThrowError(expect.objectContaining({ code: 'frame-count' }))
  })

  it('rejects a frame descriptor outside the logical canvas', () => {
    expect(() => validateParsedGifMetadata(2, 2, [rawFrame(1, 1, 2, 0)])).toThrowError(
      expect.objectContaining({ code: 'frame-bounds' })
    )
  })

  it('accounts for rendered frames, work canvases, and patches in the RGBA budget', () => {
    const frames = [rawFrame(2, 2)]
    expect(validateParsedGifMetadata(2, 2, frames, { maxTotalRgbaBytes: 64 })).toEqual({
      frameCount: 1,
      estimatedRgbaBytes: 64n
    })
    expect(() => validateParsedGifMetadata(2, 2, frames, { maxTotalRgbaBytes: 63 })).toThrowError(
      expect.objectContaining({ code: 'rgba-budget' })
    )
  })

  it('validates decompressed patch lengths before compositing', () => {
    const frame = parsedFrame(1, 1, 0, 0, [RED])
    frame.patch = new Uint8ClampedArray(3)

    expect(() => validateDecodedGifFrames(1, 1, [frame], 1)).toThrowError(
      expect.objectContaining({ code: 'decoded-frame' })
    )
  })

  it('rejects invalid custom limits instead of allowing a budget bypass', () => {
    expect(() => assertGifFileWithinLimits({ size: 1 }, { maxFrames: Number.NaN })).toThrowError(
      expect.objectContaining({ code: 'invalid-limit' })
    )
  })
})

describe('GIF full-canvas invariants', () => {
  it('accepts a document whose frames all match its canvas', () => {
    const document: GifDocument = {
      width: 2,
      height: 1,
      sourceName: 'valid.gif',
      frames: [gifFrame('a', imageData(2, 1, [RED, BLUE]))]
    }

    expect(validateGifDocumentCanvasInvariant(document)).toEqual({ ok: true })
    expect(() => assertGifDocumentCanvasInvariant(document)).not.toThrow()
  })

  it('reports the first frame that violates the document canvas', () => {
    const document: GifDocument = {
      width: 2,
      height: 1,
      sourceName: 'invalid.gif',
      frames: [gifFrame('a', imageData(2, 1, [RED, BLUE])), gifFrame('b', imageData(1, 1, [GREEN]))]
    }

    expect(validateGifDocumentCanvasInvariant(document)).toMatchObject({
      ok: false,
      frameIndex: 1
    })
    expect(() => assertGifDocumentCanvasInvariant(document)).toThrowError(GifValidationError)
  })

  it('refuses to encode frames with mixed canvas dimensions', async () => {
    const frames = [
      gifFrame('a', imageData(2, 1, [RED, BLUE])),
      gifFrame('b', imageData(1, 1, [GREEN]))
    ]

    await expect(encodeGif(frames, {})).rejects.toMatchObject({ code: 'document-invariant' })
  })

  it('refuses to encode more than 500 frames before creating the encoder', async () => {
    const data = imageData(1, 1, [RED])
    const frames = Array.from({ length: 501 }, (_, index) => gifFrame(String(index), data))

    await expect(encodeGif(frames, {})).rejects.toMatchObject({ code: 'frame-count' })
  })

  it('refuses to encode when the projected work buffers exceed the budget', async () => {
    const frames = [gifFrame('a', imageData(2, 2, [RED, RED, RED, RED]))]

    await expect(encodeGif(frames, {}, undefined, { maxTotalRgbaBytes: 47 })).rejects.toMatchObject(
      { code: 'rgba-budget' }
    )
  })
})

describe('GIF split output budgets', () => {
  it('rejects grid dimensions and projected bytes before allocating output frames', () => {
    const source = imageData(2, 2, [RED, RED, RED, RED])

    expect(() => splitGrid(source, 17, 1)).toThrowError(
      expect.objectContaining({ code: 'output-count' })
    )
    expect(() => splitGrid(source, 2, 2, { maxOutputRgbaBytes: 63 })).toThrowError(
      expect.objectContaining({ code: 'rgba-budget' })
    )
  })

  it('bounds checkerboard connected-object output to the largest 16 layers', () => {
    const width = 11
    const height = 11
    const pixels: Rgba[] = Array.from({ length: width * height }, () => [0, 0, 0, 255])
    for (let y = 1; y < height; y += 2) {
      for (let x = 1; x < width; x += 2) {
        pixels[y * width + x] = [255, 255, 255, 255]
      }
    }
    const layers = splitConnectedObjects(imageData(width, height, pixels), 0, 1)

    expect(layers).toHaveLength(16)
    expect(layers.every((layer) => layer.width === width && layer.height === height)).toBe(true)
  })
})

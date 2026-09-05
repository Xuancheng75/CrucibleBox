import { parseGIF, decompressFrames } from 'gifuct-js'
import type { ParsedFrame } from 'gifuct-js'
import { GIFEncoder, quantize, applyPalette } from 'gifenc'
import type {
  GifDocument,
  GifFrame,
  EncodeOptions,
  CropRect,
  Rgb,
  TransformSet,
} from '../types'
import {
  GifValidationError,
  assertGifDocumentCanvasInvariant,
  assertGifFileWithinLimits,
  assertGifFramesCanvasInvariant,
  assertGifOutputProjection,
  limitGifOutputCount,
  validateDecodedGifFrames,
  validateGifEncodeBudget,
  validateParsedGifMetadata,
} from './gif-validation'
import type { GifDecodeLimits, GifOutputLimits } from './gif-validation'

export {
  DEFAULT_GIF_DECODE_LIMITS,
  DEFAULT_GIF_SPLIT_OUTPUT_LIMITS,
  GifValidationError,
  assertGifDocumentCanvasInvariant,
  assertGifFileWithinLimits,
  assertGifFramesCanvasInvariant,
  assertGifOutputProjection,
  calculateRgbaByteLength,
  limitGifOutputCount,
  resolveGifDecodeLimits,
  resolveGifOutputLimits,
  validateDecodedGifFrames,
  validateGifDocumentCanvasInvariant,
  validateGifEncodeBudget,
  validateGifFramesCanvasInvariant,
  validateParsedGifMetadata,
} from './gif-validation'
export type {
  GifCanvasInvariantResult,
  GifDecodeLimits,
  GifMetadataValidation,
  GifOutputLimits,
  GifOutputProjection,
  GifValidationErrorCode,
} from './gif-validation'

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

const clampByte = (v: number): number =>
  v < 0 ? 0 : v > 255 ? 255 : Math.round(v)

function copyImageData(imageData: ImageData): ImageData {
  return new ImageData(
    new Uint8ClampedArray(imageData.data),
    imageData.width,
    imageData.height
  )
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/**
 * Decode a GIF file into a full-canvas frame document.
 *
 * gifuct-js patches only cover the local image area, so we composite each
 * patch onto a persistent full-size canvas, honoring the GIF disposal method
 * (1 = keep, 2 = clear, 3 = restore to previous).
 */
export async function decodeGifFile(
  file: File,
  limitOverrides: Partial<GifDecodeLimits> = {}
): Promise<GifDocument> {
  const limits = assertGifFileWithinLimits(file, limitOverrides)
  const buffer = await file.arrayBuffer()
  const gif = parseGIF(buffer)
  const width = gif.lsd.width
  const height = gif.lsd.height
  const metadata = validateParsedGifMetadata(width, height, gif.frames, limits)
  const parsedFrames = decompressFrames(gif, true)
  validateDecodedGifFrames(width, height, parsedFrames, metadata.frameCount, limits)
  const rendered = compositeGifFrames(parsedFrames, width, height)

  const frames: GifFrame[] = rendered.map((imageData, i) => ({
    id: crypto.randomUUID(),
    imageData,
    delay: typeof parsedFrames[i].delay === 'number' ? parsedFrames[i].delay : 100,
  }))

  const document = { width, height, sourceName: file.name, frames }
  assertGifDocumentCanvasInvariant(document)
  return document
}

interface RawGifData {
  width: number
  height: number
  parsedFrames: ParsedFrame[]
}

/** Parse a GIF file into raw data without rendering (used by residue analysis). */
async function parseGifFile(file: File): Promise<RawGifData> {
  const limits = assertGifFileWithinLimits(file)
  const buffer = await file.arrayBuffer()
  const gif = parseGIF(buffer)
  const width = gif.lsd.width
  const height = gif.lsd.height
  const metadata = validateParsedGifMetadata(width, height, gif.frames, limits)
  const parsedFrames = decompressFrames(gif, true) as unknown as ParsedFrame[]
  validateDecodedGifFrames(width, height, parsedFrames, metadata.frameCount, limits)
  return {
    width,
    height,
    parsedFrames,
  }
}

/**
 * Composite parsed frames into full-canvas ImageData, honoring GIF disposal
 * methods (1 keep, 2 clear, 3 restore to previous). When `forceClear` is given,
 * the frame at that index is treated as disposal=2 (clear after drawing), which
 * is the core of the residue fix.
 */
export function compositeGifFrames(
  parsedFrames: ParsedFrame[],
  width: number,
  height: number,
  forceClear?: (index: number) => boolean
): ImageData[] {
  const canvas = new Uint8ClampedArray(width * height * 4)
  let previous: Uint8ClampedArray | null = null
  const rendered: ImageData[] = []

  for (let i = 0; i < parsedFrames.length; i++) {
    const f = parsedFrames[i]
    const disposalType = typeof f.disposalType === 'number' ? f.disposalType : 1
    const effectiveClear = forceClear ? forceClear(i) : false
    const effectiveDisposal = effectiveClear ? 2 : disposalType

    // Snapshot the canvas BEFORE drawing when the frame will restore to previous.
    if (effectiveDisposal === 3) {
      previous = new Uint8ClampedArray(canvas)
    }

    // Composite the patch (RGBA, dims.width x dims.height) at (left, top).
    const pw = f.dims.width
    const ph = f.dims.height
    for (let patchY = 0; patchY < ph; patchY++) {
      const canvasY = f.dims.top + patchY
      if (canvasY < 0 || canvasY >= height) continue
      for (let patchX = 0; patchX < pw; patchX++) {
        const canvasX = f.dims.left + patchX
        if (canvasX < 0 || canvasX >= width) continue
        const sourceOffset = (patchY * pw + patchX) * 4
        const sourceAlphaByte = f.patch[sourceOffset + 3]
        if (sourceAlphaByte === 0) continue
        const destinationOffset = (canvasY * width + canvasX) * 4

        if (sourceAlphaByte === 255) {
          canvas[destinationOffset] = f.patch[sourceOffset]
          canvas[destinationOffset + 1] = f.patch[sourceOffset + 1]
          canvas[destinationOffset + 2] = f.patch[sourceOffset + 2]
          canvas[destinationOffset + 3] = 255
          continue
        }

        const sourceAlpha = sourceAlphaByte / 255
        const destinationAlpha = canvas[destinationOffset + 3] / 255
        const outputAlpha = sourceAlpha + destinationAlpha * (1 - sourceAlpha)
        if (outputAlpha === 0) continue
        for (let channel = 0; channel < 3; channel++) {
          const source = f.patch[sourceOffset + channel]
          const destination = canvas[destinationOffset + channel]
          canvas[destinationOffset + channel] = Math.round(
            (source * sourceAlpha + destination * destinationAlpha * (1 - sourceAlpha)) /
              outputAlpha
          )
        }
        canvas[destinationOffset + 3] = Math.round(outputAlpha * 255)
      }
    }

    rendered.push(new ImageData(new Uint8ClampedArray(canvas), width, height))

    // Update canvas state per disposal method.
    if (effectiveDisposal === 2) {
      for (let clearY = 0; clearY < ph; clearY++) {
        const canvasY = f.dims.top + clearY
        if (canvasY < 0 || canvasY >= height) continue
        const startX = Math.max(0, f.dims.left)
        const endX = Math.min(width, f.dims.left + pw)
        if (endX <= startX) continue
        const startOffset = (canvasY * width + startX) * 4
        const endOffset = (canvasY * width + endX) * 4
        canvas.fill(0, startOffset, endOffset)
      }
    } else if (effectiveDisposal === 3 && previous) {
      canvas.set(previous)
    }
  }

  return rendered
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/**
 * Encode full-canvas frames into a GIF Blob using gifenc.
 */
export async function encodeGif(
  frames: GifFrame[],
  options: EncodeOptions,
  onProgress?: (percent: number) => void,
  limitOverrides: Partial<GifDecodeLimits> = {}
): Promise<Blob> {
  const firstFrame = frames[0]
  if (!firstFrame) {
    throw new GifValidationError('document-invariant', '无法编码不含帧的 GIF')
  }
  assertGifFramesCanvasInvariant(
    frames,
    firstFrame.imageData.width,
    firstFrame.imageData.height
  )
  validateGifEncodeBudget(
    frames,
    firstFrame.imageData.width,
    firstFrame.imageData.height,
    limitOverrides
  )
  const repeat = options.repeat ?? 0
  const maxColors = Math.max(2, Math.min(256, Math.round(options.quality ?? 256)))

  const gif = GIFEncoder()

  for (let i = 0; i < frames.length; i++) {
    const frame = frames[i]
    const data = frame.imageData.data as unknown as Uint8Array

    const palette = quantize(data, maxColors, { format: 'rgba4444' })
    const index = applyPalette(data, palette, 'rgba4444')

    const delay = options.delay ?? frame.delay
    const writeOpts: {
      palette: number[][]
      delay: number
      repeat: number
      transparent?: boolean
      transparentIndex?: number
    } = { palette, delay, repeat }

    const transparentIndex = palette.findIndex((p: number[]) => p[3] === 0)
    if (transparentIndex >= 0) {
      writeOpts.transparent = true
      writeOpts.transparentIndex = transparentIndex
    }

    gif.writeFrame(index, frame.imageData.width, frame.imageData.height, writeOpts)

    onProgress?.(((i + 1) / frames.length) * 100)
    // Yield to the main thread so the UI stays responsive during encoding.
    await new Promise<void>((resolve) => setTimeout(resolve, 0))
  }

  gif.finish()
  return new Blob([gif.bytes()], { type: 'image/gif' })
}

// ---------------------------------------------------------------------------
// Frame helpers
// ---------------------------------------------------------------------------

export function createBlankFrame(
  width: number,
  height: number,
  delay = 100
): GifFrame {
  return {
    id: crypto.randomUUID(),
    imageData: new ImageData(new Uint8ClampedArray(width * height * 4), width, height),
    delay,
  }
}

export function cloneFrame(frame: GifFrame): GifFrame {
  return {
    id: crypto.randomUUID(),
    imageData: copyImageData(frame.imageData),
    delay: frame.delay,
  }
}

export function frameToDataURL(frame: GifFrame, maxDimension = 192): string {
  const canvas = document.createElement('canvas')
  const sourceWidth = frame.imageData.width
  const sourceHeight = frame.imageData.height
  const scale = Math.min(1, maxDimension / Math.max(sourceWidth, sourceHeight))
  canvas.width = Math.max(1, Math.round(sourceWidth * scale))
  canvas.height = Math.max(1, Math.round(sourceHeight * scale))
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('Failed to get 2d context for frame preview')
  if (scale === 1) {
    ctx.putImageData(frame.imageData, 0, 0)
  } else {
    const source = document.createElement('canvas')
    source.width = sourceWidth
    source.height = sourceHeight
    const sourceContext = source.getContext('2d')
    if (!sourceContext) throw new Error('Failed to get 2d context for frame preview source')
    sourceContext.putImageData(frame.imageData, 0, 0)
    ctx.drawImage(source, 0, 0, canvas.width, canvas.height)
  }
  return canvas.toDataURL('image/png')
}

// ---------------------------------------------------------------------------
// Transforms (all return NEW ImageData, never mutate the input)
// ---------------------------------------------------------------------------

function rotate90Impl(imageData: ImageData): ImageData {
  const src = imageData.data
  const w = imageData.width
  const h = imageData.height
  const out = new Uint8ClampedArray(src.length)
  const nw = h
  for (let y = 0; y < h; y++) {
    const rowStart = y * w * 4
    for (let x = 0; x < w; x++) {
      const si = rowStart + x * 4
      const di = (x * nw + (h - 1 - y)) * 4
      out[di] = src[si]
      out[di + 1] = src[si + 1]
      out[di + 2] = src[si + 2]
      out[di + 3] = src[si + 3]
    }
  }
  return new ImageData(out, nw, w)
}

function rotate270Impl(imageData: ImageData): ImageData {
  const src = imageData.data
  const w = imageData.width
  const h = imageData.height
  const out = new Uint8ClampedArray(src.length)
  const nw = h
  for (let y = 0; y < h; y++) {
    const rowStart = y * w * 4
    for (let x = 0; x < w; x++) {
      const si = rowStart + x * 4
      const di = ((w - 1 - x) * nw + y) * 4
      out[di] = src[si]
      out[di + 1] = src[si + 1]
      out[di + 2] = src[si + 2]
      out[di + 3] = src[si + 3]
    }
  }
  return new ImageData(out, nw, w)
}

function flipHorizontalImpl(imageData: ImageData): ImageData {
  const src = imageData.data
  const w = imageData.width
  const h = imageData.height
  const out = new Uint8ClampedArray(src.length)
  for (let y = 0; y < h; y++) {
    const rowStart = y * w * 4
    for (let x = 0; x < w; x++) {
      const si = rowStart + x * 4
      const di = rowStart + (w - 1 - x) * 4
      out[di] = src[si]
      out[di + 1] = src[si + 1]
      out[di + 2] = src[si + 2]
      out[di + 3] = src[si + 3]
    }
  }
  return new ImageData(out, w, h)
}

function flipVerticalImpl(imageData: ImageData): ImageData {
  const src = imageData.data
  const w = imageData.width
  const h = imageData.height
  const out = new Uint8ClampedArray(src.length)
  for (let y = 0; y < h; y++) {
    const rowStart = y * w * 4
    const dstRowStart = (h - 1 - y) * w * 4
    for (let x = 0; x < w; x++) {
      const si = rowStart + x * 4
      const di = dstRowStart + x * 4
      out[di] = src[si]
      out[di + 1] = src[si + 1]
      out[di + 2] = src[si + 2]
      out[di + 3] = src[si + 3]
    }
  }
  return new ImageData(out, w, h)
}

function scaleImpl(factor: number): (imageData: ImageData) => ImageData {
  return (imageData) => {
    const w = imageData.width
    const h = imageData.height
    if (!(factor > 0)) {
      return copyImageData(imageData)
    }
    const nw = Math.max(1, Math.round(w * factor))
    const nh = Math.max(1, Math.round(h * factor))

    const srcCanvas = document.createElement('canvas')
    srcCanvas.width = w
    srcCanvas.height = h
    const srcCtx = srcCanvas.getContext('2d')
    if (!srcCtx) throw new Error('Failed to get 2d context for scaling')
    srcCtx.putImageData(imageData, 0, 0)

    const dstCanvas = document.createElement('canvas')
    dstCanvas.width = nw
    dstCanvas.height = nh
    const dstCtx = dstCanvas.getContext('2d')
    if (!dstCtx) throw new Error('Failed to get 2d context for scaling')
    dstCtx.imageSmoothingEnabled = true
    dstCtx.drawImage(srcCanvas, 0, 0, nw, nh)
    return dstCtx.getImageData(0, 0, nw, nh)
  }
}

function cropImpl(rect: CropRect): (imageData: ImageData) => ImageData {
  return (imageData) => {
    const src = imageData.data
    const srcW = imageData.width
    const srcH = imageData.height
    const x = Math.floor(rect.x)
    const y = Math.floor(rect.y)
    const w = Math.floor(rect.width)
    const h = Math.floor(rect.height)

    // Invalid rect: zero/negative size or entirely outside the canvas.
    if (w <= 0 || h <= 0 || x >= srcW || y >= srcH || x + w <= 0 || y + h <= 0) {
      return copyImageData(imageData)
    }

    const out = new Uint8ClampedArray(w * h * 4)
    for (let dy = 0; dy < h; dy++) {
      const srcY = y + dy
      if (srcY < 0 || srcY >= srcH) continue
      const srcRowStart = srcY * srcW * 4
      const dstRowStart = dy * w * 4
      for (let dx = 0; dx < w; dx++) {
        const srcX = x + dx
        if (srcX < 0 || srcX >= srcW) continue
        const si = srcRowStart + srcX * 4
        const di = dstRowStart + dx * 4
        out[di] = src[si]
        out[di + 1] = src[si + 1]
        out[di + 2] = src[si + 2]
        out[di + 3] = src[si + 3]
      }
    }
    return new ImageData(out, w, h)
  }
}

function brightnessImpl(amount: number): (imageData: ImageData) => ImageData {
  return (imageData) => {
    const src = imageData.data
    const out = new Uint8ClampedArray(src.length)
    const delta = amount * 255
    for (let i = 0; i < out.length; i += 4) {
      out[i] = clampByte(src[i] + delta)
      out[i + 1] = clampByte(src[i + 1] + delta)
      out[i + 2] = clampByte(src[i + 2] + delta)
      out[i + 3] = src[i + 3]
    }
    return new ImageData(out, imageData.width, imageData.height)
  }
}

function contrastImpl(amount: number): (imageData: ImageData) => ImageData {
  return (imageData) => {
    const src = imageData.data
    const out = new Uint8ClampedArray(src.length)
    const c = amount * 255
    const factor = (259 * (c + 255)) / (255 * (259 - c))
    for (let i = 0; i < out.length; i += 4) {
      out[i] = clampByte(factor * (src[i] - 128) + 128)
      out[i + 1] = clampByte(factor * (src[i + 1] - 128) + 128)
      out[i + 2] = clampByte(factor * (src[i + 2] - 128) + 128)
      out[i + 3] = src[i + 3]
    }
    return new ImageData(out, imageData.width, imageData.height)
  }
}

function saturationImpl(amount: number): (imageData: ImageData) => ImageData {
  return (imageData) => {
    const src = imageData.data
    const out = new Uint8ClampedArray(src.length)
    const t = 1 + amount
    for (let i = 0; i < out.length; i += 4) {
      const r = src[i]
      const g = src[i + 1]
      const b = src[i + 2]
      const gray = 0.2126 * r + 0.7152 * g + 0.0722 * b
      out[i] = clampByte(gray + (r - gray) * t)
      out[i + 1] = clampByte(gray + (g - gray) * t)
      out[i + 2] = clampByte(gray + (b - gray) * t)
      out[i + 3] = src[i + 3]
    }
    return new ImageData(out, imageData.width, imageData.height)
  }
}

function grayscaleImpl(imageData: ImageData): ImageData {
  const src = imageData.data
  const out = new Uint8ClampedArray(src.length)
  for (let i = 0; i < out.length; i += 4) {
    const gray = clampByte(
      0.2126 * src[i] + 0.7152 * src[i + 1] + 0.0722 * src[i + 2]
    )
    out[i] = gray
    out[i + 1] = gray
    out[i + 2] = gray
    out[i + 3] = src[i + 3]
  }
  return new ImageData(out, imageData.width, imageData.height)
}

function invertImpl(imageData: ImageData): ImageData {
  const src = imageData.data
  const out = new Uint8ClampedArray(src.length)
  for (let i = 0; i < out.length; i += 4) {
    out[i] = 255 - src[i]
    out[i + 1] = 255 - src[i + 1]
    out[i + 2] = 255 - src[i + 2]
    out[i + 3] = src[i + 3]
  }
  return new ImageData(out, imageData.width, imageData.height)
}

function replaceColorImpl(
  target: Rgb,
  replacement: Rgb,
  tolerance: number
): (imageData: ImageData) => ImageData {
  return (imageData) => {
    const src = imageData.data
    const out = new Uint8ClampedArray(src.length)
    const [tr, tg, tb] = target
    const [rr, rg, rb] = replacement
    const tol = Math.max(0, tolerance)
    for (let i = 0; i < out.length; i += 4) {
      const r = src[i]
      const g = src[i + 1]
      const b = src[i + 2]
      if (Math.abs(r - tr) + Math.abs(g - tg) + Math.abs(b - tb) <= tol) {
        out[i] = rr
        out[i + 1] = rg
        out[i + 2] = rb
      } else {
        out[i] = r
        out[i + 1] = g
        out[i + 2] = b
      }
      out[i + 3] = src[i + 3]
    }
    return new ImageData(out, imageData.width, imageData.height)
  }
}

// Populate the transforms contract declared in types.ts.
export const transforms: TransformSet = {
  rotate90: rotate90Impl,
  rotate270: rotate270Impl,
  flipHorizontal: flipHorizontalImpl,
  flipVertical: flipVerticalImpl,
  scale: scaleImpl,
  crop: cropImpl,
  brightness: brightnessImpl,
  contrast: contrastImpl,
  saturation: saturationImpl,
  grayscale: grayscaleImpl,
  invert: invertImpl,
  replaceColor: replaceColorImpl,
}

// ---------------------------------------------------------------------------
// Smart analysis: flood fill, background removal, auto-crop, region delete,
// color-layer split, grid split
// ---------------------------------------------------------------------------

function colorDistance(r1: number, g1: number, b1: number, r2: number, g2: number, b2: number): number {
  return Math.abs(r1 - r2) + Math.abs(g1 - g2) + Math.abs(b1 - b2)
}

/**
 * BFS flood fill over pixels whose RGBA distance to the seed is within
 * tolerance. Returns a mask (Uint8Array, 1 = in region) sized w*h.
 */
export function floodFill(
  imageData: ImageData,
  seedX: number,
  seedY: number,
  tolerance: number
): Uint8Array {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data
  const tol = Math.max(0, tolerance)
  const mask = new Uint8Array(w * h)
  const sx = Math.max(0, Math.min(w - 1, Math.round(seedX)))
  const sy = Math.max(0, Math.min(h - 1, Math.round(seedY)))
  const start = sy * w + sx

  const sr = data[start * 4]
  const sg = data[start * 4 + 1]
  const sb = data[start * 4 + 2]
  const sa = data[start * 4 + 3]

  const queue = new Int32Array(w * h)
  let head = 0
  let tail = 0
  queue[tail++] = start
  mask[start] = 1

  while (head < tail) {
    const idx = queue[head++]
    const x = idx % w
    const y = (idx / w) | 0
    // 4-connected neighbors
    const neighbors =
      x > 0 ? idx - 1 : -1
    const right = x < w - 1 ? idx + 1 : -1
    const up = y > 0 ? idx - w : -1
    const down = y < h - 1 ? idx + w : -1
    for (const ni of [neighbors, right, up, down]) {
      if (ni < 0 || mask[ni]) continue
      const o = ni * 4
      const dr = data[o]
      const dg = data[o + 1]
      const db = data[o + 2]
      const da = data[o + 3]
      const dist = colorDistance(dr, dg, db, sr, sg, sb) + Math.abs(da - sa)
      if (dist <= tol) {
        mask[ni] = 1
        queue[tail++] = ni
      }
    }
  }
  return mask
}

/**
 * Auto-remove background: flood fill from all four corners (union) and set
 * those pixels transparent. Returns a NEW ImageData.
 */
export function removeBackground(imageData: ImageData, tolerance: number): ImageData {
  const w = imageData.width
  const h = imageData.height
  const union = new Uint8Array(w * h)
  const corners: [number, number][] = [
    [0, 0],
    [w - 1, 0],
    [0, h - 1],
    [w - 1, h - 1],
  ]
  for (const [cx, cy] of corners) {
    const o = cy * w * 4 + cx * 4
    if (imageData.data[o + 3] === 0) continue
    const m = floodFill(imageData, cx, cy, tolerance)
    for (let i = 0; i < m.length; i++) {
      if (m[i]) union[i] = 1
    }
  }
  const out = copyImageData(imageData)
  for (let i = 0; i < union.length; i++) {
    if (union[i]) out.data[i * 4 + 3] = 0
  }
  return out
}

/**
 * Compute the content bounding box, ignoring transparent pixels and, when the
 * image is fully opaque, pixels similar to the corner background color.
 * Returns null when nothing meaningful is found.
 */
export function autoCropBounds(imageData: ImageData, tolerance: number): CropRect | null {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data

  let hasAlpha = false
  for (let i = 3; i < data.length; i += 4) {
    if (data[i] < 200) {
      hasAlpha = true
      break
    }
  }

  let bgR = 0
  let bgG = 0
  let bgB = 0
  if (!hasAlpha) {
    const n = Math.min(4, w * h)
    let sumR = 0
    let sumG = 0
    let sumB = 0
    const offsets = [0, (w - 1) * 4, (h - 1) * w * 4, (h - 1) * w * 4 + (w - 1) * 4]
    for (let k = 0; k < n; k++) {
      sumR += data[offsets[k]]
      sumG += data[offsets[k] + 1]
      sumB += data[offsets[k] + 2]
    }
    bgR = sumR / n
    bgG = sumG / n
    bgB = sumB / n
  }

  let minX = w
  let minY = h
  let maxX = -1
  let maxY = -1

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4
      const a = data[o + 3]
      const isContent = hasAlpha ? a >= 200 : colorDistance(data[o], data[o + 1], data[o + 2], bgR, bgG, bgB) > tolerance
      if (isContent) {
        if (x < minX) minX = x
        if (x > maxX) maxX = x
        if (y < minY) minY = y
        if (y > maxY) maxY = y
      }
    }
  }

  if (maxX < minX || maxY < minY) return null
  return { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 }
}

/** Auto-crop to content bounds. Returns the original (copied) when nothing to crop. */
export function autoCropImage(imageData: ImageData, tolerance: number): ImageData {
  const rect = autoCropBounds(imageData, tolerance)
  if (!rect) return copyImageData(imageData)
  return cropImpl(rect)(imageData)
}

/** Set a rectangle region transparent. Returns NEW ImageData. */
export function deleteRectRegion(imageData: ImageData, rect: CropRect): ImageData {
  const out = copyImageData(imageData)
  const w = imageData.width
  const h = imageData.height
  const x0 = Math.max(0, Math.floor(rect.x))
  const y0 = Math.max(0, Math.floor(rect.y))
  const x1 = Math.min(w, Math.ceil(rect.x + rect.width))
  const y1 = Math.min(h, Math.ceil(rect.y + rect.height))
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      out.data[(y * w + x) * 4 + 3] = 0
    }
  }
  return out
}

/** Set a flood-fill mask region transparent. Returns NEW ImageData. */
export function deleteRegionByMask(imageData: ImageData, mask: Uint8Array): ImageData {
  const out = copyImageData(imageData)
  for (let i = 0; i < mask.length; i++) {
    if (mask[i]) out.data[i * 4 + 3] = 0
  }
  return out
}

export interface LassoMaskOptions {
  /** 保留边线宽度(px)，边线附近 edgeWidth 内的像素不删除。默认 2。 */
  edgeWidth?: number
}

interface EdgeSeg {
  x1: number
  y1: number
  x2: number
  y2: number
}

function pointToSegmentDist(px: number, py: number, seg: EdgeSeg): number {
  const dx = seg.x2 - seg.x1
  const dy = seg.y2 - seg.y1
  const len2 = dx * dx + dy * dy
  if (len2 === 0) return Math.hypot(px - seg.x1, py - seg.y1)
  let t = ((px - seg.x1) * dx + (py - seg.y1) * dy) / len2
  t = Math.max(0, Math.min(1, t))
  const cx = seg.x1 + t * dx
  const cy = seg.y1 + t * dy
  return Math.hypot(px - cx, py - cy)
}

/**
 * Build a mask (0/1, sized w*h) covering the interior of a closed polygon
 * defined by `points`. Uses the even-odd ray-casting rule, so self-intersecting
 * lasso shapes are handled naturally. When `edgeWidth > 0`, pixels within that
 * distance of the polygon outline are excluded from the mask, preserving an
 * edge stroke around the deleted region.
 */
export function polygonMask(
  imageData: ImageData,
  points: { x: number; y: number }[],
  options: LassoMaskOptions = {}
): Uint8Array {
  const w = imageData.width
  const h = imageData.height
  const mask = new Uint8Array(w * h)
  const pts = points.filter((p) => Number.isFinite(p.x) && Number.isFinite(p.y))
  if (pts.length < 3) return mask

  const segs: EdgeSeg[] = []
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i]
    const b = pts[(i + 1) % pts.length]
    segs.push({ x1: a.x, y1: a.y, x2: b.x, y2: b.y })
  }

  const edgeWidth = Math.max(0, options.edgeWidth ?? 2)

  // Precompute per-row segment spans on the polygon outline for edge preservation.
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      // Even-odd ray casting: count crossings of a horizontal ray to the right.
      let inside = false
      for (const s of segs) {
        const yMin = Math.min(s.y1, s.y2)
        const yMax = Math.max(s.y1, s.y2)
        if (y <= yMin || y > yMax) continue
        const xInt = s.x1 + ((y - s.y1) * (s.x2 - s.x1)) / (s.y2 - s.y1)
        if (xInt > x) inside = !inside
      }
      if (inside) mask[y * w + x] = 1
    }
  }

  if (edgeWidth > 0) {
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        if (!mask[y * w + x]) continue
        let nearEdge = false
        for (const s of segs) {
          if (pointToSegmentDist(x, y, s) <= edgeWidth) {
            nearEdge = true
            break
          }
        }
        if (nearEdge) mask[y * w + x] = 0
      }
    }
  }

  return mask
}

export interface SplitColorOptions {
  tolerance?: number
  maxLayers?: number
  minArea?: number
  outputLimits?: Partial<GifOutputLimits>
}

/**
 * Split into connected same-color regions (color layers). Each returned
 * ImageData keeps only one region's pixels; everything else transparent.
 * Regions are collected via BFS over pixels within color tolerance, filtered
 * by minArea, then sorted largest-first, capped at maxLayers.
 */
export function splitColorLayers(imageData: ImageData, options: SplitColorOptions = {}): ImageData[] {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data
  const tol = Math.max(0, options.tolerance ?? 32)
  const requestedLayers = Math.max(1, Math.min(32, Math.floor(options.maxLayers ?? 6)))
  const maxLayers = limitGifOutputCount(
    w,
    h,
    requestedLayers,
    options.outputLimits
  ).outputCount
  const minArea = Math.max(1, Math.floor(options.minArea ?? Math.max(16, (w * h) / 500)))

  const visited = new Uint8Array(w * h)
  const regions: { pixels: number[]; area: number }[] = []
  const queue = new Int32Array(w * h)

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const idx = y * w + x
      if (visited[idx]) continue
      const o = idx * 4
      if (data[o + 3] === 0) {
        visited[idx] = 1
        continue
      }
      const sr = data[o]
      const sg = data[o + 1]
      const sb = data[o + 2]

      let head = 0
      let tail = 0
      queue[tail++] = idx
      visited[idx] = 1
      const pixels: number[] = []
      while (head < tail) {
        const ci = queue[head++]
        pixels.push(ci)
        const cx = ci % w
        const cy = (ci / w) | 0
        const nbs =
          cx > 0 ? ci - 1 : -1
        const right = cx < w - 1 ? ci + 1 : -1
        const up = cy > 0 ? ci - w : -1
        const down = cy < h - 1 ? ci + w : -1
        for (const ni of [nbs, right, up, down]) {
          if (ni < 0 || visited[ni]) continue
          const no = ni * 4
          if (data[no + 3] === 0) {
            visited[ni] = 1
            continue
          }
          if (colorDistance(data[no], data[no + 1], data[no + 2], sr, sg, sb) <= tol) {
            visited[ni] = 1
            queue[tail++] = ni
          }
        }
      }
      regions.push({ pixels, area: pixels.length })
    }
  }

  const layers = regions
    .filter((r) => r.area >= minArea)
    .sort((a, b) => b.area - a.area)
    .slice(0, maxLayers)

  return layers.map((region) => {
    const out = new Uint8ClampedArray(w * h * 4)
    for (const pi of region.pixels) {
      const o = pi * 4
      out[o] = data[o]
      out[o + 1] = data[o + 1]
      out[o + 2] = data[o + 2]
      out[o + 3] = data[o + 3]
    }
    return new ImageData(out, w, h)
  })
}

/**
 * Grid-split a sprite sheet into frames. Each returned ImageData is the same
 * canvas size with only one cell's content kept.
 */
export function splitGrid(
  imageData: ImageData,
  rows: number,
  cols: number,
  outputLimits: Partial<GifOutputLimits> = {}
): ImageData[] {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data
  const r = Math.floor(rows)
  const c = Math.floor(cols)
  if (!Number.isSafeInteger(r) || !Number.isSafeInteger(c) || r < 1 || c < 1 || r > 16 || c > 16) {
    throw new GifValidationError('output-count', '网格行列必须是 1 到 16 之间的整数')
  }
  assertGifOutputProjection(w, h, r * c, { maxOutputs: 16 * 16, ...outputLimits })
  const cw = w / c
  const ch = h / r
  const frames: ImageData[] = []
  for (let row = 0; row < r; row++) {
    for (let col = 0; col < c; col++) {
      const out = new Uint8ClampedArray(w * h * 4)
      const x0 = Math.floor(col * cw)
      const y0 = Math.floor(row * ch)
      const x1 = Math.floor((col + 1) * cw)
      const y1 = Math.floor((row + 1) * ch)
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const si = (y * w + x) * 4
          const di = si
          out[di] = data[si]
          out[di + 1] = data[si + 1]
          out[di + 2] = data[si + 2]
          out[di + 3] = data[si + 3]
        }
      }
      frames.push(new ImageData(out, w, h))
    }
  }
  return frames
}

// ---------------------------------------------------------------------------
// Smart layer separation: subject/background, connected objects, color clusters
// ---------------------------------------------------------------------------

/**
 * Separate a frame into a foreground (subject) layer and a background layer.
 * The foreground keeps the pixels outside the corner-connected background;
 * the background layer keeps those background pixels (plus transparency holes
 * where the subject was). Both layers are full-canvas sized.
 */
export function splitSubjectBackground(
  imageData: ImageData,
  tolerance: number
): [ImageData, ImageData] {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data
  const fg = new Uint8ClampedArray(data.length)
  const bg = new Uint8ClampedArray(data.length)

  const mask = floodFillCornersMask(imageData, tolerance)

  for (let i = 0; i < w * h; i++) {
    const o = i * 4
    if (mask[i]) {
      // background pixel -> keep on bg layer, subject hole transparent there
      bg[o] = data[o]
      bg[o + 1] = data[o + 1]
      bg[o + 2] = data[o + 2]
      bg[o + 3] = data[o + 3]
    } else {
      fg[o] = data[o]
      fg[o + 1] = data[o + 1]
      fg[o + 2] = data[o + 2]
      fg[o + 3] = data[o + 3]
    }
  }

  return [new ImageData(fg, w, h), new ImageData(bg, w, h)]
}

/**
 * Split the non-background content into connected objects, one layer each.
 * Uses corner flood-fill to remove background, then labels connected
 * components over the remaining opaque pixels. Tiny regions are discarded.
 */
export function splitConnectedObjects(
  imageData: ImageData,
  tolerance: number,
  minArea = 16,
  outputLimits: Partial<GifOutputLimits> = {}
): ImageData[] {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data

  const fgMask = floodFillCornersMask(imageData, tolerance)
  // Invert: foreground mask = pixels NOT in background
  for (let i = 0; i < fgMask.length; i++) fgMask[i] = fgMask[i] ? 0 : 1

  const visited = new Uint8Array(w * h)
  const queue = new Int32Array(w * h)
  const components: number[][] = []

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const idx = y * w + x
      if (!fgMask[idx] || visited[idx]) continue
      visited[idx] = 1
      let head = 0
      let tail = 0
      queue[tail++] = idx
      const comp: number[] = []
      while (head < tail) {
        const ci = queue[head++]
        comp.push(ci)
        const cx = ci % w
        const cy = (ci / w) | 0
        const nbs = cx > 0 ? ci - 1 : -1
        const right = cx < w - 1 ? ci + 1 : -1
        const up = cy > 0 ? ci - w : -1
        const down = cy < h - 1 ? ci + w : -1
        for (const ni of [nbs, right, up, down]) {
          if (ni < 0 || visited[ni] || !fgMask[ni]) continue
          visited[ni] = 1
          queue[tail++] = ni
        }
      }
      if (comp.length >= minArea) components.push(comp)
    }
  }

  const outputCount = limitGifOutputCount(w, h, components.length, outputLimits).outputCount
  const selectedComponents = components.sort((a, b) => b.length - a.length).slice(0, outputCount)

  return selectedComponents.map((comp) => {
    const out = new Uint8ClampedArray(w * h * 4)
    for (const pi of comp) {
      const o = pi * 4
      out[o] = data[o]
      out[o + 1] = data[o + 1]
      out[o + 2] = data[o + 2]
      out[o + 3] = data[o + 3]
    }
    return new ImageData(out, w, h)
  })
}

/**
 * Cluster pixels by color (simple k-means on RGB) and emit one layer per
 * cluster. Each layer keeps only the pixels assigned to that cluster.
 */
export function splitColorClusters(
  imageData: ImageData,
  k: number,
  options: {
    maxIter?: number
    minArea?: number
    outputLimits?: Partial<GifOutputLimits>
  } = {}
): ImageData[] {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data
  const requestedClusters = Math.max(2, Math.min(16, Math.floor(k)))
  const maxIter = Math.max(1, Math.min(200, Math.floor(options.maxIter ?? 50)))
  const minArea = Math.max(1, Math.floor(options.minArea ?? Math.max(8, (w * h) / 1000)))

  // Collect opaque pixels
  const px: number[] = [] // pixel indices with alpha > 0
  for (let i = 0; i < w * h; i++) {
    if (data[i * 4 + 3] > 0) px.push(i)
  }
  if (px.length === 0) return []
  const kk = limitGifOutputCount(
    w,
    h,
    requestedClusters,
    options.outputLimits
  ).outputCount
  if (kk < 2) {
    throw new GifValidationError('rgba-budget', '颜色聚类至少需要两份完整画布的输出预算')
  }

  // Initialize centers deterministically: evenly sample sorted-by-intensity pixels
  px.sort((a, b) => {
    const av = data[a * 4] + data[a * 4 + 1] + data[a * 4 + 2]
    const bv = data[b * 4] + data[b * 4 + 1] + data[b * 4 + 2]
    return av - bv
  })
  const centers: number[][] = []
  for (let c = 0; c < kk; c++) {
    const idx = px[Math.floor((c * (px.length - 1)) / (kk - 1))]
    centers.push([data[idx * 4], data[idx * 4 + 1], data[idx * 4 + 2]])
  }

  const assign = new Int32Array(w * h).fill(-1)

  for (let iter = 0; iter < maxIter; iter++) {
    // Assign
    let changed = false
    for (const pi of px) {
      const r = data[pi * 4]
      const g = data[pi * 4 + 1]
      const b = data[pi * 4 + 2]
      let best = 0
      let bestD = Infinity
      for (let c = 0; c < kk; c++) {
        const d = (r - centers[c][0]) ** 2 + (g - centers[c][1]) ** 2 + (b - centers[c][2]) ** 2
        if (d < bestD) {
          bestD = d
          best = c
        }
      }
      if (assign[pi] !== best) {
        assign[pi] = best
        changed = true
      }
    }
    if (!changed) break
    // Recompute centers
    const sums = Array.from({ length: kk }, () => ({ r: 0, g: 0, b: 0, n: 0 }))
    for (const pi of px) {
      const c = assign[pi]
      sums[c].r += data[pi * 4]
      sums[c].g += data[pi * 4 + 1]
      sums[c].b += data[pi * 4 + 2]
      sums[c].n++
    }
    for (let c = 0; c < kk; c++) {
      if (sums[c].n > 0) {
        centers[c][0] = sums[c].r / sums[c].n
        centers[c][1] = sums[c].g / sums[c].n
        centers[c][2] = sums[c].b / sums[c].n
      }
    }
  }

  const layers: ImageData[] = []
  for (let c = 0; c < kk; c++) {
    const out = new Uint8ClampedArray(w * h * 4)
    let count = 0
    for (const pi of px) {
      if (assign[pi] === c) {
        const o = pi * 4
        out[o] = data[o]
        out[o + 1] = data[o + 1]
        out[o + 2] = data[o + 2]
        out[o + 3] = data[o + 3]
        count++
      }
    }
    if (count >= minArea) layers.push(new ImageData(out, w, h))
  }
  return layers
}

/** Merge layers bottom-to-top into a single full-canvas ImageData. */
export function mergeLayers(
  layers: ImageData[],
  width: number,
  height: number
): ImageData {
  const out = new Uint8ClampedArray(width * height * 4)
  const buf = new Uint8ClampedArray(width * height * 4)
  for (const layer of layers) {
    if (!layer) continue
    const src = layer.data
    for (let i = 0; i < width * height; i++) {
      const o = i * 4
      const sa = src[o + 3]
      if (sa === 0) continue
      const sr = src[o]
      const sg = src[o + 1]
      const sb = src[o + 2]
      const da = out[o + 3]
      if (da === 0) {
        out[o] = sr
        out[o + 1] = sg
        out[o + 2] = sb
        out[o + 3] = sa
      } else {
        const a = sa / 255
        const inv = 1 - a
        out[o] = Math.round(sr * a + out[o] * inv)
        out[o + 1] = Math.round(sg * a + out[o + 1] * inv)
        out[o + 2] = Math.round(sb * a + out[o + 2] * inv)
        out[o + 3] = Math.round(da + sa * (255 - da) / 255)
      }
    }
  }
  buf.set(out)
  return new ImageData(buf, width, height)
}

/**
 * Split an image into two layers by a binary mask: the first layer keeps the
 * pixels where mask == 1, the second keeps the pixels where mask == 0.
 * Everything else is transparent. Both layers are full-canvas sized.
 */
export function splitByMask(
  imageData: ImageData,
  mask: Uint8Array
): [ImageData, ImageData] {
  const w = imageData.width
  const h = imageData.height
  const data = imageData.data
  const inner = new Uint8ClampedArray(data.length)
  const outer = new Uint8ClampedArray(data.length)
  for (let i = 0; i < w * h; i++) {
    const o = i * 4
    const target = mask[i] ? inner : outer
    target[o] = data[o]
    target[o + 1] = data[o + 1]
    target[o + 2] = data[o + 2]
    target[o + 3] = data[o + 3]
  }
  return [new ImageData(inner, w, h), new ImageData(outer, w, h)]
}

/** Union flood-fill mask of the four corners (background region). */
function floodFillCornersMask(imageData: ImageData, tolerance: number): Uint8Array {
  const w = imageData.width
  const h = imageData.height
  const union = new Uint8Array(w * h)
  const corners: [number, number][] = [
    [0, 0],
    [w - 1, 0],
    [0, h - 1],
    [w - 1, h - 1],
  ]
  for (const [cx, cy] of corners) {
    const o = cy * w * 4 + cx * 4
    if (imageData.data[o + 3] === 0) continue
    const m = floodFill(imageData, cx, cy, tolerance)
    for (let i = 0; i < m.length; i++) {
      if (m[i]) union[i] = 1
    }
  }
  return union
}

// ---------------------------------------------------------------------------
// Residue (frame-stacking) analysis & fix
// ---------------------------------------------------------------------------

export interface ResidueReport {
  /** 污染源帧索引（disposal=1 的完整静止帧，其内容错误地保留下来） */
  pollutedFrame: number
  /** 受影响（叠加残留）的帧数 */
  affectedCount: number
  /** 检测到的残留像素数 */
  residuePixels: number
  /** 污染源后第一个受影响帧的修复前画面（预览用） */
  previewBefore: ImageData
  /** 同上帧的修复后画面（预览用） */
  previewAfter: ImageData
}

interface ResidueCandidate {
  index: number
  residue: number
  affected: number
  maxPct: number
  score: number
}

function disposalOf(f: { disposalType?: number }): number {
  return typeof f.disposalType === 'number' ? f.disposalType : 1
}

function patchCoverage(f: { dims: { width: number; height: number } }, total: number): number {
  return (f.dims.width * f.dims.height) / total
}

/**
 * Detect a "polluted frame" — a near full-canvas frame whose content was baked
 * into a following animation segment (a common GIF authoring bug). The polluted
 * frame may be marked disposal=1 or disposal=3 (restore previous); in both cases
 * its content is carried forward as a residue overlay.
 *
 * Strategy: for every candidate, simulate clearing it (treat as disposal=2) and
 * measure how much of the FOLLOWING frames disappears. Genuine residue frames
 * lose only a small fraction (< 50%) of each following frame — the animation
 * subject stays intact. Normal animation frames (whose successors inherit their
 * canvas) lose 90-100% and are rejected. Score = removedTotal * (1 - maxPct)
 * ranks the start of the polluted segment highest.
 */
export async function analyzeResidue(file: File): Promise<ResidueReport | null> {
  const { width, height, parsedFrames } = await parseGifFile(file)
  const n = parsedFrames.length
  const totalPx = width * height
  const before = compositeGifFrames(parsedFrames, width, height)
  if (n < 5) return null

  const candidates: ResidueCandidate[] = []

  for (let i = 0; i < n; i++) {
    const dt = disposalOf(parsedFrames[i])
    if (dt === 2) continue
    if (patchCoverage(parsedFrames[i], totalPx) < 0.5) continue

    // Simulate clearing the candidate; measure how much of each following frame
    // disappears (the residue) and the max removed fraction.
    const after = compositeGifFrames(parsedFrames, width, height, (idx) => idx === i)
    let removed = 0
    let affected = 0
    let maxPct = 0
    for (let k = i + 1; k < n; k++) {
      const b = before[k].data
      const a = after[k].data
      let diff = 0
      let opaque = 0
      for (let p = 0; p < b.length; p += 4) {
        if (b[p + 3] > 0) opaque++
        if (b[p + 3] > 0 && a[p + 3] === 0) diff++
      }
      if (diff > 1000) affected++
      removed += diff
      if (opaque > 0) maxPct = Math.max(maxPct, diff / opaque)
    }

    // Reject normal animation frames: clearing them destroys 90-100% of the
    // following frames (canvas inheritance), not a small residue overlay.
    if (removed > 2000 && affected >= 3 && maxPct < 0.5) {
      const score = removed * (1 - maxPct)
      candidates.push({ index: i, residue: removed, affected, maxPct, score })
    }
  }

  if (candidates.length === 0) return null
  candidates.sort((a, b) => b.score - a.score)
  const best = candidates[0]
  const previewIndex = Math.min(best.index + 1, n - 1)

  return {
    pollutedFrame: best.index,
    affectedCount: best.affected,
    residuePixels: best.residue,
    previewBefore: before[previewIndex],
    previewAfter: compositeGifFrames(parsedFrames, width, height, (idx) => idx === best.index)[previewIndex],
  }
}

/**
 * Re-render the GIF treating the polluted frame as disposal=2 (cleared after
 * drawing), removing its baked-in residue from all following frames. Preserves
 * each frame's original delay.
 */
export async function applyResidueFix(
  file: File,
  pollutedFrameOverride?: number
): Promise<GifFrame[]> {
  const { width, height, parsedFrames } = await parseGifFile(file)
  let polluted = pollutedFrameOverride
  if (polluted === undefined) {
    const report = await analyzeResidue(file)
    polluted = report ? report.pollutedFrame : -1
  } else if (!Number.isSafeInteger(polluted) || polluted < 0 || polluted >= parsedFrames.length) {
    throw new Error('Invalid polluted GIF frame index')
  }
  const rendered = compositeGifFrames(parsedFrames, width, height, (idx) => idx === polluted)
  return rendered.map((imageData, i) => ({
    id: crypto.randomUUID(),
    imageData,
    delay: typeof parsedFrames[i].delay === 'number' ? parsedFrames[i].delay : 100,
  }))
}

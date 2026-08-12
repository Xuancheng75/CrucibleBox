import type { CropRect, GifDocument, ImageTransform } from './types'
import { autoCropBounds, transforms } from './utils/gif'
import { assertGifDocumentCanvasInvariant } from './utils/gif-validation'

function applyTransformToValidatedDocument(
  document: GifDocument,
  transform: ImageTransform
): GifDocument {
  const frames = document.frames.map((frame) => ({
    ...frame,
    imageData: transform(frame.imageData)
  }))
  const firstFrame = frames[0]
  const next = {
    ...document,
    width: firstFrame.imageData.width,
    height: firstFrame.imageData.height,
    frames
  }

  assertGifDocumentCanvasInvariant(next)
  return next
}

function returnValidatedNoop(document: GifDocument): GifDocument {
  assertGifDocumentCanvasInvariant(document)
  return document
}

function unionRects(current: CropRect | null, next: CropRect): CropRect {
  if (!current) return next

  const left = Math.min(current.x, next.x)
  const top = Math.min(current.y, next.y)
  const right = Math.max(current.x + current.width, next.x + next.width)
  const bottom = Math.max(current.y + current.height, next.y + next.height)
  return { x: left, y: top, width: right - left, height: bottom - top }
}

/** Apply one canvas-sized transform to every frame in a single document update. */
export function applyCanvasTransform(
  document: GifDocument,
  transform: ImageTransform
): GifDocument {
  assertGifDocumentCanvasInvariant(document)
  return applyTransformToValidatedDocument(document, transform)
}

/**
 * Find content bounds from one frame, then crop every frame with that same rect.
 * A frame without content leaves the document unchanged.
 */
export function cropCanvasToFrameBounds(
  document: GifDocument,
  frameIndex: number,
  tolerance: number
): GifDocument {
  assertGifDocumentCanvasInvariant(document)
  if (!Number.isSafeInteger(frameIndex) || frameIndex < 0 || frameIndex >= document.frames.length) {
    throw new RangeError(`Frame index ${frameIndex} is outside the document`)
  }

  const bounds = autoCropBounds(document.frames[frameIndex].imageData, tolerance)
  if (!bounds) return returnValidatedNoop(document)

  return applyTransformToValidatedDocument(document, transforms.crop(bounds))
}

/**
 * Union the content bounds of every non-empty frame, then crop every frame with
 * that same rect. An entirely transparent document is returned unchanged.
 */
export function cropCanvasToUnionBounds(document: GifDocument, tolerance: number): GifDocument {
  assertGifDocumentCanvasInvariant(document)
  let bounds: CropRect | null = null
  for (const frame of document.frames) {
    const frameBounds = autoCropBounds(frame.imageData, tolerance)
    if (frameBounds) bounds = unionRects(bounds, frameBounds)
  }
  if (!bounds) return returnValidatedNoop(document)

  return applyTransformToValidatedDocument(document, transforms.crop(bounds))
}

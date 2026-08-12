export interface FrameState<T> {
  id: string
  imageData: T
  delay: number
}

export interface HistorySource<T> {
  frames: readonly FrameState<T>[]
  width: number
  height: number
}

export interface HistorySnapshotEntry<T> {
  kind: 'snapshot'
  frames: FrameState<T>[]
  width: number
  height: number
  byteLength: number
}

export interface HistoryDeltaRange {
  offset: number
  xor: Uint8Array
}

export interface HistoryFrameDelta {
  id: string
  beforeDelay: number
  afterDelay: number
  ranges: HistoryDeltaRange[]
}

export interface HistoryDeltaEntry {
  kind: 'delta'
  frames: HistoryFrameDelta[]
  width: number
  height: number
  byteLength: number
}

export type HistoryEntry<T> = HistorySnapshotEntry<T> | HistoryDeltaEntry

export interface HistoryLimits {
  maxEntries: number
  maxBytes: number
}

type HistoryBytes = ArrayLike<number> & { readonly byteLength: number }

export function cloneHistoryEntry<T>(
  source: HistorySource<T>,
  cloneImageData: (imageData: T) => T,
  imageByteLength: (imageData: T) => number
): HistorySnapshotEntry<T> {
  let byteLength = 0
  const frames = source.frames.map((frame) => {
    byteLength += imageByteLength(frame.imageData)
    return {
      ...frame,
      id: frame.id,
      imageData: cloneImageData(frame.imageData)
    }
  })

  return { kind: 'snapshot', frames, width: source.width, height: source.height, byteLength }
}

export function createHistoryEntry<T>(
  before: HistorySource<T>,
  after: HistorySource<T>,
  cloneImageData: (imageData: T) => T,
  imageBytes: (imageData: T) => HistoryBytes,
  imageByteLength: (imageData: T) => number
): HistoryEntry<T> | null {
  if (
    before.width !== after.width ||
    before.height !== after.height ||
    before.frames.length !== after.frames.length ||
    before.frames.some((frame, index) => frame.id !== after.frames[index]?.id)
  ) {
    return cloneHistoryEntry(before, cloneImageData, imageByteLength)
  }

  const frames: HistoryFrameDelta[] = []
  let byteLength = 0
  for (let index = 0; index < before.frames.length; index += 1) {
    const beforeFrame = before.frames[index]
    const afterFrame = after.frames[index]
    const beforeBytes = imageBytes(beforeFrame.imageData)
    const afterBytes = imageBytes(afterFrame.imageData)
    if (beforeBytes.byteLength !== afterBytes.byteLength) {
      return cloneHistoryEntry(before, cloneImageData, imageByteLength)
    }

    let first = -1
    let last = -1
    for (let offset = 0; offset < beforeBytes.byteLength; offset += 1) {
      if (beforeBytes[offset] !== afterBytes[offset]) {
        if (first === -1) first = offset
        last = offset
      }
    }
    const ranges: HistoryDeltaRange[] = []
    if (first !== -1) {
      const xor = new Uint8Array(last - first + 1)
      for (let offset = first; offset <= last; offset += 1) {
        xor[offset - first] = beforeBytes[offset] ^ afterBytes[offset]
      }
      ranges.push({ offset: first, xor })
      byteLength += xor.byteLength
    }
    if (ranges.length > 0 || beforeFrame.delay !== afterFrame.delay) {
      frames.push({
        id: beforeFrame.id,
        beforeDelay: beforeFrame.delay,
        afterDelay: afterFrame.delay,
        ranges
      })
      byteLength += 32
    }
  }

  if (frames.length === 0) return null
  return {
    kind: 'delta',
    frames,
    width: before.width,
    height: before.height,
    byteLength
  }
}

export function applyHistoryDelta<T>(
  source: HistorySource<T>,
  entry: HistoryDeltaEntry,
  direction: 'undo' | 'redo',
  imageBytes: (imageData: T) => HistoryBytes,
  createImageData: (source: T, bytes: Uint8Array) => T
): { frames: FrameState<T>[]; width: number; height: number } {
  if (
    source.width !== entry.width ||
    source.height !== entry.height ||
    source.frames.length === 0
  ) {
    throw new Error('History delta does not match the current canvas')
  }
  const deltas = new Map(entry.frames.map((frame) => [frame.id, frame]))
  const frames = source.frames.map((frame) => {
    const delta = deltas.get(frame.id)
    if (!delta) return frame
    const bytes = new Uint8Array(imageBytes(frame.imageData))
    for (const range of delta.ranges) {
      if (range.offset < 0 || range.offset + range.xor.byteLength > bytes.byteLength) {
        throw new Error('History delta exceeds the current frame')
      }
      for (let index = 0; index < range.xor.byteLength; index += 1) {
        bytes[range.offset + index] ^= range.xor[index]
      }
    }
    return {
      ...frame,
      delay: direction === 'undo' ? delta.beforeDelay : delta.afterDelay,
      imageData:
        delta.ranges.length === 0 ? frame.imageData : createImageData(frame.imageData, bytes)
    }
  })
  return { frames, width: source.width, height: source.height }
}

export function appendHistoryEntry<T>(
  stack: readonly HistoryEntry<T>[],
  entry: HistoryEntry<T>,
  limits: HistoryLimits
): HistoryEntry<T>[] {
  const maxEntries = Math.max(1, Math.floor(limits.maxEntries))
  const maxBytes = Math.max(0, limits.maxBytes)
  const next = [...stack, entry].slice(-maxEntries)
  let totalBytes = next.reduce((sum, item) => sum + item.byteLength, 0)

  while (next.length > 1 && totalBytes > maxBytes) {
    totalBytes -= next.shift()!.byteLength
  }

  return next
}

export interface ThumbnailCacheEntry<T> {
  source: T
  url: string
}

export type ThumbnailCache<T> = Map<string, ThumbnailCacheEntry<T>>

export function reconcileThumbnailCache<T>(
  frames: readonly FrameState<T>[],
  previous: ThumbnailCache<T>,
  renderThumbnail: (frame: FrameState<T>) => string
): ThumbnailCache<T> {
  if (frames.length === 0) return previous.size === 0 ? previous : new Map()

  const next: ThumbnailCache<T> = new Map()
  let changed = previous.size !== frames.length

  for (const frame of frames) {
    const cached = previous.get(frame.id)
    if (cached?.source === frame.imageData) {
      next.set(frame.id, cached)
    } else {
      next.set(frame.id, { source: frame.imageData, url: renderThumbnail(frame) })
      changed = true
    }
  }

  return changed || next.size !== previous.size ? next : previous
}

export interface FilterValues {
  brightness: number
  contrast: number
  saturation: number
}

export interface FilterTransformSet<T> {
  brightness: (amount: number) => (source: T) => T
  contrast: (amount: number) => (source: T) => T
  saturation: (amount: number) => (source: T) => T
}

export function hasPendingFilters(values: FilterValues): boolean {
  return values.brightness !== 0 || values.contrast !== 0 || values.saturation !== 0
}

export function applyFilterValues<T>(
  source: T,
  values: FilterValues,
  transforms: FilterTransformSet<T>
): T {
  let result = source
  if (values.brightness !== 0) result = transforms.brightness(values.brightness)(result)
  if (values.contrast !== 0) result = transforms.contrast(values.contrast)(result)
  if (values.saturation !== 0) result = transforms.saturation(values.saturation)(result)
  return result
}

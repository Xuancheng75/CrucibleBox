import type { GifFrame } from './types'
import type { ResidueReport } from './utils/gif'
import { assertGifFileWithinLimits } from './utils/gif-validation'

export type ResidueWorkerOperation = { type: 'analyze' } | { type: 'fix'; pollutedFrame: number }

type ResidueWorkerResult<T extends ResidueWorkerOperation> = T['type'] extends 'analyze'
  ? ResidueReport | null
  : GifFrame[]

interface WorkerLike {
  onmessage: ((event: MessageEvent<unknown>) => void) | null
  onerror: ((event: ErrorEvent) => void) | null
  postMessage(message: unknown, transfer: Transferable[]): void
  terminate(): void
}

export interface ResidueWorkerEnvironment {
  createObjectUrl(source: string): string
  createWorker(url: string): WorkerLike
  revokeObjectUrl(url: string): void
  randomId(): string
}

export interface ResidueWorkerRunOptions {
  signal?: AbortSignal
  environment: ResidueWorkerEnvironment
}

function abortError(): DOMException {
  return new DOMException('GIF residue worker operation aborted', 'AbortError')
}

export function isResidueWorkerAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function nonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) >= 0
}

function normalizeImageData(value: unknown): ImageData {
  if (
    !isRecord(value) ||
    !Number.isSafeInteger(value.width) ||
    !Number.isSafeInteger(value.height) ||
    Number(value.width) <= 0 ||
    Number(value.height) <= 0 ||
    !(value.data instanceof Uint8ClampedArray) ||
    !(value.data.buffer instanceof ArrayBuffer) ||
    value.data.byteLength !== Number(value.width) * Number(value.height) * 4
  ) {
    throw new Error('residue worker returned invalid image data')
  }
  return new ImageData(
    value.data as Uint8ClampedArray<ArrayBuffer>,
    Number(value.width),
    Number(value.height)
  )
}

function normalizeReport(value: unknown): ResidueReport | null {
  if (value === null) return null
  if (
    !isRecord(value) ||
    !nonNegativeInteger(value.pollutedFrame) ||
    !nonNegativeInteger(value.affectedCount) ||
    !nonNegativeInteger(value.residuePixels)
  ) {
    throw new Error('residue worker returned an invalid analysis report')
  }
  return {
    pollutedFrame: value.pollutedFrame,
    affectedCount: value.affectedCount,
    residuePixels: value.residuePixels,
    previewBefore: normalizeImageData(value.previewBefore),
    previewAfter: normalizeImageData(value.previewAfter)
  }
}

function normalizeFrames(value: unknown): GifFrame[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > 500) {
    throw new Error('residue worker returned an invalid frame collection')
  }
  return value.map((frame) => {
    if (
      !isRecord(frame) ||
      typeof frame.id !== 'string' ||
      frame.id.length === 0 ||
      frame.id.length > 128 ||
      typeof frame.delay !== 'number' ||
      !Number.isFinite(frame.delay) ||
      frame.delay < 0 ||
      frame.delay > 600_000
    ) {
      throw new Error('residue worker returned invalid frame metadata')
    }
    return {
      id: frame.id,
      delay: frame.delay,
      imageData: normalizeImageData(frame.imageData)
    }
  })
}

function normalizeResponse<T extends ResidueWorkerOperation>(
  value: unknown,
  requestId: string,
  operation: T
): ResidueWorkerResult<T> {
  if (!isRecord(value) || value.v !== 1 || value.requestId !== requestId) {
    throw new Error('residue worker returned an invalid response envelope')
  }
  if (value.ok !== true) {
    const message = typeof value.error === 'string' ? value.error : 'residue worker failed'
    throw new Error(message)
  }
  return (
    operation.type === 'analyze' ? normalizeReport(value.result) : normalizeFrames(value.result)
  ) as ResidueWorkerResult<T>
}

function validateOperation(operation: ResidueWorkerOperation): void {
  if (
    operation.type !== 'analyze' &&
    (operation.type !== 'fix' || !nonNegativeInteger(operation.pollutedFrame))
  ) {
    throw new Error('invalid residue worker operation')
  }
}

export async function runResidueWorkerWithSource<T extends ResidueWorkerOperation>(
  workerSource: string,
  file: File,
  operation: T,
  options: ResidueWorkerRunOptions
): Promise<ResidueWorkerResult<T>> {
  validateOperation(operation)
  if (options.signal?.aborted) throw abortError()
  assertGifFileWithinLimits(file)
  const buffer = await file.arrayBuffer()
  if (options.signal?.aborted) throw abortError()

  const requestId = options.environment.randomId()
  const objectUrl = options.environment.createObjectUrl(workerSource)
  let worker: WorkerLike
  try {
    worker = options.environment.createWorker(objectUrl)
  } catch (error) {
    options.environment.revokeObjectUrl(objectUrl)
    throw error
  }

  return new Promise((resolve, reject) => {
    let settled = false
    const cleanup = () => {
      options.signal?.removeEventListener('abort', onAbort)
      worker.onmessage = null
      worker.onerror = null
      worker.terminate()
      options.environment.revokeObjectUrl(objectUrl)
    }
    const finish = (callback: () => void) => {
      if (settled) return
      settled = true
      cleanup()
      callback()
    }
    const onAbort = () => finish(() => reject(abortError()))
    options.signal?.addEventListener('abort', onAbort, { once: true })
    worker.onerror = (event) => {
      finish(() => reject(new Error(event.message || 'residue worker crashed')))
    }
    worker.onmessage = (event) => {
      try {
        const result = normalizeResponse(event.data, requestId, operation)
        finish(() => resolve(result))
      } catch (error) {
        finish(() => reject(error))
      }
    }

    try {
      worker.postMessage(
        {
          v: 1,
          requestId,
          operation,
          file: {
            buffer,
            name: file.name,
            type: file.type,
            lastModified: file.lastModified
          }
        },
        [buffer]
      )
    } catch (error) {
      finish(() => reject(error))
    }
  })
}

import { analyzeResidue, applyResidueFix } from '../utils/gif'

interface WorkerScope {
  onmessage: ((event: MessageEvent<unknown>) => void) | null
  postMessage(message: unknown, transfer: Transferable[]): void
}

interface ImageDataWire {
  width: number
  height: number
  data: Uint8ClampedArray
}

const scope = globalThis as unknown as WorkerScope

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function serializeImageData(imageData: ImageData): ImageDataWire {
  return { width: imageData.width, height: imageData.height, data: imageData.data }
}

scope.onmessage = async (event) => {
  const request = event.data
  if (!isRecord(request) || request.v !== 1 || typeof request.requestId !== 'string') return
  const requestId = request.requestId
  try {
    if (!isRecord(request.file) || !(request.file.buffer instanceof ArrayBuffer)) {
      throw new Error('invalid residue worker file payload')
    }
    const file = new File([request.file.buffer], String(request.file.name ?? 'input.gif'), {
      type: String(request.file.type ?? 'image/gif'),
      lastModified: Number(request.file.lastModified) || 0
    })
    if (!isRecord(request.operation) || typeof request.operation.type !== 'string') {
      throw new Error('invalid residue worker operation')
    }

    if (request.operation.type === 'analyze') {
      const report = await analyzeResidue(file)
      if (!report) {
        scope.postMessage({ v: 1, requestId, ok: true, result: null }, [])
        return
      }
      const result = {
        ...report,
        previewBefore: serializeImageData(report.previewBefore),
        previewAfter: serializeImageData(report.previewAfter)
      }
      scope.postMessage({ v: 1, requestId, ok: true, result }, [
        result.previewBefore.data.buffer,
        result.previewAfter.data.buffer
      ])
      return
    }

    if (
      request.operation.type !== 'fix' ||
      !Number.isSafeInteger(request.operation.pollutedFrame) ||
      Number(request.operation.pollutedFrame) < 0
    ) {
      throw new Error('invalid residue worker fix operation')
    }
    const frames = await applyResidueFix(file, Number(request.operation.pollutedFrame))
    const result = frames.map((frame) => ({
      ...frame,
      imageData: serializeImageData(frame.imageData)
    }))
    scope.postMessage(
      { v: 1, requestId, ok: true, result },
      result.map((frame) => frame.imageData.data.buffer)
    )
  } catch (error) {
    scope.postMessage(
      {
        v: 1,
        requestId,
        ok: false,
        error: error instanceof Error ? error.message : String(error)
      },
      []
    )
  }
}

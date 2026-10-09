import {
  isResidueWorkerAbortError,
  runResidueWorkerWithSource,
  type ResidueWorkerOperation
} from './residue-worker-client'
import { assertGifFileWithinLimits } from './utils/gif-validation'

const MAX_WORKER_SOURCE_BYTES = 2 * 1024 * 1024

function abortError(): DOMException {
  return new DOMException('GIF residue worker operation aborted', 'AbortError')
}

async function readWorkerSource(response: Response, signal?: AbortSignal): Promise<string> {
  const contentLength = response.headers.get('content-length')
  if (contentLength !== null) {
    const declaredBytes = Number(contentLength)
    if (Number.isFinite(declaredBytes) && declaredBytes > MAX_WORKER_SOURCE_BYTES) {
      await response.body?.cancel().catch(() => undefined)
      throw new Error('GIF residue worker source exceeds 2 MiB')
    }
  }

  const body = response.body
  if (!body) throw new Error('GIF residue worker source response has no body')

  const reader = body.getReader()
  const chunks: Uint8Array[] = []
  let byteLength = 0
  try {
    while (true) {
      if (signal?.aborted) {
        await reader.cancel().catch(() => undefined)
        throw abortError()
      }
      const { done, value } = await reader.read()
      if (done) break
      byteLength += value.byteLength
      if (byteLength > MAX_WORKER_SOURCE_BYTES) {
        await reader.cancel().catch(() => undefined)
        throw new Error('GIF residue worker source exceeds 2 MiB')
      }
      chunks.push(value)
    }
  } catch (error) {
    if (signal?.aborted) throw abortError()
    throw error
  }

  if (signal?.aborted) throw abortError()
  const bytes = new Uint8Array(byteLength)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.byteLength
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes)
}

export async function fetchResidueWorkerSource(signal?: AbortSignal): Promise<string> {
  if (signal?.aborted) throw abortError()

  let response: Response
  try {
    response = await fetch(new URL('./dist/workers/residue.js', import.meta.url), {
      credentials: 'same-origin',
      redirect: 'error',
      signal
    })
  } catch (error) {
    if (signal?.aborted) throw abortError()
    throw error
  }

  if (!response.ok) {
    throw new Error(`GIF residue worker request failed with HTTP ${response.status}`)
  }
  if (signal?.aborted) throw abortError()
  return readWorkerSource(response, signal)
}

function randomCorrelationId(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()

  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  bytes[8] = (bytes[8] & 0x3f) | 0x80
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

const browserEnvironment = {
  createObjectUrl: (source: string) =>
    URL.createObjectURL(new Blob([source], { type: 'application/javascript' })),
  createWorker: (url: string) => new Worker(url, { name: 'gif-residue-analysis' }),
  revokeObjectUrl: (url: string) => URL.revokeObjectURL(url),
  randomId: randomCorrelationId
}

export async function runResidueWorker<T extends ResidueWorkerOperation>(
  file: File,
  operation: T,
  signal?: AbortSignal
) {
  if (signal?.aborted) throw abortError()
  assertGifFileWithinLimits(file)
  const workerSource = await fetchResidueWorkerSource(signal)
  if (signal?.aborted) throw abortError()
  return runResidueWorkerWithSource(workerSource, file, operation, {
    environment: browserEnvironment,
    signal
  })
}

export { isResidueWorkerAbortError }

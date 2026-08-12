import { describe, expect, it, vi } from 'vitest'
import {
  isResidueWorkerAbortError,
  runResidueWorkerWithSource,
  type ResidueWorkerEnvironment
} from '../src/residue-worker-client'

type Reply = (request: Record<string, unknown>) => unknown

class FakeWorker {
  onmessage: ((event: MessageEvent<unknown>) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  readonly terminate = vi.fn()
  readonly postMessage = vi.fn((message: unknown) => {
    if (!this.reply) return
    queueMicrotask(() => {
      const request = message as Record<string, unknown>
      this.onmessage?.({ data: this.reply?.(request) } as MessageEvent<unknown>)
    })
  })

  constructor(private readonly reply?: Reply) {}
}

function imageDataWire(red = 255) {
  return {
    width: 1,
    height: 1,
    data: new Uint8ClampedArray([red, 0, 0, 255])
  }
}

function environment(worker: FakeWorker) {
  const revokeObjectUrl = vi.fn()
  const result: ResidueWorkerEnvironment = {
    createObjectUrl: vi.fn(() => 'blob:residue-worker'),
    createWorker: vi.fn(() => worker),
    revokeObjectUrl,
    randomId: () => 'request-1'
  }
  return { result, revokeObjectUrl }
}

const file = () => new File([new Uint8Array([1, 2, 3])], 'input.gif', { type: 'image/gif' })

describe('GIF residue worker client', () => {
  it('validates and rehydrates an analysis report', async () => {
    const worker = new FakeWorker((request) => ({
      v: 1,
      requestId: request.requestId,
      ok: true,
      result: {
        pollutedFrame: 2,
        affectedCount: 4,
        residuePixels: 1200,
        previewBefore: imageDataWire(),
        previewAfter: imageDataWire(0)
      }
    }))
    const env = environment(worker)

    const report = await runResidueWorkerWithSource(
      'worker code',
      file(),
      { type: 'analyze' },
      {
        environment: env.result
      }
    )

    expect(report).toMatchObject({ pollutedFrame: 2, affectedCount: 4, residuePixels: 1200 })
    expect(report?.previewBefore).toBeInstanceOf(ImageData)
    expect(report?.previewAfter.data[0]).toBe(0)
    expect(worker.terminate).toHaveBeenCalledOnce()
    expect(env.revokeObjectUrl).toHaveBeenCalledWith('blob:residue-worker')
  })

  it('validates and rehydrates fixed frames', async () => {
    const worker = new FakeWorker((request) => ({
      v: 1,
      requestId: request.requestId,
      ok: true,
      result: [{ id: 'frame-1', delay: 120, imageData: imageDataWire() }]
    }))
    const env = environment(worker)

    const frames = await runResidueWorkerWithSource(
      'worker code',
      file(),
      { type: 'fix', pollutedFrame: 1 },
      { environment: env.result }
    )

    expect(frames).toHaveLength(1)
    expect(frames[0]).toMatchObject({ id: 'frame-1', delay: 120 })
    expect(frames[0].imageData).toBeInstanceOf(ImageData)
  })

  it('terminates and revokes the worker when cancelled', async () => {
    const worker = new FakeWorker()
    const env = environment(worker)
    const controller = new AbortController()
    const pending = runResidueWorkerWithSource(
      'worker code',
      file(),
      { type: 'analyze' },
      {
        environment: env.result,
        signal: controller.signal
      }
    )
    await vi.waitFor(() => expect(worker.postMessage).toHaveBeenCalledOnce())

    controller.abort()

    await expect(pending).rejects.toSatisfy(isResidueWorkerAbortError)
    expect(worker.terminate).toHaveBeenCalledOnce()
    expect(env.revokeObjectUrl).toHaveBeenCalledOnce()
  })

  it('rejects oversized files before reading or creating a worker', async () => {
    const worker = new FakeWorker()
    const env = environment(worker)
    const arrayBuffer = vi.fn<() => Promise<ArrayBuffer>>()
    const oversized = {
      size: 64 * 1024 * 1024 + 1,
      name: 'oversized.gif',
      arrayBuffer
    } as unknown as File

    await expect(
      runResidueWorkerWithSource(
        'worker code',
        oversized,
        { type: 'analyze' },
        {
          environment: env.result
        }
      )
    ).rejects.toMatchObject({ code: 'file-size' })
    expect(arrayBuffer).not.toHaveBeenCalled()
    expect(env.result.createWorker).not.toHaveBeenCalled()
  })

  it('rejects malformed success payloads and cleans up', async () => {
    const worker = new FakeWorker((request) => ({
      v: 1,
      requestId: request.requestId,
      ok: true,
      result: { pollutedFrame: -1 }
    }))
    const env = environment(worker)

    await expect(
      runResidueWorkerWithSource(
        'worker code',
        file(),
        { type: 'analyze' },
        {
          environment: env.result
        }
      )
    ).rejects.toThrow('invalid analysis report')
    expect(worker.terminate).toHaveBeenCalledOnce()
  })

  it('rejects worker crashes and cleans up', async () => {
    const worker = new FakeWorker()
    worker.postMessage.mockImplementationOnce(() => {
      queueMicrotask(() => worker.onerror?.({ message: 'worker crashed' } as ErrorEvent))
    })
    const env = environment(worker)

    await expect(
      runResidueWorkerWithSource(
        'worker code',
        file(),
        { type: 'analyze' },
        {
          environment: env.result
        }
      )
    ).rejects.toThrow('worker crashed')
    expect(worker.terminate).toHaveBeenCalledOnce()
  })
})

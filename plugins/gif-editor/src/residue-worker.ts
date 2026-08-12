import workerSource from 'openbox-residue-worker-source'
import {
  isResidueWorkerAbortError,
  runResidueWorkerWithSource,
  type ResidueWorkerOperation
} from './residue-worker-client'

const browserEnvironment = {
  createObjectUrl: (source: string) =>
    URL.createObjectURL(new Blob([source], { type: 'application/javascript' })),
  createWorker: (url: string) => new Worker(url, { name: 'gif-residue-analysis' }),
  revokeObjectUrl: (url: string) => URL.revokeObjectURL(url),
  randomId: () => crypto.randomUUID()
}

export function runResidueWorker<T extends ResidueWorkerOperation>(
  file: File,
  operation: T,
  signal?: AbortSignal
) {
  return runResidueWorkerWithSource(workerSource, file, operation, {
    environment: browserEnvironment,
    signal
  })
}

export { isResidueWorkerAbortError }

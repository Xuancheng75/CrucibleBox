export class UpdateCheckTimeoutError extends Error {
  readonly timeoutMs: number

  constructor(timeoutMs: number) {
    super(`更新检查超过 ${timeoutMs}ms 未响应`)
    this.name = 'UpdateCheckTimeoutError'
    this.timeoutMs = timeoutMs
  }
}

export interface UpdateRetryOptions {
  timeoutMs: number
  maxAttempts?: number
  retryDelayMs?: number
}

export function updateErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}

/** 网络抖动和请求超时可以重试，签名/清单解析错误应立即返回。 */
export function isTransientUpdateError(error: unknown): boolean {
  if (error instanceof UpdateCheckTimeoutError) return true
  const message = updateErrorMessage(error).toLowerCase()
  return [
    'error sending request',
    'network',
    'timed out',
    'timeout',
    'connection reset',
    'connection refused',
    'failed to connect',
    'dns',
    'fetch failed',
    'error decoding response body',
    'decoding response body',
    'body decode'
  ].some((marker) => message.includes(marker))
}

export async function withUpdateTimeout<T>(
  operation: () => Promise<T>,
  timeoutMs: number
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new UpdateCheckTimeoutError(timeoutMs)), timeoutMs)
  })
  try {
    return await Promise.race([operation(), timeout])
  } finally {
    if (timer) clearTimeout(timer)
  }
}

export async function retryUpdateCheck<T>(
  operation: () => Promise<T>,
  { timeoutMs, maxAttempts = 2, retryDelayMs = 750 }: UpdateRetryOptions
): Promise<T> {
  const attempts = Math.max(1, Math.floor(maxAttempts))
  let lastError: unknown

  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await withUpdateTimeout(operation, timeoutMs)
    } catch (error) {
      lastError = error
      if (attempt >= attempts || !isTransientUpdateError(error)) throw error
      await new Promise<void>((resolve) => setTimeout(resolve, retryDelayMs * attempt))
    }
  }

  throw lastError ?? new Error('更新检查失败')
}

/**
 * The updater stream can be interrupted by a proxy or a transient GitHub
 * edge error.  The native updater does not expose byte-range resume, so a
 * retry restarts the bounded download and lets the signed archive be checked
 * again from the beginning.
 */
export async function retryUpdateDownload<T>(
  operation: (attempt: number) => Promise<T>,
  { timeoutMs, maxAttempts = 3, retryDelayMs = 1_200 }: UpdateRetryOptions
): Promise<T> {
  const attempts = Math.max(1, Math.floor(maxAttempts))
  let lastError: unknown
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await withUpdateTimeout(() => operation(attempt), timeoutMs)
    } catch (error) {
      lastError = error
      if (attempt >= attempts || !isTransientUpdateError(error)) throw error
      await new Promise<void>((resolve) => setTimeout(resolve, retryDelayMs * attempt))
    }
  }
  throw lastError ?? new Error('更新下载失败')
}

export function formatUpdateError(error: unknown): string {
  if (error instanceof UpdateCheckTimeoutError || isTransientUpdateError(error)) {
    return '更新服务器暂时不可达，请检查网络后重试。'
  }
  return updateErrorMessage(error) || '更新检查失败'
}

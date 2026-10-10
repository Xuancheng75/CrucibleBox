function pause(signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const cancel = () => {
      clearTimeout(timer)
      reject(new DOMException('Session request stopped', 'AbortError'))
    }
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', cancel)
      resolve()
    }, 100)
    signal.addEventListener('abort', cancel, { once: true })
    if (signal.aborted) cancel()
  })
}

/** Retry only transient database contention; permission/protocol denials stay unchanged. */
export async function requestNextSession<T>(
  request: () => Promise<T>,
  signal: AbortSignal
): Promise<T> {
  for (let attempt = 0; ; attempt += 1) {
    if (signal.aborted) throw new DOMException('Session request stopped', 'AbortError')
    try {
      // Return late issued sessions so the caller can dispose their tokens.
      return await request()
    } catch (reason) {
      if (String(reason) !== 'BUSY' || attempt >= 20) throw reason
      await pause(signal)
    }
  }
}

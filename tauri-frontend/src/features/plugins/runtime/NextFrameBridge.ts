import {
  contract,
  validateRequest,
  validateResponse,
  type Response
} from '../../../../../packages/cruciblebox-next2-api/src/index.mjs'

export interface NextSession {
  permissions?: readonly string[]
  token: string
  handshakeToken: string
  indexUrl: string
  origin: 'null'
  rendererApiVersion: 6
  expiresAt: number
}
const budget = contract.budget as {
  inflight: number
  requestTimeoutMs: number
  interactiveTimeoutMs: number
  handshakeTimeoutMs: number
}
export const transport = budget
export function isNextReady(
  event: { source: unknown; origin: string; data: unknown },
  source: unknown,
  session: NextSession
): boolean {
  const data = event.data as { kind?: unknown; nonce?: unknown; wireVersion?: unknown } | null
  return (
    source != null &&
    event.source === source &&
    event.origin === session.origin &&
    data?.kind === 'next-ready' &&
    data.nonce === session.handshakeToken &&
    data.wireVersion === 4 &&
    Date.now() < session.expiresAt
  )
}
export function createNextDispatcher(
  session: NextSession,
  invoke: (raw: string) => Promise<unknown>,
  send: (response: Response) => void
) {
  const wireVersion = 4
  const checkRequest = validateRequest
  const checkResponse = validateResponse
  let disposed = false
  let execution: Promise<void> = Promise.resolve()
  let queued = 0
  const pending = new Map<string, ReturnType<typeof setTimeout>>()
  function failure(
    id: string,
    code:
      | 'SESSION_DENIED'
      | 'BUSY'
      | 'REPLAY_DENIED'
      | 'TIMEOUT'
      | 'INVALID_RESPONSE'
      | 'INTERNAL_ERROR'
  ): Response {
    return { wireVersion, requestId: id, ok: false, error: { code, message: code } }
  }
  function finish(id: string, response: Response) {
    const timer = pending.get(id)
    if (timer === undefined) return
    clearTimeout(timer)
    pending.delete(id)
    if (!disposed) send(response)
  }
  function dispatch(value: unknown): void {
    if (disposed) return
    let request
    try {
      request = checkRequest(JSON.stringify(value))
    } catch {
      return
    }
    const id = request.requestId
    if (request.session !== session.token || Date.now() >= session.expiresAt) {
      send(failure(id, 'SESSION_DENIED'))
      return
    }
    if (pending.has(id)) {
      return // One response per pending request; native ledger rejects later replay.
    }
    if (pending.size >= budget.inflight || queued >= budget.inflight) {
      send(failure(id, 'BUSY'))
      return
    }
    const timeoutMs =
      request.target.kind === 'platform' && request.method === 'files.choose'
        ? budget.interactiveTimeoutMs
        : budget.requestTimeoutMs
    pending.set(
      id,
      setTimeout(() => finish(id, failure(id, 'TIMEOUT')), timeoutMs)
    )
    queued += 1
    execution = execution
      .then(() => {
        queued -= 1
        if (disposed || !pending.has(id)) return
        return invoke(JSON.stringify(request))
      })
      .then((response) => {
        if (disposed || !pending.has(id)) return
        try {
          finish(id, checkResponse(JSON.stringify(response), id))
        } catch {
          finish(id, failure(id, 'INVALID_RESPONSE'))
        }
      })
      .catch(() => finish(id, failure(id, 'INTERNAL_ERROR')))
  }
  return {
    dispatch,
    dispose() {
      disposed = true
      for (const timer of pending.values()) clearTimeout(timer)
      pending.clear()
    }
  }
}

export function nextSandbox(session: NextSession): string {
  return session.permissions?.includes('browser:downloads')
    ? 'allow-scripts allow-downloads'
    : 'allow-scripts'
}

export function scheduleNextLease(session: NextSession, dispose: () => void): () => void {
  const timer = setTimeout(dispose, Math.max(0, session.expiresAt - Date.now()))
  return () => clearTimeout(timer)
}

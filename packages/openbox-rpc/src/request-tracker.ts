/**
 * RequestTracker — RPC pending 请求上限追踪。
 *
 * 合并了 backend/renderer/frame-entry 三处独立的 pending 上限逻辑：
 * - 校验 requestId 格式（调用方注入校验器）
 * - 拒绝重复 requestId
 * - 拒绝超过 limit（默认 64）
 *
 * 报错经 errorFactory 注入，保持各协议的错误码/文案（renderer 结构化
 * issue vs backend 裸字符串）各自不变。
 */

export interface PendingRequestTrackerErrorFactory {
  /** requestId 已存在 */
  duplicateRequestId(): Error
  /** 超过 limit */
  limitReached(): Error
}

export class PendingRequestTracker {
  private readonly requestIds = new Set<string>()

  constructor(
    readonly limit = 64,
    private readonly errors: PendingRequestTrackerErrorFactory,
    private readonly validateRequestId?: (value: string) => void
  ) {
    if (!Number.isSafeInteger(limit) || limit <= 0) {
      throw new RangeError(`Pending request limit must be a positive integer, received ${limit}`)
    }
  }

  get size(): number {
    return this.requestIds.size
  }

  add(requestId: string): void {
    this.validateRequestId?.(requestId)
    if (this.requestIds.has(requestId)) throw this.errors.duplicateRequestId()
    if (this.requestIds.size >= this.limit) throw this.errors.limitReached()
    this.requestIds.add(requestId)
  }

  delete(requestId: string): boolean {
    return this.requestIds.delete(requestId)
  }

  has(requestId: string): boolean {
    return this.requestIds.has(requestId)
  }

  clear(): void {
    this.requestIds.clear()
  }
}

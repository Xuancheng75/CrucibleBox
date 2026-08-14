/**
 * 公共 RPC 校验原语。
 *
 * - isPlainObject：两协议逐字相同的实现收敛到一处
 * - validateRpcToken / validateRpcRequestId：token/requestId 格式校验，
 *   通过 fail 回调注入报错；token 长度区间参数化
 *   （renderer 16-128、backend 32-128），requestId 统一 1-64。
 */

export function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

export interface RpcTokenValidationOptions {
  minLength: number
  maxLength: number
}

/** 校验 session token；返回原始值（若合法）。 */
export function validateRpcToken(
  value: unknown,
  { minLength, maxLength }: RpcTokenValidationOptions,
  fail: (message: string) => never
): string {
  if (typeof value !== 'string' || value.length < minLength || value.length > maxLength) {
    fail(`expected a string with ${minLength}-${maxLength} characters`)
  }
  if (!/^[A-Za-z0-9_-]+$/.test(value)) {
    fail('expected a token containing only letters, digits, "_" or "-"')
  }
  return value
}

/** 校验请求 ID；返回原始值（若合法）。 */
export function validateRpcRequestId(value: unknown, fail: (message: string) => never): string {
  if (typeof value !== 'string' || value.length < 1 || value.length > 64) {
    fail('expected a string with 1-64 characters')
  }
  if (!/^[A-Za-z0-9._:-]+$/.test(value)) {
    fail('expected a requestId containing only letters, digits, ".", "_", ":" or "-"')
  }
  return value
}

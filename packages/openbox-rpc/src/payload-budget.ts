/**
 * PayloadBudget — RPC 信封 JSON 负载的预算检查（不序列化）。
 *
 * 从 shared/plugin-renderer-rpc.ts 原样搬移：
 * - utf8ByteLength / jsonStringByteLength：精确 UTF-8 / JSON 转义字节计数
 * - inspectRpcPayload：深度优先遍历 + 环检测 + 预算断言
 *
 * 报错通过 fail 回调注入（而非内部抛协议特定错误），使 renderer（结构化
 * {code,message,path} issue）与 backend（裸字符串）的报错形状各自保持不变。
 * 预算错误码映射与文案（'payload exceeds serialized byte budget' 等）由
 * 各协议的 fail 实现决定；本函数按调用方传入的 code 分派。
 */

export interface RpcPayloadBudget {
  maxSerializedBytes: number
  maxDepth: number
  maxNodes: number
  maxArrayLength: number
  maxObjectKeys: number
  maxStringBytes: number
}

export interface RpcPayloadStats {
  serializedBytes: number
  depth: number
  nodes: number
}

/** 预算/形状违规回调；由调用方抛协议特定错误 */
export type RpcPayloadFail = (code: string, message: string, path: string) => never

/** 默认预算（与 renderer 的 PLUGIN_RENDERER_RPC_BUDGET 同值；backend 复用此默认） */
export const DEFAULT_RPC_PAYLOAD_BUDGET: Readonly<RpcPayloadBudget> = Object.freeze({
  maxSerializedBytes: 256 * 1024,
  maxDepth: 16,
  maxNodes: 4096,
  maxArrayLength: 512,
  maxObjectKeys: 256,
  maxStringBytes: 64 * 1024
})

export function utf8ByteLength(value: string): number {
  let bytes = 0
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code <= 0x7f) bytes += 1
    else if (code <= 0x7ff) bytes += 2
    else if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1)
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4
        index += 1
      } else {
        bytes += 3
      }
    } else bytes += 3
  }
  return bytes
}

export function jsonStringByteLength(value: string): number {
  let bytes = 2
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (
      code === 0x22 ||
      code === 0x5c ||
      code === 0x08 ||
      code === 0x09 ||
      code === 0x0a ||
      code === 0x0c ||
      code === 0x0d
    ) {
      bytes += 2
    } else if (code < 0x20 || (code >= 0xd800 && code <= 0xdfff)) {
      const next = value.charCodeAt(index + 1)
      if (code >= 0xd800 && code <= 0xdbff && next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4
        index += 1
      } else {
        bytes += 6
      }
    } else if (code <= 0x7f) bytes += 1
    else if (code <= 0x7ff) bytes += 2
    else bytes += 3
  }
  return bytes
}

function inspectObjectShape(
  value: object,
  path: string,
  fail: RpcPayloadFail
): PropertyDescriptorMap {
  const keys = Reflect.ownKeys(value)
  if (keys.some((key) => typeof key !== 'string')) {
    fail('INVALID_ENVELOPE', 'symbol properties are not supported', path)
  }
  const descriptors = Object.getOwnPropertyDescriptors(value)
  for (const key of keys as string[]) {
    if (Array.isArray(value) && key === 'length') continue
    const descriptor = descriptors[key]
    if (!descriptor.enumerable || !('value' in descriptor)) {
      fail(
        'INVALID_ENVELOPE',
        'accessors and non-enumerable properties are not supported',
        `${path}.${key}`
      )
    }
  }
  return descriptors
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false
  const prototype = Object.getPrototypeOf(value)
  return prototype === Object.prototype || prototype === null
}

export function inspectRpcPayload(
  value: unknown,
  budget: Readonly<RpcPayloadBudget>,
  fail: RpcPayloadFail
): RpcPayloadStats {
  let serializedBytes = 0
  let nodes = 0
  let maxDepth = 0
  const ancestors = new WeakSet<object>()

  const addBytes = (count: number, path: string): void => {
    serializedBytes += count
    if (serializedBytes > budget.maxSerializedBytes) {
      fail('PAYLOAD_TOO_LARGE', 'payload exceeds serialized byte budget', path)
    }
  }

  const visit = (current: unknown, depth: number, path: string): void => {
    nodes += 1
    if (nodes > budget.maxNodes) fail('PAYLOAD_TOO_COMPLEX', 'payload exceeds node budget', path)
    if (depth > budget.maxDepth) fail('PAYLOAD_TOO_DEEP', 'payload exceeds depth budget', path)
    maxDepth = Math.max(maxDepth, depth)

    if (current === null) {
      addBytes(4, path)
      return
    }
    if (typeof current === 'string') {
      if (utf8ByteLength(current) > budget.maxStringBytes) {
        fail('PAYLOAD_TOO_LARGE', 'string exceeds byte budget', path)
      }
      addBytes(jsonStringByteLength(current), path)
      return
    }
    if (typeof current === 'boolean') {
      addBytes(current ? 4 : 5, path)
      return
    }
    if (typeof current === 'number') {
      if (!Number.isFinite(current)) fail('INVALID_ENVELOPE', 'numbers must be finite', path)
      addBytes(String(Object.is(current, -0) ? 0 : current).length, path)
      return
    }
    if (typeof current !== 'object') {
      fail('INVALID_ENVELOPE', 'payload must contain only JSON-compatible values', path)
    }
    if (ancestors.has(current)) fail('INVALID_ENVELOPE', 'cyclic payloads are not supported', path)
    ancestors.add(current)

    if (Array.isArray(current)) {
      if (current.length > budget.maxArrayLength) {
        fail('PAYLOAD_TOO_COMPLEX', 'array exceeds item budget', path)
      }
      const descriptors = inspectObjectShape(current, path, fail)
      const ownKeys = Reflect.ownKeys(current).filter((key) => key !== 'length')
      if (ownKeys.length !== current.length) {
        fail(
          'INVALID_ENVELOPE',
          'sparse arrays and additional array properties are not supported',
          path
        )
      }
      addBytes(2 + Math.max(0, current.length - 1), path)
      for (let index = 0; index < current.length; index += 1) {
        if (!Object.prototype.hasOwnProperty.call(descriptors, String(index))) {
          fail('INVALID_ENVELOPE', 'sparse arrays are not supported', `${path}[${index}]`)
        }
        visit(descriptors[String(index)].value, depth + 1, `${path}[${index}]`)
      }
    } else {
      if (!isPlainObject(current))
        fail('INVALID_ENVELOPE', 'objects must have a plain prototype', path)
      const descriptors = inspectObjectShape(current, path, fail)
      const keys = Object.keys(descriptors)
      if (keys.length > budget.maxObjectKeys) {
        fail('PAYLOAD_TOO_COMPLEX', 'object exceeds key budget', path)
      }
      addBytes(2 + Math.max(0, keys.length - 1), path)
      for (const key of keys) {
        if (utf8ByteLength(key) > budget.maxStringBytes) {
          fail('PAYLOAD_TOO_LARGE', 'property name exceeds byte budget', `${path}.${key}`)
        }
        addBytes(jsonStringByteLength(key) + 1, path)
        visit(descriptors[key].value, depth + 1, `${path}.${key}`)
      }
    }
    ancestors.delete(current)
  }

  visit(value, 0, '$')
  return { serializedBytes, depth: maxDepth, nodes }
}

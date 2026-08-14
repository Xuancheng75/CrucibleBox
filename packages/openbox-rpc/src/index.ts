/**
 * openbox-rpc — backend/renderer 两套 RPC 的公共底层。
 *
 * 抽取自 shared/plugin-renderer-rpc.ts 与 shared/plugin-backend-rpc.ts 的重复实现：
 * - PayloadBudget（inspectRpcPayload + utf8ByteLength/jsonStringByteLength）
 * - RequestTracker（pending 上限追踪）
 * - Session 校验（token/requestId 格式）
 * - isPlainObject
 *
 * 约束：线上信封字节格式与各协议的错误码/文案由各协议文件保持；本包只提供
 * 无协议语义的纯函数与通用数据结构。错误上报通过 fail 回调注入，确保
 * renderer（结构化 issue）与 backend（裸字符串）的报错形状各自不变。
 */

export {
  utf8ByteLength,
  jsonStringByteLength,
  inspectRpcPayload,
  DEFAULT_RPC_PAYLOAD_BUDGET,
  type RpcPayloadBudget,
  type RpcPayloadStats
} from './payload-budget'

export { PendingRequestTracker, type PendingRequestTrackerErrorFactory } from './request-tracker'

export { isPlainObject, validateRpcToken, validateRpcRequestId } from './validation'

import type { GifDocument, GifFrame } from '../types'

const MEBIBYTE = 1024 * 1024

export interface GifDecodeLimits {
  maxFileBytes: number
  maxWidth: number
  maxHeight: number
  maxFrames: number
  maxTotalRgbaBytes: number
}

export interface GifOutputLimits {
  maxOutputs: number
  maxOutputRgbaBytes: number
}

export const DEFAULT_GIF_DECODE_LIMITS: Readonly<GifDecodeLimits> = Object.freeze({
  maxFileBytes: 64 * MEBIBYTE,
  maxWidth: 4096,
  maxHeight: 4096,
  maxFrames: 500,
  maxTotalRgbaBytes: 256 * MEBIBYTE
})

export const DEFAULT_GIF_SPLIT_OUTPUT_LIMITS: Readonly<GifOutputLimits> = Object.freeze({
  maxOutputs: 16,
  maxOutputRgbaBytes: 256 * MEBIBYTE
})

export type GifValidationErrorCode =
  | 'invalid-limit'
  | 'file-size'
  | 'dimensions'
  | 'frame-count'
  | 'frame-bounds'
  | 'decoded-frame'
  | 'rgba-budget'
  | 'output-count'
  | 'document-invariant'

export class GifValidationError extends Error {
  readonly code: GifValidationErrorCode

  constructor(code: GifValidationErrorCode, message: string) {
    super(message)
    this.name = 'GifValidationError'
    this.code = code
  }
}

export interface GifMetadataValidation {
  frameCount: number
  estimatedRgbaBytes: bigint
}

export interface GifOutputProjection {
  outputCount: number
  projectedRgbaBytes: bigint
}

export type GifCanvasInvariantResult =
  { ok: true } | { ok: false; message: string; frameIndex?: number }

interface GifFrameDescriptor {
  width: number
  height: number
  left: number
  top: number
}

function assertPositiveSafeInteger(value: number, label: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new GifValidationError('invalid-limit', `${label} 必须是正安全整数`)
  }
}

export function resolveGifDecodeLimits(
  overrides: Partial<GifDecodeLimits> = {}
): Readonly<GifDecodeLimits> {
  const limits = { ...DEFAULT_GIF_DECODE_LIMITS, ...overrides }
  assertPositiveSafeInteger(limits.maxFileBytes, 'maxFileBytes')
  assertPositiveSafeInteger(limits.maxWidth, 'maxWidth')
  assertPositiveSafeInteger(limits.maxHeight, 'maxHeight')
  assertPositiveSafeInteger(limits.maxFrames, 'maxFrames')
  assertPositiveSafeInteger(limits.maxTotalRgbaBytes, 'maxTotalRgbaBytes')
  return limits
}

export function resolveGifOutputLimits(
  overrides: Partial<GifOutputLimits> = {}
): Readonly<GifOutputLimits> {
  const limits = { ...DEFAULT_GIF_SPLIT_OUTPUT_LIMITS, ...overrides }
  assertPositiveSafeInteger(limits.maxOutputs, 'maxOutputs')
  assertPositiveSafeInteger(limits.maxOutputRgbaBytes, 'maxOutputRgbaBytes')
  return limits
}

function assertNonNegativeSafeInteger(
  value: number,
  label: string,
  code: GifValidationErrorCode
): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new GifValidationError(code, `${label} 必须是非负安全整数`)
  }
}

export function calculateRgbaByteLength(width: number, height: number, copies = 1): bigint {
  assertNonNegativeSafeInteger(width, 'width', 'dimensions')
  assertNonNegativeSafeInteger(height, 'height', 'dimensions')
  assertNonNegativeSafeInteger(copies, 'copies', 'dimensions')
  return BigInt(width) * BigInt(height) * 4n * BigInt(copies)
}

export function assertGifOutputProjection(
  width: number,
  height: number,
  outputCount: number,
  overrides: Partial<GifOutputLimits> = {}
): GifOutputProjection {
  const limits = resolveGifOutputLimits(overrides)
  assertPositiveSafeInteger(outputCount, 'outputCount')
  if (outputCount > limits.maxOutputs) {
    throw new GifValidationError(
      'output-count',
      `输出数量 ${outputCount} 超过上限 ${limits.maxOutputs}`
    )
  }
  const projectedRgbaBytes = calculateRgbaByteLength(width, height, outputCount)
  if (projectedRgbaBytes > BigInt(limits.maxOutputRgbaBytes)) {
    throw new GifValidationError(
      'rgba-budget',
      `输出预计需要 ${projectedRgbaBytes} 字节 RGBA 缓冲，超过上限 ${limits.maxOutputRgbaBytes} 字节`
    )
  }
  return { outputCount, projectedRgbaBytes }
}

export function limitGifOutputCount(
  width: number,
  height: number,
  requestedOutputCount: number,
  overrides: Partial<GifOutputLimits> = {}
): GifOutputProjection {
  const limits = resolveGifOutputLimits(overrides)
  assertNonNegativeSafeInteger(requestedOutputCount, 'requestedOutputCount', 'output-count')
  if (requestedOutputCount === 0) {
    return { outputCount: 0, projectedRgbaBytes: 0n }
  }

  const canvasBytes = calculateRgbaByteLength(width, height)
  if (canvasBytes === 0n) {
    throw new GifValidationError('dimensions', `输出画布尺寸无效：${width}×${height}`)
  }
  const byteLimitedCount = BigInt(limits.maxOutputRgbaBytes) / canvasBytes
  const boundedByteCount =
    byteLimitedCount > BigInt(Number.MAX_SAFE_INTEGER)
      ? Number.MAX_SAFE_INTEGER
      : Number(byteLimitedCount)
  const outputCount = Math.min(requestedOutputCount, limits.maxOutputs, boundedByteCount)
  if (outputCount < 1) {
    throw new GifValidationError(
      'rgba-budget',
      `单个 ${width}×${height} 输出画布已超过 ${limits.maxOutputRgbaBytes} 字节预算`
    )
  }
  return {
    outputCount,
    projectedRgbaBytes: canvasBytes * BigInt(outputCount)
  }
}

export function assertGifFileWithinLimits(
  file: { readonly size: number },
  overrides: Partial<GifDecodeLimits> = {}
): Readonly<GifDecodeLimits> {
  const limits = resolveGifDecodeLimits(overrides)
  if (!Number.isSafeInteger(file.size) || file.size <= 0) {
    throw new GifValidationError('file-size', 'GIF 文件为空或大小无效')
  }
  if (file.size > limits.maxFileBytes) {
    throw new GifValidationError(
      'file-size',
      `GIF 文件大小 ${file.size} 字节超过上限 ${limits.maxFileBytes} 字节`
    )
  }
  return limits
}

function assertLogicalScreen(
  width: number,
  height: number,
  limits: Readonly<GifDecodeLimits>
): void {
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width <= 0 || height <= 0) {
    throw new GifValidationError('dimensions', `GIF 画布尺寸无效：${width}×${height}`)
  }
  if (width > limits.maxWidth || height > limits.maxHeight) {
    throw new GifValidationError(
      'dimensions',
      `GIF 画布 ${width}×${height} 超过上限 ${limits.maxWidth}×${limits.maxHeight}`
    )
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null
}

function readDescriptor(
  value: unknown,
  frameIndex: number,
  kind: 'raw' | 'decoded'
): GifFrameDescriptor | null {
  const frame = asRecord(value)
  if (!frame) {
    throw new GifValidationError('frame-bounds', `第 ${frameIndex + 1} 帧结构无效`)
  }

  let descriptorValue: unknown
  if (kind === 'raw') {
    if (!Object.prototype.hasOwnProperty.call(frame, 'image')) return null
    const image = asRecord(frame.image)
    if (!image) {
      throw new GifValidationError('frame-bounds', `第 ${frameIndex + 1} 帧图像描述无效`)
    }
    descriptorValue = image.descriptor
  } else {
    descriptorValue = frame.dims
  }

  const descriptor = asRecord(descriptorValue)
  if (!descriptor) {
    throw new GifValidationError('frame-bounds', `第 ${frameIndex + 1} 帧尺寸描述无效`)
  }

  const width = descriptor.width
  const height = descriptor.height
  const left = kind === 'raw' ? descriptor.left : (descriptor.left ?? 0)
  const top = kind === 'raw' ? descriptor.top : (descriptor.top ?? 0)
  if (
    typeof width !== 'number' ||
    typeof height !== 'number' ||
    typeof left !== 'number' ||
    typeof top !== 'number'
  ) {
    throw new GifValidationError('frame-bounds', `第 ${frameIndex + 1} 帧尺寸字段无效`)
  }
  return { width, height, left, top }
}

function validateDescriptors(
  width: number,
  height: number,
  descriptors: readonly GifFrameDescriptor[],
  limits: Readonly<GifDecodeLimits>
): GifMetadataValidation {
  const frameCount = descriptors.length
  if (frameCount === 0) {
    throw new GifValidationError('frame-count', 'GIF 不包含可解码图像帧')
  }
  if (frameCount > limits.maxFrames) {
    throw new GifValidationError(
      'frame-count',
      `GIF 帧数 ${frameCount} 超过上限 ${limits.maxFrames}`
    )
  }

  let patchBytes = 0n
  for (let index = 0; index < descriptors.length; index++) {
    const descriptor = descriptors[index]
    const { width: frameWidth, height: frameHeight, left, top } = descriptor
    if (
      !Number.isSafeInteger(frameWidth) ||
      !Number.isSafeInteger(frameHeight) ||
      !Number.isSafeInteger(left) ||
      !Number.isSafeInteger(top) ||
      frameWidth <= 0 ||
      frameHeight <= 0 ||
      left < 0 ||
      top < 0
    ) {
      throw new GifValidationError('frame-bounds', `第 ${index + 1} 帧边界无效`)
    }
    if (
      BigInt(left) + BigInt(frameWidth) > BigInt(width) ||
      BigInt(top) + BigInt(frameHeight) > BigInt(height)
    ) {
      throw new GifValidationError('frame-bounds', `第 ${index + 1} 帧区域超出 GIF 画布`)
    }
    patchBytes += calculateRgbaByteLength(frameWidth, frameHeight)
  }

  // 解码峰值同时持有：所有完整画布帧、工作画布、disposal=3 快照和解压 patch。
  const canvasBytes = calculateRgbaByteLength(width, height)
  const estimatedRgbaBytes = canvasBytes * (BigInt(frameCount) + 2n) + patchBytes
  if (estimatedRgbaBytes > BigInt(limits.maxTotalRgbaBytes)) {
    throw new GifValidationError(
      'rgba-budget',
      `GIF 预计需要 ${estimatedRgbaBytes} 字节 RGBA 缓冲，超过上限 ${limits.maxTotalRgbaBytes} 字节`
    )
  }

  return { frameCount, estimatedRgbaBytes }
}

export function validateParsedGifMetadata(
  width: number,
  height: number,
  frames: readonly unknown[],
  overrides: Partial<GifDecodeLimits> = {}
): GifMetadataValidation {
  const limits = resolveGifDecodeLimits(overrides)
  assertLogicalScreen(width, height, limits)
  const descriptors: GifFrameDescriptor[] = []
  for (let index = 0; index < frames.length; index++) {
    const descriptor = readDescriptor(frames[index], index, 'raw')
    if (descriptor) descriptors.push(descriptor)
  }
  return validateDescriptors(width, height, descriptors, limits)
}

export function validateDecodedGifFrames(
  width: number,
  height: number,
  frames: readonly unknown[],
  expectedFrameCount: number,
  overrides: Partial<GifDecodeLimits> = {}
): GifMetadataValidation {
  const limits = resolveGifDecodeLimits(overrides)
  assertLogicalScreen(width, height, limits)
  if (frames.length !== expectedFrameCount) {
    throw new GifValidationError(
      'decoded-frame',
      `GIF 解码帧数 ${frames.length} 与元数据帧数 ${expectedFrameCount} 不一致`
    )
  }

  const descriptors = frames.map((frame, index) => {
    const descriptor = readDescriptor(frame, index, 'decoded')
    if (!descriptor) {
      throw new GifValidationError('decoded-frame', `第 ${index + 1} 帧缺少尺寸描述`)
    }
    const record = asRecord(frame)
    const patch = record?.patch
    if (!(patch instanceof Uint8ClampedArray)) {
      throw new GifValidationError('decoded-frame', `第 ${index + 1} 帧缺少 RGBA patch`)
    }
    const expectedPatchBytes = calculateRgbaByteLength(descriptor.width, descriptor.height)
    if (BigInt(patch.byteLength) !== expectedPatchBytes) {
      throw new GifValidationError('decoded-frame', `第 ${index + 1} 帧 RGBA patch 长度不匹配`)
    }
    return descriptor
  })
  return validateDescriptors(width, height, descriptors, limits)
}

export function validateGifEncodeBudget(
  frames: readonly GifFrame[],
  width: number,
  height: number,
  overrides: Partial<GifDecodeLimits> = {}
): GifMetadataValidation {
  const limits = resolveGifDecodeLimits(overrides)
  assertLogicalScreen(width, height, limits)
  const frameCount = frames.length
  if (frameCount === 0) {
    throw new GifValidationError('frame-count', '无法编码不含帧的 GIF')
  }
  if (frameCount > limits.maxFrames) {
    throw new GifValidationError(
      'frame-count',
      `GIF 帧数 ${frameCount} 超过上限 ${limits.maxFrames}`
    )
  }

  // 输入文档全部帧 + 当前量化工作帧 + 编码工作缓冲的保守 RGBA 等价预算。
  const estimatedRgbaBytes = calculateRgbaByteLength(width, height) * (BigInt(frameCount) + 2n)
  if (estimatedRgbaBytes > BigInt(limits.maxTotalRgbaBytes)) {
    throw new GifValidationError(
      'rgba-budget',
      `GIF 编码预计需要 ${estimatedRgbaBytes} 字节工作缓冲，超过上限 ${limits.maxTotalRgbaBytes} 字节`
    )
  }
  return { frameCount, estimatedRgbaBytes }
}

export function validateGifFramesCanvasInvariant(
  frames: readonly GifFrame[],
  width: number,
  height: number
): GifCanvasInvariantResult {
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width <= 0 || height <= 0) {
    return { ok: false, message: `文档画布尺寸无效：${width}×${height}` }
  }
  if (frames.length === 0) {
    return { ok: false, message: '文档必须至少包含一帧' }
  }

  const expectedBytes = calculateRgbaByteLength(width, height)
  for (let index = 0; index < frames.length; index++) {
    const imageData = frames[index].imageData
    if (imageData.width !== width || imageData.height !== height) {
      return {
        ok: false,
        frameIndex: index,
        message: `第 ${index + 1} 帧尺寸 ${imageData.width}×${imageData.height} 与文档画布 ${width}×${height} 不一致`
      }
    }
    if (BigInt(imageData.data.byteLength) !== expectedBytes) {
      return {
        ok: false,
        frameIndex: index,
        message: `第 ${index + 1} 帧 RGBA 数据长度与画布尺寸不一致`
      }
    }
  }
  return { ok: true }
}

export function validateGifDocumentCanvasInvariant(
  document: GifDocument
): GifCanvasInvariantResult {
  return validateGifFramesCanvasInvariant(document.frames, document.width, document.height)
}

export function assertGifFramesCanvasInvariant(
  frames: readonly GifFrame[],
  width: number,
  height: number
): void {
  const result = validateGifFramesCanvasInvariant(frames, width, height)
  if (!result.ok) {
    throw new GifValidationError('document-invariant', result.message)
  }
}

export function assertGifDocumentCanvasInvariant(document: GifDocument): void {
  const result = validateGifDocumentCanvasInvariant(document)
  if (!result.ok) {
    throw new GifValidationError('document-invariant', result.message)
  }
}

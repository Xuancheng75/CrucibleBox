/** Typed message contract used by the renderer and the trusted host service. */

export interface OcrWorkerStatus {
  available: boolean
  running: boolean
  path?: string
  protocolVersion?: number
  timeoutSeconds?: number
  reason?: string
}

export interface EngineStatus {
  ocrWorker?: OcrWorkerStatus
  pdfium?: {
    available: boolean
    initialized?: boolean
    path?: string
    version?: string
    runtimeDownload?: boolean
    error?: string | null
  }
  models?: {
    directory?: string
    default?: {
      id?: string
      version?: string
      ready?: boolean
      offline?: boolean
      missing?: string[]
      invalid?: string[]
      error?: string
    }
  }
  gpu?: { available: boolean; provider?: string; device?: string }
  workers?: { ocr: number; parser: number; converter: number }
  config?: {
    device?: string
    modelDirectory?: string
    dictionaryPath?: string
    cacheDirectory?: string
    outputDirectory?: string
  }
}

export interface ModelCatalogArtifact {
  name: string
  url: string
  sources?: string[]
  sha256: string
  bytes?: number
  purpose?: string
}

export interface ModelCatalogEntry {
  id: string
  name: string
  version: string
  description: string
  recommended?: boolean
  default?: boolean
  offline?: boolean
  license?: string
  totalBytes?: number
  artifacts: ModelCatalogArtifact[]
}

export interface ModelBundleStatus {
  id: string
  version?: string
  ready: boolean
  offline?: boolean
  default?: boolean
  missing?: string[]
  invalid?: string[]
  error?: string
}

export interface ModelListResponse {
  directory?: string
  models: Array<Record<string, unknown>>
  count?: number
  bundles?: ModelBundleStatus[]
}

export interface OcrProgress {
  stage: string
  percent: number
  message: string
  [key: string]: unknown
}

export interface OcrBlock {
  text: string
  polygon: [[number, number], [number, number], [number, number], [number, number]]
  bbox: [number, number, number, number]
  confidence: number
}

export interface OcrResult {
  type: 'result'
  requestId: string
  text: string
  blocks: OcrBlock[]
  model?: {
    detectionSha256: string
    recognitionSha256: string
    dictionarySha256: string
    device: string
  }
}

export interface DocumentBlock {
  id: string
  type: string
  content?: string
  language?: string
  [key: string]: unknown
}

export interface ParsedDocument {
  id: string
  source: {
    path: string
    mime: string
    size: number
    hash: string
    engine: string
    engineVersion: string
  }
  metadata: {
    pageCount: number
    hasTextLayer: boolean
    isScanned: boolean
    hasTables: boolean
    hasFormulas: boolean
    hasImages: boolean
    [key: string]: unknown
  }
  pages: Array<{
    number: number
    width: number
    height: number
    blocks: DocumentBlock[]
  }>
  structure: {
    outline: unknown[]
    readingOrder: string[]
  }
}

export interface ParsedDocumentResult {
  route: 'native' | 'mixed' | 'ocr' | string
  requiresOcr: boolean
  ocrPageNumbers: number[]
  warnings: Array<{ code?: string; message?: string }>
  document: ParsedDocument
  outputDirectory?: string
  outputs?: {
    directory?: string
    textCharacters?: number
    files?: Array<{ kind: string; path: string; bytes: number }>
  }
}

export interface ChunkResult {
  documentId: string
  strategy: string
  count: number
  chunks: Array<Record<string, unknown>>
}

export interface PdfSplitResult {
  sourcePath: string
  outputDirectory: string
  pageCount: number
  pagesPerFile: number
  fileCount: number
  files: Array<{
    index: number
    path: string
    startPage: number
    endPage: number
    pageCount: number
  }>
}

export interface ConversionResult {
  outputPath: string
  target: string
  bytes: number
  documentId?: string
  cacheKey?: string
  warnings?: string[]
}

export interface BatchResult {
  operation: 'ocr' | 'parse' | 'convert' | string
  count: number
  items: Array<{ path: string; result: unknown }>
}

export function isOcrResult(value: unknown): value is OcrResult {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Record<string, unknown>
  return (
    candidate.type === 'result' &&
    typeof candidate.text === 'string' &&
    Array.isArray(candidate.blocks)
  )
}

/** Turn a trusted-service error envelope into a useful renderer error. */
export function backendErrorMessage(response: unknown, fallback: string): string {
  if (!response || typeof response !== 'object') return fallback
  const candidate = response as Record<string, unknown>
  if (typeof candidate.error === 'string' && candidate.error.length > 0) {
    const code =
      typeof candidate.code === 'string' && candidate.code.length > 0 ? ` [${candidate.code}]` : ''
    return `${candidate.error}${code}`
  }
  if (typeof candidate.message === 'string' && candidate.message.length > 0)
    return candidate.message
  if (typeof candidate.code === 'string' && candidate.code.length > 0) return candidate.code
  return fallback
}

/** MessagePort/RPC payloads cannot contain JavaScript `undefined`. */
function omitUndefined<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.filter((item) => item !== undefined).map((item) => omitUndefined(item)) as T
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .filter(([, item]) => item !== undefined)
        .map(([key, item]) => [key, omitUndefined(item)])
    ) as T
  }
  return value
}

function isModelCatalogEntry(value: unknown): value is ModelCatalogEntry {
  if (!value || typeof value !== 'object') return false
  const entry = value as Record<string, unknown>
  if (
    typeof entry.id !== 'string' ||
    typeof entry.name !== 'string' ||
    typeof entry.version !== 'string' ||
    typeof entry.description !== 'string' ||
    !Array.isArray(entry.artifacts)
  ) {
    return false
  }
  return entry.artifacts.every((artifact) => {
    if (!artifact || typeof artifact !== 'object') return false
    const candidate = artifact as Record<string, unknown>
    const validSources =
      candidate.sources === undefined ||
      (Array.isArray(candidate.sources) &&
        candidate.sources.length > 0 &&
        candidate.sources.every(
          (source) => typeof source === 'string' && source.startsWith('https://')
        ))
    return (
      typeof candidate.name === 'string' &&
      typeof candidate.url === 'string' &&
      candidate.url.startsWith('https://') &&
      typeof candidate.sha256 === 'string' &&
      /^[0-9a-f]{64}$/i.test(candidate.sha256) &&
      validSources
    )
  })
}

export interface DocumentTaskSnapshot {
  taskId: string
  resourceKey: string
  status: 'queued' | 'running' | 'paused' | 'succeeded' | 'failed' | 'cancelled' | string
  progress?: OcrProgress
  result?:
    | OcrResult
    | ParsedDocumentResult
    | ChunkResult
    | ConversionResult
    | BatchResult
    | Record<string, unknown>
  error?: { name?: string; message?: string }
}

export interface TaskAccepted {
  taskId: string
  status: 'queued' | 'running' | string
}

export interface DocumentProgressMessage {
  type: 'document.progress'
  taskId: string
  progress: OcrProgress
}

export function isDocumentProgressMessage(value: unknown): value is DocumentProgressMessage {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Record<string, unknown>
  const progress = candidate.progress
  return (
    candidate.type === 'document.progress' &&
    typeof candidate.taskId === 'string' &&
    Boolean(progress) &&
    typeof progress === 'object' &&
    typeof (progress as Record<string, unknown>).percent === 'number'
  )
}

export function isTerminalTask(snapshot: DocumentTaskSnapshot): boolean {
  return ['succeeded', 'failed', 'cancelled'].includes(snapshot.status)
}

export async function getStatus(
  send: (message: unknown) => Promise<unknown>
): Promise<EngineStatus> {
  const response = await send({ type: 'getStatus' })
  if (!response || typeof response !== 'object') {
    throw new Error(backendErrorMessage(response, '引擎状态读取失败：后端未返回有效响应'))
  }
  const candidate = response as { status?: unknown }
  if (!candidate.status || typeof candidate.status !== 'object') {
    throw new Error(backendErrorMessage(response, '引擎状态读取失败'))
  }
  return candidate.status as EngineStatus
}

export async function startOcr(
  send: (message: unknown) => Promise<unknown>,
  path: string,
  options?: { language?: string; device?: string }
): Promise<TaskAccepted> {
  const response = (await send(
    omitUndefined({
      type: 'document.ocr',
      path,
      options
    })
  )) as Partial<TaskAccepted> & {
    error?: string
    code?: string
  }
  if (typeof response.taskId !== 'string' || response.taskId.length === 0) {
    throw new Error(response.error ?? response.code ?? 'OCR 任务启动失败')
  }
  return { taskId: response.taskId, status: response.status ?? 'queued' }
}

export async function startParse(
  send: (message: unknown) => Promise<unknown>,
  path: string,
  options?: { outputDirectory?: string }
): Promise<TaskAccepted> {
  const response = (await send(omitUndefined({ type: 'document.parse', path, options }))) as Partial<TaskAccepted> & {
    error?: string
    code?: string
  }
  if (typeof response.taskId !== 'string' || response.taskId.length === 0) {
    throw new Error(response.error ?? response.code ?? 'PDF 解析任务启动失败')
  }
  return { taskId: response.taskId, status: response.status ?? 'queued' }
}

export async function startPdfSplit(
  send: (message: unknown) => Promise<unknown>,
  path: string,
  options?: { outputDirectory?: string; pagesPerFile?: number }
): Promise<TaskAccepted> {
  const response = (await send(
    omitUndefined({ type: 'document.pdf.split', path, options })
  )) as Partial<TaskAccepted> & {
    error?: string
    code?: string
  }
  if (typeof response.taskId !== 'string' || response.taskId.length === 0) {
    throw new Error(response.error ?? response.code ?? 'PDF 拆分任务启动失败')
  }
  return { taskId: response.taskId, status: response.status ?? 'queued' }
}

export async function startChunk(
  send: (message: unknown) => Promise<unknown>,
  path: string,
  options?: Record<string, unknown>
): Promise<TaskAccepted> {
  const response = (await send(
    omitUndefined({
      type: 'document.chunk',
      path,
      options
    })
  )) as Partial<TaskAccepted> & {
    error?: string
    code?: string
  }
  if (typeof response.taskId !== 'string' || response.taskId.length === 0) {
    throw new Error(response.error ?? response.code ?? '文档切分任务启动失败')
  }
  return { taskId: response.taskId, status: response.status ?? 'queued' }
}

export async function startConvert(
  send: (message: unknown) => Promise<unknown>,
  path: string,
  target: string,
  outputPath?: string
): Promise<TaskAccepted> {
  const response = (await send(
    omitUndefined({
      type: 'document.convert',
      path,
      target,
      outputPath
    })
  )) as Partial<TaskAccepted> & {
    error?: string
    code?: string
  }
  if (typeof response.taskId !== 'string' || response.taskId.length === 0) {
    throw new Error(response.error ?? response.code ?? '文档转换任务启动失败')
  }
  return { taskId: response.taskId, status: response.status ?? 'queued' }
}

export async function startBatch(
  send: (message: unknown) => Promise<unknown>,
  paths: string[],
  operation: 'ocr' | 'parse' | 'convert',
  target?: string
): Promise<TaskAccepted> {
  const response = (await send(
    omitUndefined({
      type: 'document.batch',
      paths,
      operation,
      target
    })
  )) as Partial<TaskAccepted> & {
    error?: string
    code?: string
  }
  if (typeof response.taskId !== 'string' || response.taskId.length === 0) {
    throw new Error(response.error ?? response.code ?? '批量任务启动失败')
  }
  return { taskId: response.taskId, status: response.status ?? 'queued' }
}

export function isParsedDocumentResult(value: unknown): value is ParsedDocumentResult {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Record<string, unknown>
  const document = candidate.document
  return (
    (candidate.route === 'native' || candidate.route === 'mixed' || candidate.route === 'ocr') &&
    Boolean(document) &&
    typeof document === 'object' &&
    Boolean((document as Record<string, unknown>).metadata)
  )
}

export async function getTask(
  send: (message: unknown) => Promise<unknown>,
  taskId: string
): Promise<DocumentTaskSnapshot> {
  const response = (await send({ type: 'document.jobs.get', taskId })) as DocumentTaskSnapshot & {
    error?: string
  }
  if (response.status === undefined) {
    throw new Error(response.error ?? '任务不存在')
  }
  return response
}

export async function cancelTask(
  send: (message: unknown) => Promise<unknown>,
  taskId: string
): Promise<void> {
  const response = (await send({ type: 'document.jobs.cancel', taskId })) as {
    success?: boolean
    error?: string
  }
  if (!response.success) throw new Error(response.error ?? '取消任务失败')
}

async function setTaskState(
  send: (message: unknown) => Promise<unknown>,
  type: 'document.jobs.pause' | 'document.jobs.resume',
  taskId: string
): Promise<void> {
  const response = (await send({ type, taskId })) as { success?: boolean; error?: string }
  if (!response.success) throw new Error(response.error ?? '任务状态更新失败')
}

export const pauseTask = (send: (message: unknown) => Promise<unknown>, taskId: string) =>
  setTaskState(send, 'document.jobs.pause', taskId)

export const resumeTask = (send: (message: unknown) => Promise<unknown>, taskId: string) =>
  setTaskState(send, 'document.jobs.resume', taskId)

export async function retryTask(
  send: (message: unknown) => Promise<unknown>,
  taskId: string
): Promise<TaskAccepted> {
  const response = (await send({
    type: 'document.jobs.retry',
    taskId
  })) as Partial<TaskAccepted> & {
    error?: string
    code?: string
  }
  if (typeof response.taskId !== 'string' || response.taskId.length === 0) {
    throw new Error(response.error ?? response.code ?? '任务重试失败')
  }
  return { taskId: response.taskId, status: response.status ?? 'queued' }
}

export async function listModels(
  send: (message: unknown) => Promise<unknown>
): Promise<ModelListResponse> {
  const response = await send({ type: 'document.models.list' })
  if (!response || typeof response !== 'object') {
    throw new Error(backendErrorMessage(response, '模型列表读取失败：后端未返回有效响应'))
  }
  const candidate = response as { models?: unknown[] }
  if (!Array.isArray(candidate.models))
    throw new Error(backendErrorMessage(response, '模型列表读取失败'))
  return response as ModelListResponse
}

export async function listModelCatalog(
  send: (message: unknown) => Promise<unknown>
): Promise<ModelCatalogEntry[]> {
  const response = await send({ type: 'document.models.catalog' })
  if (!response || typeof response !== 'object') {
    throw new Error(backendErrorMessage(response, '模型目录读取失败：后端未返回有效响应'))
  }
  const candidate = response as { catalog?: unknown }
  if (!Array.isArray(candidate.catalog) || !candidate.catalog.every(isModelCatalogEntry)) {
    throw new Error(backendErrorMessage(response, '模型目录读取失败'))
  }
  return candidate.catalog as ModelCatalogEntry[]
}

export async function installModelBundle(
  send: (message: unknown) => Promise<unknown>,
  modelId: string
): Promise<unknown> {
  const response = (await send({ type: 'document.models.installBundle', modelId })) as {
    success?: boolean
    error?: string
  }
  if (!response.success) throw new Error(response.error ?? '模型包安装失败')
  return response
}

export async function installModel(
  send: (message: unknown) => Promise<unknown>,
  sourcePath: string,
  name?: string
): Promise<unknown> {
  const response = (await send(
    omitUndefined({ type: 'document.models.install', sourcePath, name })
  )) as {
    success?: boolean
    error?: string
  }
  if (!response.success) throw new Error(response.error ?? '模型安装失败')
  return response
}

export async function installRemoteModel(
  send: (message: unknown) => Promise<unknown>,
  url: string,
  sha256: string,
  name?: string
): Promise<unknown> {
  const response = (await send(
    omitUndefined({ type: 'document.models.install', url, sha256, name })
  )) as {
    success?: boolean
    error?: string
  }
  if (!response.success) throw new Error(response.error ?? '远程模型安装失败')
  return response
}

export async function updateRemoteModel(
  send: (message: unknown) => Promise<unknown>,
  url: string,
  sha256: string,
  name: string
): Promise<unknown> {
  const response = (await send(
    omitUndefined({ type: 'document.models.update', url, sha256, name })
  )) as {
    success?: boolean
    error?: string
  }
  if (!response.success) throw new Error(response.error ?? '远程模型更新失败')
  return response
}

export async function removeModel(
  send: (message: unknown) => Promise<unknown>,
  path: string
): Promise<void> {
  const response = (await send({ type: 'document.models.remove', path })) as {
    success?: boolean
    error?: string
  }
  if (!response.success) throw new Error(response.error ?? '模型删除失败')
}

export async function clearCache(send: (message: unknown) => Promise<unknown>): Promise<unknown> {
  const response = (await send({ type: 'document.cache.clear' })) as {
    success?: boolean
    error?: string
  }
  if (!response.success) throw new Error(response.error ?? '缓存清理失败')
  return response
}

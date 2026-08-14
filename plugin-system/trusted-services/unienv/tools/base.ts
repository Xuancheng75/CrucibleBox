// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  symlinkSync,
  unlinkSync
} from 'fs'
import { open, rename, unlink } from 'fs/promises'
import type { FileHandle } from 'fs/promises'
import { createHash } from 'node:crypto'
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'path'
import { runProcess } from '../process-runner'

export const MAX_DOWNLOAD_BYTES = 512 * 1024 * 1024
export const DOWNLOAD_IDLE_TIMEOUT_MS = 30_000

const EXPAND_ARCHIVE_SCRIPT =
  "$ErrorActionPreference = 'Stop'; Expand-Archive -LiteralPath $env:OPENBOX_ARCHIVE_PATH -DestinationPath $env:OPENBOX_ARCHIVE_DEST -Force"
const INSTALL_STAGING_PREFIX = '.unienv-staging-'

export interface ToolInfo {
  installed: boolean
  version?: string
  path?: string
  error?: string
}

export interface InstallProgress {
  stage: 'downloading' | 'installing' | 'configuring' | 'done'
  percent: number
  message: string
}

export type ProgressCallback = (p: InstallProgress) => void

export interface ToolDef {
  id: string
  displayName: string
  icon: string
  description: string
  detect(installRoot: string): Promise<ToolInfo>
  listVersions(): Promise<string[]>
  install(
    version: string,
    installRoot: string,
    onProgress: ProgressCallback,
    opts?: InstallOptions
  ): Promise<void>
  uninstall(installRoot: string, onProgress: ProgressCallback): Promise<void>
  switchVersion(version: string, installRoot: string): Promise<void>
}

export interface InstallOptions {
  downloadMirror?: string
  signal?: AbortSignal
}

export interface DownloadOptions {
  expectedSha256: string
  signal?: AbortSignal
  maxBytes?: number
  idleTimeoutMs?: number
}

export class DownloadIntegrityError extends Error {
  readonly expectedSha256: string
  readonly actualSha256: string

  constructor(expectedSha256: string, actualSha256: string) {
    super(`下载制品 SHA-256 不匹配: 预期 ${expectedSha256}，实际 ${actualSha256}`)
    this.name = 'DownloadIntegrityError'
    this.expectedSha256 = expectedSha256
    this.actualSha256 = actualSha256
  }
}

function createAbortError(): Error {
  const error = new Error('操作已取消')
  error.name = 'AbortError'
  return error
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) throw createAbortError()
}

function assertHttpsUrl(value: string): void {
  let parsed: URL
  try {
    parsed = new URL(value)
  } catch {
    throw new TypeError('下载地址必须是有效的 HTTPS URL')
  }
  if (parsed.protocol !== 'https:' || parsed.username !== '' || parsed.password !== '') {
    throw new TypeError('下载地址必须使用 HTTPS 且不能包含凭据')
  }
}

function readContentLength(response: globalThis.Response, maxBytes: number): number {
  const raw = response.headers.get('content-length')
  if (raw === null) return 0
  const value = Number(raw)
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error('下载响应包含无效的 Content-Length')
  }
  if (value > maxBytes) {
    throw new Error(`下载响应超过 ${formatBytes(maxBytes)} 上限`)
  }
  return value
}

async function writeAll(file: FileHandle, buffer: Buffer): Promise<void> {
  let offset = 0
  while (offset < buffer.byteLength) {
    const { bytesWritten } = await file.write(buffer, offset, buffer.byteLength - offset, null)
    if (bytesWritten <= 0) throw new Error('无法继续写入下载文件')
    offset += bytesWritten
  }
}

async function readWithIdleTimeout(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  idleTimeoutMs: number
): Promise<ReadableStreamReadResult<Uint8Array>> {
  let timer: ReturnType<typeof setTimeout> | undefined
  const timeout = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => {
      const error = new Error(`下载响应超过 ${idleTimeoutMs}ms 未产生数据`)
      error.name = 'DownloadIdleTimeoutError'
      reject(error)
      void reader.cancel(error).catch(() => undefined)
    }, idleTimeoutMs)
  })
  try {
    return await Promise.race([reader.read(), timeout])
  } finally {
    if (timer) clearTimeout(timer)
  }
}

function pathEntryExists(path: string): boolean {
  try {
    lstatSync(path)
    return true
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false
    throw error
  }
}

function assertDirectStagingDir(versionRoot: string, stagingDir: string): void {
  const canonicalRoot = realpathSync(versionRoot)
  const resolvedStaging = resolve(stagingDir)
  const canonicalParent = realpathSync(dirname(resolvedStaging))
  if (
    canonicalParent !== canonicalRoot ||
    !basename(resolvedStaging).startsWith(INSTALL_STAGING_PREFIX)
  ) {
    throw new Error(`拒绝操作 version 目录外的 staging: ${stagingDir}`)
  }
}

export function createInstallStagingDir(versionRoot: string): string {
  const canonicalRoot = realpathSync(versionRoot)
  return mkdtempSync(join(canonicalRoot, INSTALL_STAGING_PREFIX))
}

export function prepareDirectInstallDirectory(directory: string, label: string): void {
  if (!pathEntryExists(directory)) {
    mkdirSync(directory, { recursive: true })
    return
  }
  const stats = lstatSync(directory)
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`${label} 的安装目标不是普通目录，拒绝覆盖: ${directory}`)
  }
  if (readdirSync(directory).length > 0) {
    throw new Error(`${label} 的安装目标已包含文件，拒绝覆盖: ${directory}`)
  }
}

export function cleanupInstallStagingDir(versionRoot: string, stagingDir: string): void {
  assertDirectStagingDir(versionRoot, stagingDir)
  if (!pathEntryExists(stagingDir)) return
  const stats = lstatSync(stagingDir)
  if (!stats.isDirectory() || stats.isSymbolicLink()) {
    throw new Error(`拒绝递归删除非普通 staging 目录: ${stagingDir}`)
  }
  rmSync(stagingDir, { force: true, recursive: true })
}

export function recoverInterruptedInstallStaging(versionRoots: readonly string[]): string[] {
  const removed: string[] = []
  for (const versionRoot of versionRoots) {
    if (!pathEntryExists(versionRoot)) continue
    const rootStats = lstatSync(versionRoot)
    if (!rootStats.isDirectory() || rootStats.isSymbolicLink()) {
      throw new Error(`版本目录不是普通目录，拒绝恢复: ${versionRoot}`)
    }
    for (const entry of readdirSync(versionRoot)) {
      if (!entry.startsWith(INSTALL_STAGING_PREFIX)) continue
      const stagingDir = join(versionRoot, entry)
      cleanupInstallStagingDir(versionRoot, stagingDir)
      removed.push(stagingDir)
    }
  }
  return removed
}

export function promoteStagedRuntime(
  versionRoot: string,
  stagingDir: string,
  sourceDir: string,
  finalDir: string
): void {
  assertDirectStagingDir(versionRoot, stagingDir)
  const canonicalRoot = realpathSync(versionRoot)
  const canonicalStaging = realpathSync(stagingDir)
  const canonicalSource = realpathSync(sourceDir)
  const sourceRelative = relative(canonicalStaging, canonicalSource)
  if (
    sourceRelative === '' ||
    sourceRelative === '..' ||
    sourceRelative.startsWith(`..${sep}`) ||
    isAbsolute(sourceRelative)
  ) {
    throw new Error(`拒绝提升 staging 外的运行时目录: ${sourceDir}`)
  }
  const sourceStats = lstatSync(sourceDir)
  if (!sourceStats.isDirectory() || sourceStats.isSymbolicLink()) {
    throw new Error(`staging 运行时必须是普通目录: ${sourceDir}`)
  }
  if (realpathSync(dirname(finalDir)) !== canonicalRoot) {
    throw new Error(`最终运行时目录必须直属于 version 目录: ${finalDir}`)
  }
  if (pathEntryExists(finalDir)) {
    throw new Error(`最终运行时目录已存在，拒绝覆盖: ${finalDir}`)
  }
  renameSync(canonicalSource, finalDir)
}

async function abortableDelay(delayMs: number, signal?: AbortSignal): Promise<void> {
  throwIfAborted(signal)
  await new Promise<void>((resolve, reject) => {
    const onAbort = (): void => {
      clearTimeout(timer)
      reject(createAbortError())
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort)
      resolve()
    }, delayMs)
    signal?.addEventListener('abort', onAbort, { once: true })
    if (signal?.aborted) onAbort()
  })
}

export async function fetchWithTimeout(
  url: string,
  timeoutMs = 120000,
  retries = 2,
  signal?: AbortSignal
): Promise<globalThis.Response> {
  assertHttpsUrl(url)
  let lastError: Error | null = null
  for (let attempt = 0; attempt <= retries; attempt++) {
    throwIfAborted(signal)
    if (attempt > 0) {
      const delay = Math.min(2000 * Math.pow(2, attempt - 1), 15000)
      await abortableDelay(delay, signal)
    }
    const controller = new AbortController()
    let timedOut = false
    const onAbort = (): void => controller.abort()
    signal?.addEventListener('abort', onAbort, { once: true })
    if (signal?.aborted) onAbort()
    const timer = setTimeout(() => {
      timedOut = true
      controller.abort()
    }, timeoutMs)
    try {
      const response = await fetch(url, { signal: controller.signal })
      if (response.url !== '') assertHttpsUrl(response.url)
      if (!response.ok) {
        throw new Error(`下载失败: HTTP ${response.status} ${response.statusText}`)
      }
      return response
    } catch (err) {
      if (signal?.aborted) throw createAbortError()
      if (timedOut) {
        const filename = url.split('/').pop() || url
        lastError = new Error(`下载超时(>${timeoutMs / 1000}s): ${filename}`)
      } else {
        const e = err as Error
        lastError = new Error(`下载失败: ${e.message}`)
      }
      if (attempt === retries) throw lastError
    } finally {
      clearTimeout(timer)
      signal?.removeEventListener('abort', onAbort)
    }
  }
  throw lastError ?? new Error('下载失败')
}

export async function downloadWithProgress(
  url: string,
  destPath: string,
  onProgress: ProgressCallback,
  stageLabel: string,
  options: DownloadOptions
): Promise<void> {
  const { expectedSha256, signal } = options
  const maxBytes = options.maxBytes ?? MAX_DOWNLOAD_BYTES
  const idleTimeoutMs = options.idleTimeoutMs ?? DOWNLOAD_IDLE_TIMEOUT_MS
  if (!/^[a-f0-9]{64}$/.test(expectedSha256)) {
    throw new TypeError('expectedSha256 必须是小写 SHA-256 十六进制摘要')
  }
  if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0) {
    throw new TypeError('maxBytes 必须是正安全整数')
  }
  if (!Number.isSafeInteger(idleTimeoutMs) || idleTimeoutMs <= 0) {
    throw new TypeError('idleTimeoutMs 必须是正安全整数')
  }
  throwIfAborted(signal)
  const response = await fetchWithTimeout(url, 120000, 2, signal)
  let contentLength: number
  try {
    contentLength = readContentLength(response, maxBytes)
  } catch (error) {
    await response.body?.cancel().catch(() => undefined)
    throw error
  }
  const reader = response.body?.getReader()
  if (!reader) throw new Error('下载响应没有可读取的响应体')

  const partPath = `${destPath}.part`
  cleanupFile(partPath)
  let downloaded = 0
  let lastReport = 0
  let file: FileHandle | undefined
  let completed = false
  let bodyFinished = false
  const sha256 = createHash('sha256')

  const onAbort = (): void => {
    void reader.cancel(createAbortError()).catch(() => undefined)
  }
  signal?.addEventListener('abort', onAbort, { once: true })
  try {
    file = await open(partPath, 'wx')
    for (;;) {
      throwIfAborted(signal)
      const { done, value } = await readWithIdleTimeout(reader, idleTimeoutMs)
      throwIfAborted(signal)
      if (done) {
        bodyFinished = true
        break
      }
      const chunk = Buffer.from(value)
      if (downloaded > maxBytes - chunk.byteLength) {
        throw new Error(`下载内容超过 ${formatBytes(maxBytes)} 上限`)
      }
      await writeAll(file, chunk)
      sha256.update(chunk)
      downloaded += chunk.byteLength
      if (contentLength > 0 && Date.now() - lastReport > 300) {
        const pct = Math.min(95, Math.round((downloaded / contentLength) * 100))
        onProgress({
          stage: 'downloading',
          percent: pct,
          message: `${stageLabel} (${formatBytes(downloaded)}/${formatBytes(contentLength)})`
        })
        lastReport = Date.now()
      }
    }
    if (contentLength > 0 && downloaded !== contentLength) {
      throw new Error(`下载内容不完整: 预期 ${contentLength} 字节，实际 ${downloaded} 字节`)
    }
    const actualSha256 = sha256.digest('hex')
    if (actualSha256 !== expectedSha256) {
      throw new DownloadIntegrityError(expectedSha256, actualSha256)
    }
    await file.sync()
    await file.close()
    file = undefined
    await rename(partPath, destPath)
    completed = true
  } finally {
    signal?.removeEventListener('abort', onAbort)
    if (!bodyFinished) await reader.cancel().catch(() => undefined)
    if (file) await file.close().catch(() => undefined)
    if (!completed) await unlink(partPath).catch(() => undefined)
  }
}

export async function downloadWithFallback(
  urls: Array<{ url: string; label: string }>,
  destPath: string,
  onProgress: ProgressCallback,
  expectedSha256: string,
  signal?: AbortSignal
): Promise<void> {
  let lastError: Error | null = null
  for (const { url, label } of urls) {
    throwIfAborted(signal)
    try {
      await downloadWithProgress(url, destPath, onProgress, label, {
        expectedSha256,
        signal
      })
      return
    } catch (err) {
      if (signal?.aborted) throw createAbortError()
      lastError = err as Error
    }
  }
  throw lastError ?? new Error('所有下载源均失败')
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`
}

export function toolDir(installRoot: string, toolId: string): string {
  return `${installRoot}\\${toolId}`
}

export function versionDir(installRoot: string, toolId: string, version: string): string {
  return `${toolDir(installRoot, toolId)}\\${version}`
}

export function currentLink(installRoot: string, toolId: string): string {
  return `${toolDir(installRoot, toolId)}\\current`
}

export async function extractZip(
  zipPath: string,
  destDir: string,
  signal?: AbortSignal
): Promise<void> {
  const powershellExecutable = process.env.SystemRoot
    ? join(process.env.SystemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe')
    : 'powershell.exe'
  await runProcess(
    powershellExecutable,
    ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', EXPAND_ARCHIVE_SCRIPT],
    {
      timeoutMs: 300_000,
      signal,
      env: {
        ...process.env,
        OPENBOX_ARCHIVE_PATH: zipPath,
        OPENBOX_ARCHIVE_DEST: destDir
      }
    }
  )
}

export async function createJunction(link: string, target: string): Promise<void> {
  await removeJunction(link)
  symlinkSync(target, link, 'junction')
}

export async function removeJunction(link: string): Promise<void> {
  let stats: ReturnType<typeof lstatSync>
  try {
    stats = lstatSync(link)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return
    throw error
  }
  if (!stats.isSymbolicLink()) {
    throw new Error(`拒绝删除非 junction 路径: ${link}`)
  }
  unlinkSync(link)
}

export function cleanupFile(filePath: string): void {
  try {
    if (existsSync(filePath)) unlinkSync(filePath)
  } catch {
    // ignore
  }
}

export function findTopDir(extractDir: string): string {
  const entries: string[] = readdirSync(extractDir)
  const singleDir = entries.find((e: string) => {
    try {
      const stat = statSync(`${extractDir}\\${e}`)
      return stat.isDirectory()
    } catch {
      return false
    }
  })
  return singleDir ? `${extractDir}\\${singleDir}` : extractDir
}

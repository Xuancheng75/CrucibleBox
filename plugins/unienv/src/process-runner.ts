import { spawn } from 'node:child_process'
import type { ChildProcess, SpawnOptions } from 'node:child_process'
import type { Readable } from 'node:stream'

const DEFAULT_TIMEOUT_MS = 30_000
const DEFAULT_OUTPUT_LIMIT_BYTES = 1024 * 1024
const WINDOWS_TERMINATION_TIMEOUT_MS = 2_000

export type ProcessFailureKind =
  'spawn' | 'exit' | 'timeout' | 'aborted' | 'stdout-limit' | 'stderr-limit'

export interface ProcessResult {
  stdout: string
  stderr: string
  exitCode: number
  signal: NodeJS.Signals | null
}

export interface RunProcessOptions {
  cwd?: string
  env?: NodeJS.ProcessEnv
  timeoutMs?: number
  signal?: AbortSignal
  maxStdoutBytes?: number
  maxStderrBytes?: number
}

export type ProcessChild = ChildProcess & {
  stdout: Readable
  stderr: Readable
}

export interface ProcessTerminationContext {
  platform: NodeJS.Platform
  reason: ProcessFailureKind
}

export type ProcessTreeTerminator = (
  child: ProcessChild,
  context: ProcessTerminationContext
) => void | Promise<void>

export type SpawnProcess = (
  executable: string,
  args: readonly string[],
  options: SpawnOptions
) => ProcessChild

export interface ProcessRunnerDependencies {
  spawnProcess: SpawnProcess
  terminateProcessTree: ProcessTreeTerminator
  platform: NodeJS.Platform
}

export class ProcessExecutionError extends Error {
  readonly kind: ProcessFailureKind
  readonly executable: string
  readonly args: readonly string[]
  readonly stdout: string
  readonly stderr: string
  readonly exitCode: number | null
  readonly signal: NodeJS.Signals | null

  constructor(
    kind: ProcessFailureKind,
    message: string,
    details: {
      executable: string
      args: readonly string[]
      stdout?: string
      stderr?: string
      exitCode?: number | null
      signal?: NodeJS.Signals | null
    }
  ) {
    super(message)
    this.name = 'ProcessExecutionError'
    this.kind = kind
    this.executable = details.executable
    this.args = [...details.args]
    this.stdout = details.stdout ?? ''
    this.stderr = details.stderr ?? ''
    this.exitCode = details.exitCode ?? null
    this.signal = details.signal ?? null
  }
}

function assertSafeToken(value: string, label: string): void {
  if (value.length === 0 || value.includes('\0') || value.includes('\r') || value.includes('\n')) {
    throw new TypeError(`${label} 不能为空或包含 NUL/换行符`)
  }
}

function assertPositiveSafeInteger(value: number, label: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new TypeError(`${label} 必须是正安全整数`)
  }
}

function nodeSpawnProcess(
  executable: string,
  args: readonly string[],
  options: SpawnOptions
): ProcessChild {
  return spawn(executable, [...args], options) as ProcessChild
}

export const defaultProcessTreeTerminator: ProcessTreeTerminator = (child, context) => {
  if (context.platform === 'win32' && child.pid) {
    return new Promise<void>((resolve) => {
      let settled = false
      let killer: ChildProcess | undefined

      const finish = (): void => {
        if (settled) return
        settled = true
        clearTimeout(fallbackTimer)
        killer?.removeListener('close', onClose)
        killer?.removeListener('error', onError)
        resolve()
      }

      const fallbackToDirectKill = (): void => {
        try {
          killer?.kill('SIGKILL')
        } catch {
          // The taskkill helper may already have exited.
        }
        try {
          child.kill('SIGKILL')
        } catch {
          // The child may have exited between the liveness check and fallback.
        } finally {
          finish()
        }
      }

      const onClose = (code: number | null): void => {
        if (code !== 0 && child.exitCode === null && child.signalCode === null) {
          fallbackToDirectKill()
          return
        }
        finish()
      }

      const onError = (): void => fallbackToDirectKill()

      const fallbackTimer = setTimeout(fallbackToDirectKill, WINDOWS_TERMINATION_TIMEOUT_MS)
      fallbackTimer.unref()

      try {
        killer = spawn('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], {
          shell: false,
          windowsHide: true,
          stdio: 'ignore'
        })
        killer.once('close', onClose)
        killer.once('error', onError)
      } catch {
        fallbackToDirectKill()
        return
      }
    })
  }

  if (child.pid) {
    try {
      process.kill(-child.pid, 'SIGTERM')
      const hardKill = setTimeout(() => {
        if (child.exitCode === null && child.signalCode === null) {
          try {
            process.kill(-child.pid!, 'SIGKILL')
          } catch {
            child.kill('SIGKILL')
          }
        }
      }, 2_000)
      hardKill.unref()
      return
    } catch {
      // The process may not own a process group; fall back to the direct child.
    }
  }
  child.kill('SIGTERM')
}

const defaultDependencies: ProcessRunnerDependencies = {
  spawnProcess: nodeSpawnProcess,
  terminateProcessTree: defaultProcessTreeTerminator,
  platform: process.platform
}

function appendChunk(
  chunks: Buffer[],
  chunk: Buffer | string,
  bytesSeen: number,
  limit: number
): { bytesSeen: number; exceeded: boolean } {
  const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)
  const boundedBytesSeen = Math.min(bytesSeen, limit)
  const remaining = limit - boundedBytesSeen
  if (remaining > 0) chunks.push(buffer.subarray(0, remaining))
  const exceeded = buffer.byteLength > remaining
  const nextBytesSeen = exceeded ? limit : boundedBytesSeen + buffer.byteLength
  return { bytesSeen: nextBytesSeen, exceeded }
}

export function createProcessRunner(
  overrides: Partial<ProcessRunnerDependencies> = {}
): (
  executable: string,
  args?: readonly string[],
  options?: RunProcessOptions
) => Promise<ProcessResult> {
  const dependencies = { ...defaultDependencies, ...overrides }

  return async (
    executable: string,
    args: readonly string[] = [],
    options: RunProcessOptions = {}
  ): Promise<ProcessResult> => {
    assertSafeToken(executable, 'executable')
    args.forEach((arg, index) => assertSafeToken(arg, `args[${index}]`))

    const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS
    const maxStdoutBytes = options.maxStdoutBytes ?? DEFAULT_OUTPUT_LIMIT_BYTES
    const maxStderrBytes = options.maxStderrBytes ?? DEFAULT_OUTPUT_LIMIT_BYTES
    assertPositiveSafeInteger(timeoutMs, 'timeoutMs')
    assertPositiveSafeInteger(maxStdoutBytes, 'maxStdoutBytes')
    assertPositiveSafeInteger(maxStderrBytes, 'maxStderrBytes')

    if (options.signal?.aborted) {
      throw new ProcessExecutionError('aborted', '进程在启动前已取消', {
        executable,
        args
      })
    }

    let child: ProcessChild
    try {
      child = dependencies.spawnProcess(executable, args, {
        cwd: options.cwd,
        env: options.env,
        shell: false,
        windowsHide: true,
        detached: dependencies.platform !== 'win32',
        stdio: ['ignore', 'pipe', 'pipe']
      })
    } catch (error) {
      throw new ProcessExecutionError('spawn', `无法启动进程: ${String(error)}`, {
        executable,
        args
      })
    }

    return await new Promise<ProcessResult>((resolve, reject) => {
      const stdoutChunks: Buffer[] = []
      const stderrChunks: Buffer[] = []
      let stdoutBytes = 0
      let stderrBytes = 0
      let settled = false
      let terminationRequested = false

      const getStdout = (): string => Buffer.concat(stdoutChunks).toString('utf8')
      const getStderr = (): string => Buffer.concat(stderrChunks).toString('utf8')

      const cleanup = (): void => {
        clearTimeout(timeout)
        options.signal?.removeEventListener('abort', onAbort)
        child.stdout.removeListener('data', onStdout)
        child.stderr.removeListener('data', onStderr)
        child.removeListener('error', onError)
        child.removeListener('close', onClose)
      }

      const settleError = (error: ProcessExecutionError): void => {
        if (settled) return
        settled = true
        cleanup()
        reject(error)
      }

      const failAndTerminate = (kind: ProcessFailureKind, message: string): void => {
        if (settled || terminationRequested) return
        terminationRequested = true
        const error = new ProcessExecutionError(kind, message, {
          executable,
          args,
          stdout: getStdout(),
          stderr: getStderr(),
          exitCode: child.exitCode,
          signal: child.signalCode
        })
        void Promise.resolve(
          dependencies.terminateProcessTree(child, {
            platform: dependencies.platform,
            reason: kind
          })
        ).then(
          () => settleError(error),
          (terminationError) => {
            settleError(
              new ProcessExecutionError(
                kind,
                `${message}; 终止进程树失败: ${String(terminationError)}`,
                {
                  executable,
                  args,
                  stdout: getStdout(),
                  stderr: getStderr(),
                  exitCode: child.exitCode,
                  signal: child.signalCode
                }
              )
            )
          }
        )
      }

      const onStdout = (chunk: Buffer | string): void => {
        const result = appendChunk(stdoutChunks, chunk, stdoutBytes, maxStdoutBytes)
        stdoutBytes = result.bytesSeen
        if (result.exceeded) {
          failAndTerminate('stdout-limit', `stdout 超过 ${maxStdoutBytes} 字节上限`)
        }
      }

      const onStderr = (chunk: Buffer | string): void => {
        const result = appendChunk(stderrChunks, chunk, stderrBytes, maxStderrBytes)
        stderrBytes = result.bytesSeen
        if (result.exceeded) {
          failAndTerminate('stderr-limit', `stderr 超过 ${maxStderrBytes} 字节上限`)
        }
      }

      const onAbort = (): void => {
        failAndTerminate('aborted', '进程已取消')
      }

      const onError = (error: Error): void => {
        if (settled || terminationRequested) return
        settleError(
          new ProcessExecutionError('spawn', `进程启动失败: ${error.message}`, {
            executable,
            args,
            stdout: getStdout(),
            stderr: getStderr()
          })
        )
      }

      const onClose = (code: number | null, signal: NodeJS.Signals | null): void => {
        if (settled || terminationRequested) return
        const result: ProcessResult = {
          stdout: getStdout(),
          stderr: getStderr(),
          exitCode: code ?? -1,
          signal
        }
        if (code === 0) {
          settled = true
          cleanup()
          resolve(result)
          return
        }
        settleError(
          new ProcessExecutionError('exit', `进程退出码为 ${code ?? 'null'}`, {
            executable,
            args,
            ...result
          })
        )
      }

      const timeout = setTimeout(() => {
        failAndTerminate('timeout', `进程运行超过 ${timeoutMs}ms`)
      }, timeoutMs)
      timeout.unref()
      child.stdout.on('data', onStdout)
      child.stderr.on('data', onStderr)
      child.once('error', onError)
      child.once('close', onClose)
      options.signal?.addEventListener('abort', onAbort, { once: true })
      if (options.signal?.aborted) onAbort()
    })
  }
}

export const runProcess = createProcessRunner()

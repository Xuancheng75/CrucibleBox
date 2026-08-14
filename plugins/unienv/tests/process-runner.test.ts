import { EventEmitter } from 'node:events'
import type { SpawnOptions } from 'node:child_process'
import { PassThrough } from 'node:stream'
import { describe, expect, it, vi } from 'vitest'
import {
  ProcessExecutionError,
  createProcessRunner,
  type ProcessChild,
  type ProcessTreeTerminator,
  type SpawnProcess
} from '../../../plugin-system/trusted-services/unienv/process-runner'

class FakeChild extends EventEmitter {
  readonly pid = 4242
  readonly stdout = new PassThrough()
  readonly stderr = new PassThrough()
  exitCode: number | null = null
  signalCode: NodeJS.Signals | null = null
  readonly kill = vi.fn(() => true)

  close(code: number | null, signal: NodeJS.Signals | null = null): void {
    this.exitCode = code
    this.signalCode = signal
    this.emit('close', code, signal)
  }
}

function deferred(): { promise: Promise<void>; resolve(): void } {
  let resolve!: () => void
  const promise = new Promise<void>((resolve_) => {
    resolve = resolve_
  })
  return { promise, resolve }
}

function createHarness(terminateProcessTree?: ProcessTreeTerminator): {
  child: FakeChild
  runner: ReturnType<typeof createProcessRunner>
  spawnProcess: ReturnType<typeof vi.fn<SpawnProcess>>
  terminateProcessTree: ProcessTreeTerminator
} {
  const child = new FakeChild()
  const spawnProcess = vi.fn<SpawnProcess>(
    (_executable: string, _args: readonly string[], _options: SpawnOptions) =>
      child as unknown as ProcessChild
  )
  const terminator = terminateProcessTree ?? vi.fn(async () => undefined)
  return {
    child,
    runner: createProcessRunner({
      spawnProcess,
      terminateProcessTree: terminator,
      platform: 'win32'
    }),
    spawnProcess,
    terminateProcessTree: terminator
  }
}

async function flushMicrotasks(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

describe('process runner', () => {
  it('passes the executable and literal argv with shell disabled', async () => {
    const { child, runner, spawnProcess } = createHarness()
    const resultPromise = runner('tool.exe', ['space value', '& literal'], {
      cwd: 'C:\\work',
      timeoutMs: 1_000
    })

    child.stdout.write('hello')
    child.stderr.write('warning')
    child.close(0)

    await expect(resultPromise).resolves.toEqual({
      stdout: 'hello',
      stderr: 'warning',
      exitCode: 0,
      signal: null
    })
    expect(spawnProcess).toHaveBeenCalledWith(
      'tool.exe',
      ['space value', '& literal'],
      expect.objectContaining({
        cwd: 'C:\\work',
        detached: false,
        shell: false,
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe']
      })
    )
  })

  it('returns a structured failure for a non-zero exit', async () => {
    const { child, runner } = createHarness()
    const resultPromise = runner('tool.exe', ['--check'])
    child.stderr.write('bad input')
    child.close(7)

    await expect(resultPromise).rejects.toMatchObject({
      name: 'ProcessExecutionError',
      kind: 'exit',
      exitCode: 7,
      stderr: 'bad input'
    })
  })

  it('terminates and rejects on timeout', async () => {
    vi.useFakeTimers()
    try {
      const terminator = vi.fn(async () => undefined)
      const { child, runner } = createHarness(terminator)
      const resultPromise = runner('slow.exe', [], { timeoutMs: 50 })
      const assertion = expect(resultPromise).rejects.toMatchObject({ kind: 'timeout' })

      await vi.advanceTimersByTimeAsync(50)
      await assertion
      expect(terminator).toHaveBeenCalledWith(child, {
        platform: 'win32',
        reason: 'timeout'
      })
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not spawn when the signal is already aborted', async () => {
    const { runner, spawnProcess } = createHarness()
    const controller = new AbortController()
    controller.abort()

    await expect(runner('tool.exe', [], { signal: controller.signal })).rejects.toMatchObject({
      kind: 'aborted'
    })
    expect(spawnProcess).not.toHaveBeenCalled()
  })

  it('terminates a running process when its signal aborts', async () => {
    const terminator = vi.fn(async () => undefined)
    const { child, runner } = createHarness(terminator)
    const controller = new AbortController()
    const resultPromise = runner('tool.exe', [], { signal: controller.signal })

    controller.abort()

    await expect(resultPromise).rejects.toMatchObject({ kind: 'aborted' })
    expect(terminator).toHaveBeenCalledWith(child, {
      platform: 'win32',
      reason: 'aborted'
    })
  })

  it.each([['stdout', 'stdout-limit'] as const, ['stderr', 'stderr-limit'] as const])(
    'bounds %s before terminating the process',
    async (streamName, kind) => {
      const terminator = vi.fn(async () => undefined)
      const { child, runner } = createHarness(terminator)
      const resultPromise = runner('noisy.exe', [], {
        maxStdoutBytes: 4,
        maxStderrBytes: 4
      })
      const assertion = expect(resultPromise).rejects.toMatchObject({
        kind,
        [streamName]: '1234'
      })

      child[streamName].write('12345')

      await assertion
      expect(terminator).toHaveBeenCalledOnce()
    }
  )

  it('waits for asynchronous tree termination before rejecting', async () => {
    const termination = deferred()
    const terminateProcessTree = vi.fn(() => termination.promise)
    const { runner } = createHarness(terminateProcessTree)
    const controller = new AbortController()
    const resultPromise = runner('tool.exe', [], { signal: controller.signal })
    const rejectionObserved = vi.fn()
    void resultPromise.catch(rejectionObserved)

    controller.abort()
    await flushMicrotasks()
    expect(rejectionObserved).not.toHaveBeenCalled()

    termination.resolve()
    await expect(resultPromise).rejects.toMatchObject({ kind: 'aborted' })
    expect(rejectionObserved).toHaveBeenCalledOnce()
  })

  it('rejects control characters before spawning', async () => {
    const { runner, spawnProcess } = createHarness()

    await expect(runner('tool.exe\nmalicious')).rejects.toBeInstanceOf(TypeError)
    await expect(runner('tool.exe', ['safe', 'bad\0arg'])).rejects.toBeInstanceOf(TypeError)
    expect(spawnProcess).not.toHaveBeenCalled()
  })

  it('preserves the command context on spawn failure', async () => {
    const spawnProcess = vi.fn<SpawnProcess>(() => {
      throw new Error('missing executable')
    })
    const runner = createProcessRunner({
      spawnProcess,
      terminateProcessTree: vi.fn(),
      platform: 'win32'
    })

    const failure = runner('missing.exe', ['--version'])
    await expect(failure).rejects.toBeInstanceOf(ProcessExecutionError)
    await expect(failure).rejects.toMatchObject({
      kind: 'spawn',
      executable: 'missing.exe',
      args: ['--version']
    })
  })
})

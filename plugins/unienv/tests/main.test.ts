import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

interface InstallCall {
  toolId: string
  version: string
  installRoot: string
  signal?: AbortSignal
}

interface MockState {
  calls: InstallCall[]
  install: (
    toolId: string,
    version: string,
    installRoot: string,
    onProgress: (progress: { stage: string; percent: number; message: string }) => void,
    options?: { signal?: AbortSignal }
  ) => Promise<void>
}

const mocked = vi.hoisted(() => {
  const versions: Record<string, string[]> = {
    python: ['3.11.9'],
    node: ['20.15.1'],
    git: ['2.46.0'],
    go: ['1.23.0'],
    java: ['21.0.3']
  }
  const state: MockState = {
    calls: [],
    install: async () => undefined
  }
  const createTool = (toolId: string) => ({
    id: toolId,
    displayName: toolId.toUpperCase(),
    icon: toolId,
    description: `${toolId} tool`,
    detect: vi.fn(async () => ({ installed: false })),
    listVersions: vi.fn(async () => [...versions[toolId]]),
    install: vi.fn(
      async (
        version: string,
        installRoot: string,
        onProgress: (progress: { stage: string; percent: number; message: string }) => void,
        options?: { signal?: AbortSignal }
      ) => {
        state.calls.push({ toolId, version, installRoot, signal: options?.signal })
        await state.install(toolId, version, installRoot, onProgress, options)
      }
    ),
    uninstall: vi.fn(async () => undefined),
    switchVersion: vi.fn(async () => undefined)
  })
  return {
    state,
    pythonTool: createTool('python'),
    nodeTool: createTool('node'),
    gitTool: createTool('git'),
    goTool: createTool('go'),
    javaTool: createTool('java')
  }
})

vi.mock('../../../plugin-system/trusted-services/unienv/tools/python', () => ({
  pythonTool: mocked.pythonTool
}))
vi.mock('../../../plugin-system/trusted-services/unienv/tools/node', () => ({
  nodeTool: mocked.nodeTool
}))
vi.mock('../../../plugin-system/trusted-services/unienv/tools/git', () => ({
  gitTool: mocked.gitTool
}))
vi.mock('../../../plugin-system/trusted-services/unienv/tools/go', () => ({
  goTool: mocked.goTool
}))
vi.mock('../../../plugin-system/trusted-services/unienv/tools/java', () => ({
  javaTool: mocked.javaTool
}))

import plugin from '../../../plugin-system/trusted-services/unienv/trusted-service'

interface ResponseRecord {
  [key: string]: unknown
}

const logger = {
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  debug: vi.fn()
}

const context = {
  id: 'unienv',
  config: {
    installRoot: 'c:/Developer Tools/UniEnv/',
    downloadMirror: 'direct',
    customCombos: '[]'
  } as Record<string, unknown>,
  logger,
  database: {
    query: vi.fn(async () => []),
    execute: vi.fn(async () => undefined)
  },
  api: {} as never
}

async function send(message: Record<string, unknown>): Promise<ResponseRecord> {
  return (await plugin.onMessage(message)) as ResponseRecord
}

async function waitForTerminalTask(taskId: string): Promise<ResponseRecord> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const snapshot = await send({ type: 'getTask', taskId })
    if (['succeeded', 'failed', 'cancelled'].includes(String(snapshot.status))) return snapshot
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
  throw new Error(`Task did not settle: ${taskId}`)
}

describe('UniEnv backend task protocol', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocked.state.calls = []
    mocked.state.install = async () => undefined
    context.config = {
      installRoot: 'c:/Developer Tools/UniEnv/',
      downloadMirror: 'direct',
      customCombos: '[]'
    }
    plugin.activate(context)
  })

  afterEach(async () => {
    plugin.deactivate()
    await Promise.resolve()
  })

  it('validates versions before starting work and canonicalizes the install root', async () => {
    expect(await send({ type: 'install', tool: 'python', version: '3.11.9; calc' })).toMatchObject({
      code: 'invalid-value'
    })
    expect(mocked.state.calls).toHaveLength(0)

    const started = await send({ type: 'install', tool: 'python', version: '3.11.9' })
    expect(started).toMatchObject({ success: true, message: '安装任务已创建' })
    const completed = await waitForTerminalTask(String(started.taskId))

    expect(completed).toMatchObject({
      status: 'succeeded',
      result: { kind: 'install', tool: 'python', version: '3.11.9' }
    })
    expect(mocked.state.calls[0]).toMatchObject({
      toolId: 'python',
      version: '3.11.9',
      installRoot: 'C:\\Developer Tools\\UniEnv'
    })
  })

  it('reports progress, rejects a concurrent mutation and cancels by task id', async () => {
    mocked.state.install = async (_toolId, _version, _root, onProgress, options) => {
      onProgress({ stage: 'downloading', percent: 25, message: 'downloading' })
      await new Promise<void>((_resolve, reject) => {
        options?.signal?.addEventListener('abort', () => reject(options.signal?.reason), {
          once: true
        })
      })
    }

    const started = await send({ type: 'install', tool: 'node', version: '20.15.1' })
    const taskId = String(started.taskId)
    await Promise.resolve()
    expect(await send({ type: 'getTask', taskId })).toMatchObject({
      taskId,
      status: 'running',
      progress: { percent: 25 }
    })
    expect(await send({ type: 'install', tool: 'git', version: '2.46.0' })).toMatchObject({
      code: 'task-conflict'
    })

    expect(await send({ type: 'cancelTask', taskId })).toEqual({ success: true, taskId })
    const cancelled = await send({ type: 'getTask', taskId })
    expect(cancelled).toMatchObject({
      status: 'cancelled',
      error: { name: 'AbortError', message: '用户取消了任务' }
    })
    expect((cancelled.error as Record<string, unknown>).stack).toBeUndefined()
    expect(mocked.state.calls[0].signal?.aborted).toBe(true)
  })

  it('runs a combo as one task and preserves per-tool failures in its result', async () => {
    mocked.state.install = async (toolId, _version, _root, onProgress) => {
      onProgress({ stage: 'installing', percent: 50, message: toolId })
      if (toolId === 'node') throw new Error('node installer failed')
    }

    const started = await send({ type: 'installCombo', comboId: 'frontend-dev' })
    const completed = await waitForTerminalTask(String(started.taskId))

    expect(mocked.state.calls.map((call) => call.toolId)).toEqual(['node', 'git'])
    expect(completed).toMatchObject({
      status: 'succeeded',
      progress: { stage: 'done', percent: 100 },
      result: {
        kind: 'combo',
        comboId: 'frontend-dev',
        success: false,
        results: [
          { tool: 'NODE', success: false, message: 'node installer failed' },
          { tool: 'GIT', success: true }
        ]
      }
    })
  })

  it('rejects unsafe configuration but still allows task lookup and cancellation messages', async () => {
    context.config.installRoot = 'C:\\'
    expect(await send({ type: 'detect', tool: 'python' })).toMatchObject({ code: 'drive-root' })
    expect(await send({ type: 'getTask', taskId: 'missing-task' })).toEqual({
      error: '未找到指定任务',
      code: 'task-not-found'
    })
    expect(await send({ type: 'cancelTask', taskId: 'missing-task' })).toEqual({
      error: '任务不存在或已结束',
      code: 'task-not-cancellable'
    })
  })

  it('fails closed for system operations outside Windows while retaining read-only metadata', async () => {
    const platformDescriptor = Object.getOwnPropertyDescriptor(process, 'platform')
    Object.defineProperty(process, 'platform', { configurable: true, value: 'darwin' })
    try {
      expect(await send({ type: 'detect', tool: 'python' })).toMatchObject({
        error: expect.stringContaining('仅支持 Windows')
      })
      expect(await send({ type: 'install', tool: 'python', version: '3.11.9' })).toMatchObject({
        error: expect.stringContaining('仅支持 Windows')
      })
      expect(await send({ type: 'listVersions', tool: 'python' })).toEqual(['3.11.9'])
    } finally {
      if (platformDescriptor) Object.defineProperty(process, 'platform', platformDescriptor)
    }
  })

  it('returns failed task errors without exposing internal stacks', async () => {
    mocked.state.install = async () => {
      const error = new Error('installer exploded') as Error & { code: string }
      error.code = 'E_INSTALL'
      throw error
    }

    const started = await send({ type: 'install', tool: 'go', version: '1.23.0' })
    const failed = await waitForTerminalTask(String(started.taskId))
    expect(failed).toMatchObject({
      status: 'failed',
      error: { name: 'Error', message: 'installer exploded', code: 'E_INSTALL' }
    })
    expect((failed.error as Record<string, unknown>).stack).toBeUndefined()
  })
})

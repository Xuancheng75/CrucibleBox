interface TrustedServiceContext {
  id: string
  config: Record<string, unknown>
  logger: {
    info(message: string, ...args: unknown[]): void
    warn(message: string, ...args: unknown[]): void
    error(message: string, ...args: unknown[]): void
    debug(message: string, ...args: unknown[]): void
  }
}
import { getBuiltinCombos, resolveTool, type ComboPack } from './combo'
import { canonicalizeInstallRoot, safeJoinVersionDirectory } from './path-policy'
import {
  parseToolId,
  parseToolVersion,
  parseUniEnvConfig,
  parseUniEnvRequest,
  SUPPORTED_TOOL_VERSIONS,
  UniEnvProtocolError,
  type DownloadMirror,
  type ToolId,
  type ToolVersion,
  type UniEnvConfig
} from './protocol'
import {
  DuplicateResourceTaskError,
  TaskManager,
  type SerializedTaskError,
  type TaskSnapshot
} from './task-manager'
import { recoverInterruptedInstallStaging, type InstallProgress, type ToolDef } from './tools/base'
import { gitTool } from './tools/git'
import { goTool } from './tools/go'
import { javaTool } from './tools/java'
import { nodeTool } from './tools/node'
import { pythonTool } from './tools/python'

const INSTALLATION_RESOURCE = 'installation'

interface InstallTaskResult {
  kind: 'install'
  tool: ToolId
  version: ToolVersion
  message: string
}

interface ComboTaskItemResult {
  tool: string
  success: boolean
  message: string
}

interface ComboTaskResult {
  kind: 'combo'
  comboId: string
  success: boolean
  results: ComboTaskItemResult[]
  message: string
}

interface PublicTaskError {
  name: string
  message: string
  code?: string | number
}

interface ErrorResponse {
  error: string
  code?: string
}

let ctx: TrustedServiceContext
let tools = new Map<ToolId, ToolDef>()
let taskManager = new TaskManager()
let activeInlineMutation: string | undefined
let startupRecoveryError: string | undefined

function createTools(): Map<ToolId, ToolDef> {
  return new Map<ToolId, ToolDef>([
    ['python', pythonTool],
    ['node', nodeTool],
    ['git', gitTool],
    ['go', goTool],
    ['java', javaTool]
  ])
}

function getConfig(): UniEnvConfig & { installRoot: string } {
  const config = parseUniEnvConfig(ctx.config)
  return {
    ...config,
    installRoot: canonicalizeInstallRoot(config.installRoot)
  }
}

function requireWindows(): void {
  if (process.platform !== 'win32') {
    throw new Error('UniEnv 的环境检测与安装功能目前仅支持 Windows')
  }
}

function recoverInterruptedInstalls(installRoot: string): string[] {
  const versionRoots: string[] = []
  for (const tool of Object.keys(SUPPORTED_TOOL_VERSIONS) as ToolId[]) {
    for (const version of SUPPORTED_TOOL_VERSIONS[tool]) {
      versionRoots.push(safeJoinVersionDirectory(installRoot, tool, version as ToolVersion))
    }
  }
  return recoverInterruptedInstallStaging(versionRoots)
}

function assertStartupRecoveryReady(): void {
  if (startupRecoveryError) {
    throw new Error(`启动恢复未完成，拒绝修改环境: ${startupRecoveryError}`)
  }
}

function throwIfAborted(signal: AbortSignal): void {
  if (!signal.aborted) return
  const error = new Error('操作已取消')
  error.name = 'AbortError'
  throw error
}

function getTool(toolId: ToolId): ToolDef {
  return resolveTool(tools, toolId)
}

function getCombos(config: UniEnvConfig): ComboPack[] {
  return [...getBuiltinCombos(), ...config.customCombos]
}

function assertNoInlineMutation(): void {
  if (activeInlineMutation) {
    throw new Error(`另一个写操作正在执行: ${activeInlineMutation}`)
  }
}

function assertNoInstallationTask(): void {
  const activeTaskId = taskManager.getActiveTaskId(INSTALLATION_RESOURCE)
  if (activeTaskId) {
    throw new Error(`安装任务正在执行: ${activeTaskId}`)
  }
}

async function runInlineMutation<T>(label: string, operation: () => Promise<T>): Promise<T> {
  assertNoInlineMutation()
  assertNoInstallationTask()
  activeInlineMutation = label
  try {
    return await operation()
  } finally {
    activeInlineMutation = undefined
  }
}

function startInstallTask(
  toolId: ToolId,
  version: ToolVersion,
  installRoot: string,
  downloadMirror: DownloadMirror
): string {
  assertNoInlineMutation()
  safeJoinVersionDirectory(installRoot, toolId, version)
  const tool = getTool(toolId)
  const handle = taskManager.start<InstallTaskResult, InstallProgress>(
    INSTALLATION_RESOURCE,
    async (task) => {
      task.updateProgress({
        stage: 'downloading',
        percent: 0,
        message: `准备安装 ${tool.displayName} ${version}`
      })
      await tool.install(version, installRoot, (progress) => task.updateProgress(progress), {
        downloadMirror,
        signal: task.signal
      })
      throwIfAborted(task.signal)
      return {
        kind: 'install',
        tool: toolId,
        version,
        message: `${tool.displayName} ${version} 安装完成`
      }
    }
  )
  void handle.completion.then((snapshot) => logTaskCompletion(snapshot))
  return handle.taskId
}

function startComboTask(
  combo: ComboPack,
  installRoot: string,
  downloadMirror: DownloadMirror
): string {
  assertNoInlineMutation()
  for (const item of combo.items) {
    const toolId = parseToolId(item.toolId, `combo.${combo.id}.toolId`)
    const version = parseToolVersion(toolId, item.version, `combo.${combo.id}.${toolId}.version`)
    safeJoinVersionDirectory(installRoot, toolId, version)
  }

  const handle = taskManager.start<ComboTaskResult, InstallProgress>(
    INSTALLATION_RESOURCE,
    async (task) => {
      const results: ComboTaskItemResult[] = []
      for (let index = 0; index < combo.items.length; index += 1) {
        throwIfAborted(task.signal)
        const item = combo.items[index]
        const toolId = parseToolId(item.toolId, `combo.${combo.id}.toolId`)
        const version = parseToolVersion(
          toolId,
          item.version,
          `combo.${combo.id}.${toolId}.version`
        )
        const tool = getTool(toolId)
        try {
          await tool.install(
            version,
            installRoot,
            (progress) => {
              const overallPercent = Math.min(
                99,
                Math.round(((index + progress.percent / 100) / combo.items.length) * 100)
              )
              task.updateProgress({
                ...progress,
                percent: overallPercent,
                message: `${combo.name} · ${tool.displayName}: ${progress.message}`
              })
            },
            { downloadMirror, signal: task.signal }
          )
          throwIfAborted(task.signal)
          results.push({
            tool: tool.displayName,
            success: true,
            message: `${tool.displayName} ${version} 安装成功`
          })
        } catch (error) {
          if (task.signal.aborted) throw error
          const message = error instanceof Error ? error.message : String(error)
          results.push({ tool: tool.displayName, success: false, message })
        }
      }

      const success = results.every((result) => result.success)
      task.updateProgress({
        stage: 'done',
        percent: 100,
        message: success ? `${combo.name} 全部安装完成` : `${combo.name} 部分安装失败`
      })
      return {
        kind: 'combo',
        comboId: combo.id,
        success,
        results,
        message: success ? `组合包“${combo.name}”全部安装完成` : `组合包“${combo.name}”部分安装失败`
      }
    }
  )
  void handle.completion.then((snapshot) => logTaskCompletion(snapshot))
  return handle.taskId
}

function logTaskCompletion(snapshot: TaskSnapshot): void {
  if (snapshot.status === 'succeeded') {
    ctx.logger.info(`[UniEnv] 任务完成: ${snapshot.taskId}`)
    return
  }
  if (snapshot.status === 'cancelled') {
    ctx.logger.info(`[UniEnv] 任务已取消: ${snapshot.taskId}`)
    return
  }
  ctx.logger.error(
    `[UniEnv] 任务失败: ${snapshot.taskId}: ${snapshot.error?.message ?? '未知错误'}`
  )
}

function publicError(error: SerializedTaskError | undefined): PublicTaskError | undefined {
  if (!error) return undefined
  return {
    name: error.name,
    message: error.message,
    ...(error.code !== undefined ? { code: error.code } : {})
  }
}

function publicTaskSnapshot(snapshot: TaskSnapshot): Omit<TaskSnapshot, 'error'> & {
  error?: PublicTaskError
} {
  const { error, ...rest } = snapshot
  return {
    ...rest,
    ...(error ? { error: publicError(error) } : {})
  }
}

function toErrorResponse(error: unknown): ErrorResponse {
  if (error instanceof UniEnvProtocolError) {
    return { error: error.message, code: error.code }
  }
  if (error instanceof DuplicateResourceTaskError) {
    return { error: error.message, code: 'task-conflict' }
  }
  if (error instanceof Error) {
    const code = (error as Error & { code?: unknown }).code
    return {
      error: error.message,
      ...(typeof code === 'string' ? { code } : {})
    }
  }
  return { error: String(error) }
}

const plugin = {
  activate(context: TrustedServiceContext): void {
    ctx = context
    tools = createTools()
    taskManager = new TaskManager({ maxRetainedTasks: 100 })
    activeInlineMutation = undefined
    startupRecoveryError = undefined

    ctx.logger.info('[UniEnv] 插件已激活')
    try {
      const { installRoot } = getConfig()
      ctx.logger.info(`[UniEnv] 安装根目录: ${installRoot}`)
      if (process.platform === 'win32') {
        const recovered = recoverInterruptedInstalls(installRoot)
        if (recovered.length > 0) {
          ctx.logger.warn(`[UniEnv] 已清理 ${recovered.length} 个中断安装暂存目录`)
        }
      }
    } catch (error) {
      startupRecoveryError = toErrorResponse(error).error
      ctx.logger.warn(`[UniEnv] 配置或启动恢复失败: ${startupRecoveryError}`)
    }
  },

  deactivate(): void {
    for (const task of taskManager.listTasks()) {
      if (task.status === 'queued' || task.status === 'running') {
        taskManager.cancel(task.taskId, new Error('插件正在停用'))
      }
    }
    ctx.logger.info('[UniEnv] 插件已停用')
  },

  async onMessage(input: unknown): Promise<unknown> {
    try {
      const message = parseUniEnvRequest(input)

      switch (message.type) {
        case 'listTools':
          return [...tools.values()].map((tool) => ({
            id: tool.id,
            displayName: tool.displayName,
            icon: tool.icon,
            description: tool.description
          }))

        case 'detect': {
          requireWindows()
          const { installRoot } = getConfig()
          return await getTool(message.tool).detect(installRoot)
        }

        case 'listVersions':
          return await getTool(message.tool).listVersions()

        case 'install': {
          requireWindows()
          assertStartupRecoveryReady()
          const { installRoot, downloadMirror } = getConfig()
          const taskId = startInstallTask(
            message.tool,
            message.version,
            installRoot,
            downloadMirror
          )
          ctx.logger.info(`[UniEnv] 安装任务已创建: ${message.tool} ${message.version} (${taskId})`)
          return { success: true, taskId, message: '安装任务已创建' }
        }

        case 'getTask': {
          const snapshot = taskManager.getTask(message.taskId)
          return snapshot
            ? publicTaskSnapshot(snapshot)
            : { error: '未找到指定任务', code: 'task-not-found' }
        }

        case 'cancelTask': {
          const cancelled = taskManager.cancel(message.taskId, new Error('用户取消了任务'))
          return cancelled
            ? { success: true, taskId: message.taskId }
            : { error: '任务不存在或已结束', code: 'task-not-cancellable' }
        }

        case 'uninstall': {
          requireWindows()
          assertStartupRecoveryReady()
          const { installRoot } = getConfig()
          const tool = getTool(message.tool)
          return await runInlineMutation(`卸载 ${tool.displayName}`, async () => {
            await tool.uninstall(installRoot, () => undefined)
            return { success: true, message: `${tool.displayName} 已卸载` }
          })
        }

        case 'switchVersion': {
          requireWindows()
          assertStartupRecoveryReady()
          const { installRoot } = getConfig()
          safeJoinVersionDirectory(installRoot, message.tool, message.version)
          const tool = getTool(message.tool)
          return await runInlineMutation(`切换 ${tool.displayName}`, async () => {
            await tool.switchVersion(message.version, installRoot)
            return { success: true, message: `已切换到 ${tool.displayName} ${message.version}` }
          })
        }

        case 'listCombos':
          return getCombos(getConfig())

        case 'installCombo': {
          requireWindows()
          assertStartupRecoveryReady()
          const config = getConfig()
          const combo = getCombos(config).find((candidate) => candidate.id === message.comboId)
          if (!combo) return { error: `未知组合包: ${message.comboId}`, code: 'unknown-combo' }
          const taskId = startComboTask(combo, config.installRoot, config.downloadMirror)
          ctx.logger.info(`[UniEnv] 组合安装任务已创建: ${combo.id} (${taskId})`)
          return { success: true, taskId, message: '组合安装任务已创建' }
        }
      }
    } catch (error) {
      const response = toErrorResponse(error)
      ctx.logger.error(`[UniEnv] 请求失败: ${response.error}`)
      return response
    }
  }
}

export default plugin

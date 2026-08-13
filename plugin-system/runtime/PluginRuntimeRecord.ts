import type { PluginSandboxRuntime } from '../PluginSandbox'

/**
 * PluginRuntimeRecord — 单个插件的完整运行时状态容器。
 *
 * 将 PluginManager 历史上分散在多张 Map/Set 中的 per-plugin 状态收敛到单一对象：
 * - 激活/停止/停用在途操作（原 activation/stop/deactivation Promises）
 * - 沙箱实例与纯渲染器标记（原 sandboxes / rendererOnlyActivePluginIds）
 * - 资源清理函数（原 childCleanups，按后缀而非全键）
 * - 维护互斥、崩溃隔离、恢复定时器（原 maintenance/quarantined/restartTimers）
 * - 按沙箱对象去重的预期停止/错误上报标记（原 expectedStops / reportedErrors）
 *
 * 行为不变式（重构必须保持，详见 PluginManager 内注释）：
 * 1. 在途操作的身份校验清除（旧 promise settle 不得清掉新操作）；
 * 2. 停止顺序：先标记 expectedStop 再摘除 sandbox；
 * 3. 激活门禁顺序：隔离 → 停止中 → 激活中 → 纯渲染器 → 沙箱运行中；
 * 4. 恢复回调先摘除定时器再做门禁检查；
 * 5. 停用中状态对安装服务端口 / 恢复门禁 / 配置更新三处可见。
 */

export interface PluginRuntimeRecord {
  pluginId: string
  /** 无 backend 的纯渲染器插件 */
  rendererOnly: boolean
  /** 当前激活的后端沙箱（纯渲染器为 null） */
  sandbox: PluginSandboxRuntime | null
  /** per-plugin 资源清理，键为后缀（trusted-services / shortcut:<keys> / sub:<subId>） */
  cleanups: Map<string, () => void>
  /** 在途激活 */
  activationPromise: Promise<void> | null
  /** 在途停止 */
  stopPromise: Promise<void> | null
  /** 在途停用 */
  deactivationPromise: Promise<void> | null
  /** 维护互斥（升级/配置更新中） */
  maintenance: boolean
  /** 崩溃隔离标志 */
  quarantine: boolean
  /** 待执行的恢复定时器 */
  restartTimer: NodeJS.Timeout | null
  /** 主动停止的沙箱（抑制 crash 上报），按对象索引以区分新旧沙箱实例 */
  expectedStopSandboxes: WeakSet<PluginSandboxRuntime>
  /** 每个沙箱只上报一次错误 */
  reportedErrorSandboxes: WeakSet<PluginSandboxRuntime>
}

export function createPluginRuntimeRecord(pluginId: string): PluginRuntimeRecord {
  return {
    pluginId,
    rendererOnly: false,
    sandbox: null,
    cleanups: new Map(),
    activationPromise: null,
    stopPromise: null,
    deactivationPromise: null,
    maintenance: false,
    quarantine: false,
    restartTimer: null,
    expectedStopSandboxes: new WeakSet(),
    reportedErrorSandboxes: new WeakSet()
  }
}

/** 插件是否持有运行时（纯渲染器、活跃沙箱或任意在途操作） */
export function hasPluginRuntime(record: PluginRuntimeRecord): boolean {
  return (
    record.rendererOnly ||
    record.sandbox !== null ||
    record.activationPromise !== null ||
    record.stopPromise !== null
  )
}

import type { PluginTransactionPhase } from '../PluginTransactionJournal'

/**
 * 安装/升级/卸载事务的显式状态机（编排层模型）。
 *
 * 持久化 journal 刻意保留 3 个崩溃窗口相位（prepared/applied/committed）——
 * 那是启动恢复推导的正确粒度（见 PluginTransactionRecovery）。本模块把更细的
 * 编排状态建模为显式 7 态状态机，并给出「7 态 ↔ 可观测标记」的映射，作为
 * 编排与恢复之间的单一事实源，避免状态表达散落。
 *
 * 状态真源映射：
 * | 状态               | journal | 目录 artifact           | DB 行   | 内存标记                     |
 * |--------------------|---------|--------------------------|---------|------------------------------|
 * | prepared           | prepared| stage(或 target)         | -       | -                            |
 * | awaiting-confirm   | -       | stage                    | -       | preparedInstalls token(TTL)  |
 * | staged             | prepared| stage                    | -       | -                            |
 * | stopping-old       | prepared| stage + target           | 旧行    | runtime.stopRuntime 进行中  |
 * | applied            | applied | target(+backup)          | 新行    | -                            |
 * | committed          | committed| -                       | 新行    | -                            |
 * | recovery-required  | 任意    | 任意                     | 任意    | recoveryBlockedPluginNames   |
 */

export type InstallTransactionState =
  | 'prepared'
  | 'awaiting-confirmation'
  | 'staged'
  | 'stopping-old'
  | 'applied'
  | 'committed'
  | 'recovery-required'

export const INSTALL_TRANSACTION_STATES: readonly InstallTransactionState[] = [
  'prepared',
  'awaiting-confirmation',
  'staged',
  'stopping-old',
  'applied',
  'committed',
  'recovery-required'
]

/** 合法状态转移表（编排每次推进都必须命中其中一条） */
export const INSTALL_TRANSACTION_TRANSITIONS: Readonly<
  Record<InstallTransactionState, readonly InstallTransactionState[]>
> = {
  'awaiting-confirmation': ['prepared', 'staged'],
  prepared: ['staged', 'stopping-old', 'applied', 'recovery-required'],
  staged: ['prepared', 'stopping-old', 'applied', 'recovery-required'],
  'stopping-old': ['applied', 'recovery-required'],
  applied: ['committed', 'recovery-required'],
  committed: ['recovery-required'],
  'recovery-required': []
}

export function assertInstallTransactionStateTransition(
  from: InstallTransactionState,
  to: InstallTransactionState
): void {
  const allowed = INSTALL_TRANSACTION_TRANSITIONS[from]
  if (!allowed.includes(to)) {
    throw new Error(`Illegal install transaction transition: ${from} -> ${to}`)
  }
}

/** journal 3 相位是持久化子集，可直接作为显式状态使用 */
export function installTransactionStateFromPhase(
  phase: PluginTransactionPhase
): Extract<InstallTransactionState, 'prepared' | 'applied' | 'committed'> {
  return phase
}

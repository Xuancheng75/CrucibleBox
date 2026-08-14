// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
/**
 * 事务与运行时编排共享的聚合错误工具。
 *
 * 将 PluginInstallationService / PluginInstallPreparation / PluginManager
 * 三处相同的 withRollbackErrors 实现收敛到单一事实源。
 */

export function withRollbackErrors(primary: unknown, rollbackErrors: unknown[]): Error {
  const primaryError = primary instanceof Error ? primary : new Error(String(primary))
  if (rollbackErrors.length === 0) return primaryError
  return new AggregateError(
    [primaryError, ...rollbackErrors],
    `${primaryError.message}; rollback encountered ${rollbackErrors.length} additional error(s)`
  )
}

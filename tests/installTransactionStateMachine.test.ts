import { describe, expect, it } from 'vitest'
import {
  INSTALL_TRANSACTION_STATES,
  INSTALL_TRANSACTION_TRANSITIONS,
  assertInstallTransactionStateTransition,
  installTransactionStateFromPhase
} from '../plugin-system/runtime/InstallTransactionStateMachine'

describe('InstallTransactionStateMachine', () => {
  it('exposes exactly the documented 7 orchestration states', () => {
    expect(INSTALL_TRANSACTION_STATES).toEqual([
      'prepared',
      'awaiting-confirmation',
      'staged',
      'stopping-old',
      'applied',
      'committed',
      'recovery-required'
    ])
  })

  it('accepts every transition declared in the table', () => {
    for (const [from, targets] of Object.entries(INSTALL_TRANSACTION_TRANSITIONS)) {
      for (const to of targets) {
        expect(() =>
          assertInstallTransactionStateTransition(from as never, to as never)
        ).not.toThrow()
      }
    }
  })

  it('rejects transitions that are not declared', () => {
    // 不允许从已提交/终态回退到活动状态
    expect(() => assertInstallTransactionStateTransition('applied', 'stopping-old')).toThrow()
    expect(() => assertInstallTransactionStateTransition('committed', 'applied')).toThrow()
    expect(() => assertInstallTransactionStateTransition('committed', 'staged')).toThrow()
    // recovery-required 是终态
    expect(() => assertInstallTransactionStateTransition('recovery-required', 'applied')).toThrow()
    // awaiting-confirmation 只能进入 staged/prepared
    expect(() =>
      assertInstallTransactionStateTransition('awaiting-confirmation', 'committed')
    ).toThrow()
  })

  it('maps journal phases 1:1 onto the persisted subset of states', () => {
    expect(installTransactionStateFromPhase('prepared')).toBe('prepared')
    expect(installTransactionStateFromPhase('applied')).toBe('applied')
    expect(installTransactionStateFromPhase('committed')).toBe('committed')
  })

  it('documents the legal forward pipeline for each operation', () => {
    // 全新安装：prepared → staged → applied → committed
    expect(INSTALL_TRANSACTION_TRANSITIONS['prepared']).toContain('staged')
    expect(INSTALL_TRANSACTION_TRANSITIONS['staged']).toContain('applied')
    expect(INSTALL_TRANSACTION_TRANSITIONS['applied']).toContain('committed')
    // 升级：经 stopping-old 再 applied
    expect(INSTALL_TRANSACTION_TRANSITIONS['prepared']).toContain('stopping-old')
    expect(INSTALL_TRANSACTION_TRANSITIONS['stopping-old']).toContain('applied')
    // 任何活动状态都可进入 recovery-required（fail-closed）
    for (const from of ['prepared', 'staged', 'stopping-old', 'applied', 'committed'] as const) {
      expect(INSTALL_TRANSACTION_TRANSITIONS[from]).toContain('recovery-required')
    }
  })
})

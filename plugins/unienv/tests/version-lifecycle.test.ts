import { describe, expect, it } from 'vitest'
import {
  SUPPORTED_TOOL_VERSIONS,
  type ToolId
} from '../../../plugin-system/trusted-services/unienv/protocol'
import { getBuiltinCombos } from '../../../plugin-system/trusted-services/unienv/combo'
import {
  TOOL_VERSION_LIFECYCLE_AS_OF,
  formatComboLifecycleSummary,
  formatToolVersionOption,
  getPreferredToolVersion,
  getToolVersionLifecycle,
  orderToolVersionsForDisplay,
  requiresComboVersionConfirmation,
  requiresToolVersionConfirmation
} from '../../../plugin-system/trusted-services/unienv/version-lifecycle'

describe('tool version lifecycle catalog', () => {
  it('covers every supported version with an official HTTPS source', () => {
    expect(TOOL_VERSION_LIFECYCLE_AS_OF).toBe('2026-08-11')
    for (const [tool, versions] of Object.entries(SUPPORTED_TOOL_VERSIONS)) {
      for (const version of versions) {
        const lifecycle = getToolVersionLifecycle(tool as ToolId, version)
        expect(['current', 'maintained-branch', 'eol', 'legacy']).toContain(lifecycle.status)
        expect(lifecycle.note.length).toBeGreaterThan(10)
        expect(new URL(lifecycle.sourceUrl).protocol).toBe('https:')
      }
    }
  })

  it('selects the least risky compatibility default instead of the oldest entry', () => {
    expect(getPreferredToolVersion('python', SUPPORTED_TOOL_VERSIONS.python)).toBe('3.14.7')
    expect(getPreferredToolVersion('node', SUPPORTED_TOOL_VERSIONS.node)).toBe('24.18.1')
    expect(getPreferredToolVersion('git', SUPPORTED_TOOL_VERSIONS.git)).toBe('2.54.0')
    expect(getPreferredToolVersion('go', SUPPORTED_TOOL_VERSIONS.go)).toBe('1.26.5')
    expect(getPreferredToolVersion('java', SUPPORTED_TOOL_VERSIONS.java)).toBe('21.0.12')
  })

  it('orders the preferred entry first and labels risk in every option', () => {
    const ordered = orderToolVersionsForDisplay('node', SUPPORTED_TOOL_VERSIONS.node)
    expect(ordered).toEqual(['24.18.1', '22.5.1', '20.15.1', '18.20.4', '16.20.2'])
    expect(formatToolVersionOption('node', '24.18.1')).toContain('当前维护版本')
    expect(formatToolVersionOption('node', '24.18.1')).toContain('目录首选')
    expect(formatToolVersionOption('node', '20.15.1')).toContain('已停止维护')
  })

  it('summarizes combo risk without hiding EOL members', () => {
    expect(
      formatComboLifecycleSummary([
        { toolId: 'node', version: '18.20.4' },
        { toolId: 'git', version: '2.46.0' }
      ])
    ).toBe('node 18.20.4：已停止维护\ngit 2.46.0：旧版')
  })

  it('only requires the additional risk confirmation for non-current artifacts', () => {
    expect(requiresToolVersionConfirmation('node', '24.18.1')).toBe(false)
    expect(requiresToolVersionConfirmation('java', '25.0.4')).toBe(false)
    expect(requiresToolVersionConfirmation('python', '3.14.7')).toBe(false)
    expect(requiresToolVersionConfirmation('python', '3.12.5')).toBe(true)
    expect(requiresToolVersionConfirmation('node', '22.5.1')).toBe(true)
    expect(
      requiresComboVersionConfirmation([
        { toolId: 'node', version: '24.18.1' },
        { toolId: 'git', version: '2.54.0' }
      ])
    ).toBe(false)
    expect(
      requiresComboVersionConfirmation([
        { toolId: 'python', version: '3.12.5' },
        { toolId: 'node', version: '24.18.1' }
      ])
    ).toBe(true)
  })

  it('moves built-in combos to current artifacts', () => {
    const combos = getBuiltinCombos()
    expect(combos.find(({ id }) => id === 'frontend-dev')?.items).toEqual([
      { toolId: 'node', version: '24.18.1' },
      { toolId: 'git', version: '2.54.0' }
    ])
    expect(combos.find(({ id }) => id === 'java-dev')?.items).toEqual([
      { toolId: 'java', version: '21.0.12' },
      { toolId: 'git', version: '2.54.0' }
    ])
    expect(combos.find(({ id }) => id === 'fullstack-universal')?.items).toContainEqual({
      toolId: 'python',
      version: '3.14.7'
    })
  })

  it('fails closed for unmapped versions and catalogs without a preferred entry', () => {
    expect(() => getToolVersionLifecycle('node', '999.0.0')).toThrow('生命周期信息未维护')
    expect(() => getPreferredToolVersion('node', ['20.15.1'])).toThrow('缺少首选兼容版本')
  })
})

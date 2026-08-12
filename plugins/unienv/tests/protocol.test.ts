import { describe, expect, it } from 'vitest'
import {
  MAX_COMBO_ITEMS,
  MAX_CUSTOM_COMBOS,
  MAX_CUSTOM_COMBOS_JSON_LENGTH,
  MAX_PROTOCOL_STRING_LENGTH,
  SUPPORTED_MIRRORS,
  SUPPORTED_TOOL_VERSIONS,
  UniEnvProtocolError,
  parseCustomCombos,
  parseDownloadMirror,
  parseUniEnvConfig,
  parseUniEnvRequest
} from '../src/protocol'

function customCombo(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: 'team-stack',
    name: 'Team stack',
    description: 'Pinned team development environment',
    items: [
      { toolId: 'python', version: '3.11.9' },
      { toolId: 'node', version: '20.15.1' }
    ],
    ...overrides
  }
}

describe('parseUniEnvRequest', () => {
  it.each([
    [{ type: 'listTools' }, { type: 'listTools' }],
    [
      { type: 'detect', tool: 'python' },
      { type: 'detect', tool: 'python' }
    ],
    [
      { type: 'listVersions', tool: 'node' },
      { type: 'listVersions', tool: 'node' }
    ],
    [
      { type: 'install', tool: 'git', version: '2.46.0', requestId: 'request-1' },
      { type: 'install', tool: 'git', version: '2.46.0', requestId: 'request-1' }
    ],
    [
      { type: 'getTask', taskId: 'task-123' },
      { type: 'getTask', taskId: 'task-123' }
    ],
    [
      { type: 'cancelTask', taskId: 'task-123' },
      { type: 'cancelTask', taskId: 'task-123' }
    ],
    [
      { type: 'uninstall', tool: 'java' },
      { type: 'uninstall', tool: 'java' }
    ],
    [
      { type: 'switchVersion', tool: 'python', version: '3.12.5' },
      { type: 'switchVersion', tool: 'python', version: '3.12.5' }
    ],
    [{ type: 'listCombos' }, { type: 'listCombos' }],
    [
      { type: 'installCombo', comboId: 'team-stack' },
      { type: 'installCombo', comboId: 'team-stack' }
    ]
  ])('accepts a current renderer request %#', (input, expected) => {
    expect(parseUniEnvRequest(input)).toEqual(expected)
  })

  it.each([null, undefined, [], 'install', 1])('rejects non-object request %j', (input) => {
    expect(() => parseUniEnvRequest(input)).toThrowError(UniEnvProtocolError)
  })

  it('rejects unknown or null discriminants', () => {
    expect(() => parseUniEnvRequest({ type: 'execute' })).toThrowError(
      expect.objectContaining({ code: 'unknown-type', path: 'message.type' })
    )
    expect(() => parseUniEnvRequest({ type: null })).toThrowError(
      expect.objectContaining({ code: 'invalid-value', path: 'message.type' })
    )
  })

  it('rejects unknown and null tools', () => {
    expect(() => parseUniEnvRequest({ type: 'detect', tool: 'ruby' })).toThrowError(
      expect.objectContaining({ code: 'unknown-tool', path: 'message.tool' })
    )
    expect(() => parseUniEnvRequest({ type: 'detect', tool: null })).toThrowError(
      expect.objectContaining({ code: 'invalid-value', path: 'message.tool' })
    )
  })

  it('rejects unknown, cross-tool, null, and injected versions', () => {
    const invalidVersions: unknown[] = ['9.9.9', '3.11.9', null, '20.15.1 && calc.exe']
    for (const version of invalidVersions) {
      expect(() => parseUniEnvRequest({ type: 'install', tool: 'node', version })).toThrowError(
        UniEnvProtocolError
      )
    }
  })

  it('rejects overlong strings and unexpected fields', () => {
    expect(() =>
      parseUniEnvRequest({
        type: 'listTools',
        requestId: 'x'.repeat(MAX_PROTOCOL_STRING_LENGTH + 1)
      })
    ).toThrowError(expect.objectContaining({ code: 'string-limit' }))
    expect(() => parseUniEnvRequest({ type: 'listTools', mirror: 'direct' })).toThrowError(
      expect.objectContaining({ code: 'unknown-field', path: 'message.mirror' })
    )
  })

  it('applies request-id token constraints to task ids and rejects the legacy progress request', () => {
    expect(() => parseUniEnvRequest({ type: 'getTask', taskId: null })).toThrowError(
      expect.objectContaining({ code: 'invalid-value', path: 'message.taskId' })
    )
    expect(() => parseUniEnvRequest({ type: 'cancelTask', taskId: 'task id' })).toThrowError(
      expect.objectContaining({ code: 'invalid-value', path: 'message.taskId' })
    )
    expect(() =>
      parseUniEnvRequest({ type: 'getTask', taskId: 'x'.repeat(MAX_PROTOCOL_STRING_LENGTH + 1) })
    ).toThrowError(expect.objectContaining({ code: 'string-limit', path: 'message.taskId' }))
    expect(() => parseUniEnvRequest({ type: 'getProgress', tool: 'python' })).toThrowError(
      expect.objectContaining({ code: 'unknown-type', path: 'message.type' })
    )
  })
})

describe('mirror and config parsing', () => {
  it('accepts exactly the mirrors declared by the current plugin config', () => {
    for (const mirror of SUPPORTED_MIRRORS) expect(parseDownloadMirror(mirror)).toBe(mirror)
  })

  it.each([null, '', 'github', 'direct\u0000', 'x'.repeat(100)])(
    'rejects invalid mirror %j',
    (mirror) => {
      expect(() => parseDownloadMirror(mirror)).toThrowError(UniEnvProtocolError)
    }
  )

  it('parses current config defaults and a valid custom combo string', () => {
    expect(parseUniEnvConfig({})).toEqual({
      installRoot: 'C:\\UniEnv',
      downloadMirror: 'direct',
      customCombos: []
    })

    const combo = customCombo()
    expect(
      parseUniEnvConfig({
        installRoot: 'D:\\Developer Tools\\UniEnv',
        downloadMirror: 'tuna',
        customCombos: JSON.stringify([combo])
      })
    ).toEqual({
      installRoot: 'D:\\Developer Tools\\UniEnv',
      downloadMirror: 'tuna',
      customCombos: [combo]
    })
  })

  it('rejects null config and unknown config fields', () => {
    expect(() => parseUniEnvConfig(null)).toThrowError(UniEnvProtocolError)
    expect(() => parseUniEnvConfig({ installRoot: null })).toThrowError(
      expect.objectContaining({ code: 'invalid-value', path: 'config.installRoot' })
    )
    expect(() => parseUniEnvConfig({ downloadMirror: null })).toThrowError(
      expect.objectContaining({ code: 'invalid-value', path: 'config.downloadMirror' })
    )
    expect(() => parseUniEnvConfig({ customCombos: null })).toThrowError(
      expect.objectContaining({ code: 'invalid-custom-combos', path: 'customCombos' })
    )
    expect(() => parseUniEnvConfig({ executable: 'calc.exe' })).toThrowError(
      expect.objectContaining({ code: 'unknown-field', path: 'config.executable' })
    )
  })
})

describe('parseCustomCombos', () => {
  it('accepts known tool/version pairs and empty legacy values', () => {
    expect(parseCustomCombos('')).toEqual([])
    expect(parseCustomCombos('[]')).toEqual([])
    const value = customCombo()
    expect(parseCustomCombos(JSON.stringify([value]))).toEqual([value])
  })

  it('normalizes a missing legacy description to an empty string', () => {
    const value = customCombo()
    delete value.description
    expect(parseCustomCombos(JSON.stringify([value]))).toEqual([{ ...value, description: '' }])
  })

  it('keeps the protocol catalog aligned with all currently maintained versions', () => {
    expect(SUPPORTED_TOOL_VERSIONS).toEqual({
      python: ['3.8.10', '3.9.13', '3.10.11', '3.11.9', '3.12.5', '3.14.7'],
      node: ['16.20.2', '18.20.4', '20.15.1', '22.5.1', '24.18.1'],
      git: ['2.43.0', '2.44.0', '2.45.2', '2.46.0', '2.54.0'],
      go: ['1.21.6', '1.22.4', '1.23.0', '1.26.5'],
      java: ['17.0.11', '17.0.12', '17.0.20', '21.0.3', '21.0.5', '21.0.12', '22.0.1', '25.0.4']
    })
  })

  it.each([
    [null, 'non-string'],
    ['null', 'JSON null'],
    ['{}', 'object instead of array'],
    ['[null]', 'null combo'],
    [JSON.stringify([customCombo({ id: 'python-fullstack' })]), 'built-in id shadowing'],
    [
      JSON.stringify([customCombo({ items: [{ toolId: 'ruby', version: '3.3.0' }] })]),
      'unknown tool'
    ],
    [
      JSON.stringify([customCombo({ items: [{ toolId: 'python', version: '99.0.0' }] })]),
      'unknown version'
    ],
    [
      JSON.stringify([
        customCombo({
          items: [
            { toolId: 'python', version: '3.11.9' },
            { toolId: 'python', version: '3.12.5' }
          ]
        })
      ]),
      'duplicate tool'
    ],
    [JSON.stringify([customCombo(), customCombo()]), 'duplicate combo id'],
    [JSON.stringify([customCombo({ id: '../escape' })]), 'path-like combo id'],
    [
      JSON.stringify([customCombo({ description: 'x'.repeat(MAX_PROTOCOL_STRING_LENGTH + 1) })]),
      'overlong description'
    ]
  ])('rejects malicious custom combos: %s (%s)', (raw) => {
    expect(() => parseCustomCombos(raw)).toThrowError(UniEnvProtocolError)
  })

  it('rejects prototype-pollution keys at every parsed object boundary', () => {
    const raw =
      '[{"id":"team-stack","name":"Team","description":"Safe","items":' +
      '[{"toolId":"python","version":"3.11.9","__proto__":{"polluted":true}}]}]'
    expect(() => parseCustomCombos(raw)).toThrowError(
      expect.objectContaining({ code: 'unknown-field' })
    )
    expect(({} as Record<string, unknown>).polluted).toBeUndefined()
  })

  it('enforces JSON, combo-count, and item-count limits before use', () => {
    expect(() => parseCustomCombos(' '.repeat(MAX_CUSTOM_COMBOS_JSON_LENGTH + 1))).toThrowError(
      expect.objectContaining({ code: 'string-limit' })
    )
    const tooManyCombos = Array.from({ length: MAX_CUSTOM_COMBOS + 1 }, (_, index) =>
      customCombo({ id: `team-${index}` })
    )
    expect(() => parseCustomCombos(JSON.stringify(tooManyCombos))).toThrowError(
      expect.objectContaining({ code: 'invalid-custom-combos' })
    )
    const tooManyItems = Array.from({ length: MAX_COMBO_ITEMS + 1 }, () => ({
      toolId: 'python',
      version: '3.11.9'
    }))
    expect(() =>
      parseCustomCombos(JSON.stringify([customCombo({ items: tooManyItems })]))
    ).toThrowError(expect.objectContaining({ code: 'invalid-custom-combos' }))
  })
})

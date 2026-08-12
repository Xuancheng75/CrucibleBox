import { describe, expect, it } from 'vitest'
import { SUPPORTED_TOOL_VERSIONS } from '../src/protocol'
import { gitTool } from '../src/tools/git'
import { goTool } from '../src/tools/go'
import { javaTool } from '../src/tools/java'
import { nodeTool } from '../src/tools/node'
import { pythonTool } from '../src/tools/python'

describe('tool version catalogs', () => {
  it.each([
    ['python', pythonTool],
    ['node', nodeTool],
    ['git', gitTool],
    ['go', goTool],
    ['java', javaTool]
  ] as const)('returns a mutable copy of protocol versions for %s', async (toolId, tool) => {
    const versions = await tool.listVersions()

    expect(versions).toEqual(SUPPORTED_TOOL_VERSIONS[toolId])
    versions.pop()
    expect(await tool.listVersions()).toEqual(SUPPORTED_TOOL_VERSIONS[toolId])
  })
})

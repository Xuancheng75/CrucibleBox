import { describe, expect, it } from 'vitest'
import { SUPPORTED_TOOL_VERSIONS } from '../../../plugin-system/trusted-services/unienv/protocol'
import { gitTool } from '../../../plugin-system/trusted-services/unienv/tools/git'
import { goTool } from '../../../plugin-system/trusted-services/unienv/tools/go'
import { javaTool } from '../../../plugin-system/trusted-services/unienv/tools/java'
import { nodeTool } from '../../../plugin-system/trusted-services/unienv/tools/node'
import { pythonTool } from '../../../plugin-system/trusted-services/unienv/tools/python'

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

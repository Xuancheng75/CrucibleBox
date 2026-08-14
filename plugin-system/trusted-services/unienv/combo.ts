// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import type { ToolDef } from './tools/base'
import type { ToolId, ToolVersion } from './protocol'

export interface ComboItem {
  toolId: ToolId
  version: ToolVersion
}

export interface ComboPack {
  id: string
  name: string
  description: string
  items: ComboItem[]
}

export function getBuiltinCombos(): ComboPack[] {
  return [
    {
      id: 'python-fullstack',
      name: 'Python 全栈',
      description: 'Python 3.14 + Node 24 + Git',
      items: [
        { toolId: 'python', version: '3.14.7' },
        { toolId: 'node', version: '24.18.1' },
        { toolId: 'git', version: '2.54.0' }
      ]
    },
    {
      id: 'java-dev',
      name: 'Java 开发',
      description: 'JDK 21 + Git',
      items: [
        { toolId: 'java', version: '21.0.12' },
        { toolId: 'git', version: '2.54.0' }
      ]
    },
    {
      id: 'go-dev',
      name: 'Go 开发',
      description: 'Go 1.26 + Git',
      items: [
        { toolId: 'go', version: '1.26.5' },
        { toolId: 'git', version: '2.54.0' }
      ]
    },
    {
      id: 'frontend-dev',
      name: '前端开发',
      description: 'Node 24 + Git',
      items: [
        { toolId: 'node', version: '24.18.1' },
        { toolId: 'git', version: '2.54.0' }
      ]
    },
    {
      id: 'fullstack-universal',
      name: '全栈通用',
      description: 'Python 3.14 + Node 24 + Go 1.26 + Git',
      items: [
        { toolId: 'python', version: '3.14.7' },
        { toolId: 'node', version: '24.18.1' },
        { toolId: 'go', version: '1.26.5' },
        { toolId: 'git', version: '2.54.0' }
      ]
    }
  ]
}

export function resolveTool(tools: ReadonlyMap<ToolId, ToolDef>, toolId: ToolId): ToolDef {
  const tool = tools.get(toolId)
  if (!tool) {
    throw new Error(`未知工具: ${toolId}`)
  }
  return tool
}

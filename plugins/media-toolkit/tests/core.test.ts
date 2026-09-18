import { describe, expect, it } from 'vitest'
import { buildMediaProject } from '../src/renderer'

describe('图片与音视频项目', () => {
  it('只保存可恢复的非破坏编辑和帧信息', () => {
    expect(
      buildMediaProject(
        'a.png',
        [{ type: 'rotate', value: 90 }],
        [{ id: 'temporary', name: '1.png', url: 'blob:temporary', delay: 120 }]
      )
    ).toEqual({
      schema: 1,
      sourceName: 'a.png',
      edits: [{ type: 'rotate', value: 90 }],
      frames: [{ name: '1.png', delay: 120 }]
    })
  })
})

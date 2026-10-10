import { describe, expect, it } from 'vitest'
import { argsFor, availableVideoEncoder } from '../src/main'

const job = {
  input: 'C:\\媒体\\源.mp4',
  output: 'C:\\媒体\\结果.mp4',
  start: '00:00:03',
  duration: '00:00:07'
} as const

describe('audio and video process arguments', () => {
  it('writes only to the host output transaction', () => {
    const args = argsFor({ ...job, kind: 'trim' })
    expect(args).toContain('-y')
    expect(args.at(-1)).toBe('__CRUCIBLEBOX_OUTPUT__')
    expect(args).not.toContain(job.output)
    expect(args).toContain(job.input)
  })

  it('distinguishes precise and keyframe-limited trimming', () => {
    const precise = argsFor({ ...job, kind: 'trim' })
    const fast = argsFor({ ...job, kind: 'fastTrim' })
    expect(precise).toContain('libx264')
    expect(precise).not.toContain('copy')
    expect(fast).toContain('copy')
    expect(fast).not.toContain('libx264')
  })

  it('requires both input and output paths', () => {
    expect(() => argsFor({ ...job, kind: 'trim', output: ' ' })).toThrow('请选择输入和输出文件')
  })

  it('uses OpenH264 when an LGPL build omits x264', () => {
    expect(availableVideoEncoder(' V....D libopenh264 OpenH264 H.264 encoder\n')).toBe(
      'libopenh264'
    )
    const args = argsFor({ ...job, kind: 'transcode' }, 'libopenh264')
    expect(args).toContain('libopenh264')
    expect(args).toContain('yuv420p')
    expect(args).not.toContain('-crf')
  })

  it('prefers x264 when both encoders are present', () => {
    expect(availableVideoEncoder(' V....D libopenh264 OpenH264\n V....D libx264 H.264\n')).toBe(
      'libx264'
    )
  })
})

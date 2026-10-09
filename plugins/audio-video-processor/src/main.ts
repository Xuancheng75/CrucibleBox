import type { PluginContext, PluginMain } from 'cruciblebox-plugin-api'

type Job = {
  kind: 'trim' | 'fastTrim' | 'transcode' | 'audio' | 'frame'
  input: string
  output: string
  start?: string
  duration?: string
  format?: string
}
let context: PluginContext | null = null
type VideoEncoder = 'libx264' | 'libopenh264'
const encoderCache = new Map<string, VideoEncoder>()

function videoArgs(encoder: VideoEncoder, kind: 'trim' | 'transcode'): string[] {
  return encoder === 'libx264'
    ? ['-c:v', 'libx264', '-crf', kind === 'trim' ? '18' : '23']
    : ['-c:v', 'libopenh264', '-b:v', kind === 'trim' ? '4M' : '2M', '-pix_fmt', 'yuv420p']
}

export function availableVideoEncoder(encoderList: string): VideoEncoder | null {
  const encoders = encoderList.split(/\r?\n/)
  const available = (name: string) =>
    encoders.some((line) => new RegExp(`^\\s*V[^\\n]*?\\s${name}\\s`).test(line))
  return available('libx264') ? 'libx264' : available('libopenh264') ? 'libopenh264' : null
}

async function selectVideoEncoder(
  process: NonNullable<PluginContext['capabilities']['process']>,
  executable: string
): Promise<VideoEncoder> {
  const cached = encoderCache.get(executable)
  if (cached) return cached
  const result = await process.run(executable, ['-hide_banner', '-encoders'], { timeoutMs: 20_000 })
  if (result.exitCode !== 0 || result.timedOut) throw new Error('无法检查 FFmpeg 编码器')
  const selected = availableVideoEncoder(result.stdout)
  if (!selected) throw new Error('此 FFmpeg 缺少 H.264 编码器（libx264 或 libopenh264）')
  encoderCache.set(executable, selected)
  return selected
}

export function argsFor(job: Job, videoEncoder: VideoEncoder = 'libx264'): string[] {
  if (!job.input.trim() || !job.output.trim()) throw new Error('请选择输入和输出文件')
  const args = ['-hide_banner', '-nostdin', '-y']
  if (job.start?.trim()) args.push('-ss', job.start.trim())
  args.push('-i', job.input.trim())
  if (job.duration?.trim()) args.push('-t', job.duration.trim())
  if (job.kind === 'audio') args.push('-vn', '-c:a', job.format === 'mp3' ? 'libmp3lame' : 'aac')
  if (job.kind === 'frame') args.push('-frames:v', '1')
  if (job.kind === 'trim') args.push(...videoArgs(videoEncoder, 'trim'), '-c:a', 'aac')
  if (job.kind === 'fastTrim') args.push('-c', 'copy')
  if (job.kind === 'transcode') args.push(...videoArgs(videoEncoder, 'transcode'), '-c:a', 'aac')
  args.push('__CRUCIBLEBOX_OUTPUT__')
  return args
}

const plugin: PluginMain = {
  activate(ctx) {
    context = ctx
  },
  deactivate() {
    context = null
  },
  async onMessage(message) {
    if (!context) throw new Error('插件尚未启动')
    const request = message as { type?: string; executable?: string; job?: Job; taskId?: string }
    if (request.type === 'getSettings')
      return { executable: (await context.storage.get<string>('ffmpeg-path')) ?? '' }
    if (request.type === 'setExecutable') {
      encoderCache.clear()
      await context.storage.set('ffmpeg-path', request.executable?.trim() ?? '')
      return { ok: true }
    }
    if (request.type === 'start') {
      const process = context.capabilities.process
      if (!process) throw new Error('当前宿主不支持后台进程任务')
      const executable = await context.storage.get<string>('ffmpeg-path')
      if (!executable) throw new Error('请先选择本地 ffmpeg.exe')
      const job = request.job as Job
      const videoEncoder =
        job.kind === 'trim' || job.kind === 'transcode'
          ? await selectVideoEncoder(process, executable)
          : 'libx264'
      return process.start(executable, argsFor(job, videoEncoder), {
        timeoutMs: 1_800_000,
        outputTarget: job.output.trim(),
        outputValidation: 'media',
        inputPaths: [job.input.trim()]
      })
    }
    if (request.type === 'getTask')
      return context.capabilities.process?.getTask(request.taskId ?? '')
    if (request.type === 'cancel') return context.capabilities.process?.cancel(request.taskId ?? '')
    throw new Error('不支持的操作')
  }
}
export default plugin

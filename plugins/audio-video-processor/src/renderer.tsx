import { useEffect, useState } from 'react'
import type { PluginProcessTask, PluginRenderProps } from 'cruciblebox-plugin-api'

type JobKind = 'trim' | 'fastTrim' | 'transcode' | 'audio' | 'frame'
const field: React.CSSProperties = {
  width: '100%',
  boxSizing: 'border-box',
  padding: 9,
  borderRadius: 8,
  border: '1px solid var(--ob-color-border, #777)',
  background: 'var(--ob-color-bg-container, #fff)',
  color: 'var(--ob-color-text, #222)'
}

export default function AudioVideoProcessor({ api }: PluginRenderProps) {
  const [executable, setExecutable] = useState('')
  const [kind, setKind] = useState<JobKind>('trim')
  const [input, setInput] = useState('')
  const [output, setOutput] = useState('')
  const [start, setStart] = useState('')
  const [duration, setDuration] = useState('')
  const [task, setTask] = useState<PluginProcessTask | null>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    void api
      .sendToBackend({ type: 'getSettings' })
      .then((value) => setExecutable((value as { executable: string }).executable))
  }, [api])
  useEffect(() => {
    if (!task || !['queued', 'running'].includes(task.status)) return
    const timer = window.setInterval(() => {
      void api
        .sendToBackend({ type: 'getTask', taskId: task.taskId })
        .then((value) => setTask(value as PluginProcessTask))
        .catch((cause) => setError(String(cause)))
    }, 700)
    return () => window.clearInterval(timer)
  }, [api, task])

  const browse = async (target: 'input' | 'output' | 'executable') => {
    const files = await api.dialog.open({ type: 'file' })
    if (!files[0]) return
    if (target === 'input') setInput(files[0])
    if (target === 'output') setOutput(files[0])
    if (target === 'executable') {
      setExecutable(files[0])
      await api.sendToBackend({ type: 'setExecutable', executable: files[0] })
    }
  }
  const run = async () => {
    setError('')
    try {
      const result = (await api.sendToBackend({
        type: 'start',
        job: {
          kind,
          input,
          output,
          start,
          duration,
          format: output.toLowerCase().endsWith('.mp3') ? 'mp3' : 'aac'
        }
      })) as { taskId: string }
      setTask({ taskId: result.taskId, status: 'queued' })
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }
  const active = task && ['queued', 'running'].includes(task.status)
  return (
    <main style={{ padding: 20, maxWidth: 760, color: 'var(--ob-color-text, #222)' }}>
      <h2>音视频处理</h2>
      <p>
        所有文件都在本机处理。请先选择带有 ffprobe 的 FFmpeg 程序；输出先经过验证，再保存为新文件。
      </p>
      <label>
        FFmpeg 程序路径
        <input
          style={field}
          value={executable}
          onChange={(event) => setExecutable(event.target.value)}
          onBlur={() => void api.sendToBackend({ type: 'setExecutable', executable })}
          placeholder="ffmpeg.exe 的完整路径"
        />
      </label>
      <button onClick={() => void browse('executable')}>选择程序</button>
      <p>
        <label>
          处理方式{' '}
          <select value={kind} onChange={(event) => setKind(event.target.value as JobKind)}>
            <option value="trim">精确裁剪（重编码）</option>
            <option value="fastTrim">快速裁剪（关键帧限制）</option>
            <option value="transcode">转码为 H.264/AAC</option>
            <option value="audio">提取音频</option>
            <option value="frame">提取单帧</option>
          </select>
        </label>
      </p>
      <label>
        输入文件
        <input style={field} value={input} onChange={(event) => setInput(event.target.value)} />
      </label>
      <button onClick={() => void browse('input')}>选择输入</button>
      <p>
        <label>
          输出文件（填写新文件名及扩展名）
          <input
            style={field}
            value={output}
            onChange={(event) => setOutput(event.target.value)}
            placeholder="例如 D:\\视频\\片段.mp4"
          />
        </label>
      </p>
      <label>
        开始时间
        <input
          style={field}
          value={start}
          onChange={(event) => setStart(event.target.value)}
          placeholder="00:00:10"
        />
      </label>
      {kind !== 'frame' && (
        <p>
          <label>
            时长（可留空）
            <input
              style={field}
              value={duration}
              onChange={(event) => setDuration(event.target.value)}
              placeholder="00:00:30"
            />
          </label>
        </p>
      )}
      <button disabled={!!active} onClick={() => void run()}>
        开始处理
      </button>
      {active && (
        <button onClick={() => void api.sendToBackend({ type: 'cancel', taskId: task.taskId })}>
          取消
        </button>
      )}
      {task && (
        <p>
          任务：{task.status}
          {task.progress?.message ? ` · ${task.progress.message}` : ''}
          {task.result?.exitCode !== undefined ? ` · 退出码 ${task.result.exitCode}` : ''}
        </p>
      )}
      {task?.result?.outputPath && <p>已保存：{task.result.outputPath}</p>}
      {task?.result?.stderr && (
        <details>
          <summary>程序输出</summary>
          <pre style={{ whiteSpace: 'pre-wrap' }}>{task.result.stderr}</pre>
        </details>
      )}
      {task?.error && <p role="alert">{task.error.message}</p>}
      {error && <p role="alert">{error}</p>}
      <p>当前版本需自行提供 FFmpeg；批量任务、合并和便携版内置运行时尚未完成。</p>
    </main>
  )
}

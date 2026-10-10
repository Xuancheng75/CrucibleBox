import React, { useMemo, useRef, useState } from 'react'

type Edit = { type: 'rotate' | 'flip-x' | 'grayscale' | 'brightness'; value?: number }
type Frame = { id: string; name: string; url: string; delay: number }

export function buildMediaProject(sourceName: string, edits: Edit[], frames: Frame[]) {
  return {
    schema: 1,
    sourceName,
    edits,
    frames: frames.map(({ name, delay }) => ({ name, delay }))
  }
}

const tabs = ['图片工作台', '动画时间轴', '音视频工具', '批量导出'] as const

export default function MediaToolkit() {
  const [tab, setTab] = useState<(typeof tabs)[number]>('图片工作台')
  const [source, setSource] = useState<string>('')
  const [sourceName, setSourceName] = useState('')
  const [edits, setEdits] = useState<Edit[]>([])
  const [frames, setFrames] = useState<Frame[]>([])
  const [video, setVideo] = useState<string>('')
  const [outputWidth, setOutputWidth] = useState(0)
  const [watermark, setWatermark] = useState('')
  const [format, setFormat] = useState<'image/png' | 'image/jpeg' | 'image/webp'>('image/png')
  const [quality, setQuality] = useState(0.9)
  const [trimStart, setTrimStart] = useState(0)
  const [trimEnd, setTrimEnd] = useState(10)
  const [message, setMessage] = useState('')
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const videoRef = useRef<HTMLVideoElement>(null)

  const transform = useMemo(
    () =>
      edits
        .map((edit) => {
          if (edit.type === 'rotate') return `rotate(${edit.value ?? 90}deg)`
          if (edit.type === 'flip-x') return 'scaleX(-1)'
          return ''
        })
        .join(' '),
    [edits]
  )
  const filter = useMemo(
    () =>
      edits
        .map((edit) =>
          edit.type === 'grayscale'
            ? 'grayscale(1)'
            : edit.type === 'brightness'
              ? `brightness(${edit.value ?? 1})`
              : ''
        )
        .join(' '),
    [edits]
  )

  const loadImage = (file?: File) => {
    if (!file) return
    setSource(URL.createObjectURL(file))
    setSourceName(file.name)
    setEdits([])
  }
  const addFrames = (files: FileList | null) => {
    if (!files) return
    setFrames((current) => [
      ...current,
      ...Array.from(files).map((file) => ({
        id: crypto.randomUUID(),
        name: file.name,
        url: URL.createObjectURL(file),
        delay: 100
      }))
    ])
  }
  const exportImage = () => {
    if (!source) return
    const image = new Image()
    image.onload = () => {
      const canvas = canvasRef.current!
      const rotated = edits.some((edit) => edit.type === 'rotate' && (edit.value ?? 0) % 180 !== 0)
      const scale = outputWidth > 0 ? outputWidth / image.width : 1
      canvas.width = Math.round((rotated ? image.height : image.width) * scale)
      canvas.height = Math.round((rotated ? image.width : image.height) * scale)
      const ctx = canvas.getContext('2d')!
      ctx.save()
      ctx.filter = filter || 'none'
      ctx.translate(canvas.width / 2, canvas.height / 2)
      ctx.scale(scale, scale)
      for (const edit of edits) {
        if (edit.type === 'rotate') ctx.rotate(((edit.value ?? 90) * Math.PI) / 180)
        if (edit.type === 'flip-x') ctx.scale(-1, 1)
      }
      ctx.drawImage(image, -image.width / 2, -image.height / 2)
      ctx.restore()
      if (watermark.trim()) {
        ctx.font = `${Math.max(18, canvas.width / 30)}px system-ui`
        ctx.fillStyle = 'rgba(255,255,255,.82)'
        ctx.textAlign = 'right'
        ctx.fillText(watermark, canvas.width - 18, canvas.height - 18)
      }
      const extension = format.split('/')[1].replace('jpeg', 'jpg')
      const link = document.createElement('a')
      link.download = `已处理-${sourceName.replace(/\.[^.]+$/, '')}.${extension}`
      link.href = canvas.toDataURL(format, quality)
      link.click()
      setMessage(`图片已导出为 ${extension.toUpperCase()}`)
    }
    image.src = source
  }
  const captureFrame = () => {
    const element = videoRef.current
    const canvas = canvasRef.current
    if (!element || !canvas || !element.videoWidth) return
    canvas.width = element.videoWidth
    canvas.height = element.videoHeight
    canvas.getContext('2d')!.drawImage(element, 0, 0)
    const link = document.createElement('a')
    link.download = `视频帧-${Math.round(element.currentTime * 1000)}ms.png`
    link.href = canvas.toDataURL('image/png')
    link.click()
  }
  const exportClip = async () => {
    const element = videoRef.current as
      (HTMLVideoElement & { captureStream?: () => MediaStream }) | null
    if (!element?.captureStream || trimEnd <= trimStart) {
      setMessage('当前媒体或浏览器不支持片段导出')
      return
    }
    element.currentTime = trimStart
    await new Promise<void>((resolve) =>
      element.addEventListener('seeked', () => resolve(), { once: true })
    )
    const recorder = new MediaRecorder(element.captureStream(), { mimeType: 'video/webm' })
    const chunks: BlobPart[] = []
    recorder.ondataavailable = (event) => chunks.push(event.data)
    recorder.onstop = () => {
      const link = document.createElement('a')
      link.download = '媒体片段.webm'
      link.href = URL.createObjectURL(new Blob(chunks, { type: 'video/webm' }))
      link.click()
      setMessage('片段已导出')
    }
    const watcher = () => {
      if (element.currentTime >= trimEnd || element.ended) {
        element.removeEventListener('timeupdate', watcher)
        recorder.stop()
        element.pause()
      }
    }
    element.addEventListener('timeupdate', watcher)
    recorder.start()
    await element.play()
  }
  const saveProject = () => {
    const blob = new Blob([JSON.stringify(buildMediaProject(sourceName, edits, frames), null, 2)], {
      type: 'application/json'
    })
    const link = document.createElement('a')
    link.download = '媒体项目.json'
    link.href = URL.createObjectURL(blob)
    link.click()
  }
  const exportSprite = () => {
    if (!frames.length) return
    Promise.all(
      frames.map(
        (frame) =>
          new Promise<HTMLImageElement>((resolve) => {
            const image = new Image()
            image.onload = () => resolve(image)
            image.src = frame.url
          })
      )
    ).then((images) => {
      const cellWidth = Math.max(...images.map((image) => image.width))
      const cellHeight = Math.max(...images.map((image) => image.height))
      const columns = Math.ceil(Math.sqrt(images.length))
      const canvas = canvasRef.current!
      canvas.width = cellWidth * columns
      canvas.height = cellHeight * Math.ceil(images.length / columns)
      const ctx = canvas.getContext('2d')!
      images.forEach((image, index) =>
        ctx.drawImage(
          image,
          (index % columns) * cellWidth,
          Math.floor(index / columns) * cellHeight
        )
      )
      const link = document.createElement('a')
      link.download = '雪碧图.png'
      link.href = canvas.toDataURL('image/png')
      link.click()
    })
  }

  return (
    <div style={{ fontFamily: 'system-ui', padding: 18, color: '#172033' }}>
      <h2 style={{ marginTop: 0 }}>图片与音视频</h2>
      <div style={{ display: 'flex', gap: 8, marginBottom: 18 }}>
        {tabs.map((item) => (
          <button
            key={item}
            onClick={() => setTab(item)}
            style={{
              padding: '8px 14px',
              borderRadius: 8,
              border: '1px solid #ccd5e1',
              background: tab === item ? '#2563eb' : '#fff',
              color: tab === item ? '#fff' : '#172033'
            }}
          >
            {item}
          </button>
        ))}
      </div>
      {tab === '图片工作台' && (
        <section>
          <input
            type="file"
            accept="image/*"
            onChange={(event) => loadImage(event.target.files?.[0])}
          />
          <div style={{ display: 'flex', gap: 8, margin: '12px 0' }}>
            <button onClick={() => setEdits((value) => [...value, { type: 'rotate', value: 90 }])}>
              旋转 90°
            </button>
            <button onClick={() => setEdits((value) => [...value, { type: 'flip-x' }])}>
              水平翻转
            </button>
            <button onClick={() => setEdits((value) => [...value, { type: 'grayscale' }])}>
              灰度
            </button>
            <button
              onClick={() => setEdits((value) => [...value, { type: 'brightness', value: 1.2 }])}
            >
              提亮
            </button>
            <button onClick={() => setEdits((value) => value.slice(0, -1))}>撤销</button>
            <button onClick={() => setEdits([])}>还原</button>
          </div>
          <div style={{ display: 'flex', gap: 8, marginBottom: 10 }}>
            <input
              type="number"
              min="0"
              value={outputWidth}
              onChange={(event) => setOutputWidth(Number(event.target.value))}
              placeholder="输出宽度，0 为原尺寸"
            />
            <input
              value={watermark}
              onChange={(event) => setWatermark(event.target.value)}
              placeholder="文字水印"
            />
            <select
              value={format}
              onChange={(event) => setFormat(event.target.value as typeof format)}
            >
              <option value="image/png">PNG</option>
              <option value="image/jpeg">JPEG</option>
              <option value="image/webp">WebP</option>
            </select>
            <input
              type="range"
              min="0.2"
              max="1"
              step="0.05"
              value={quality}
              onChange={(event) => setQuality(Number(event.target.value))}
            />
          </div>
          {source && (
            <img src={source} style={{ maxWidth: '70%', maxHeight: 480, transform, filter }} />
          )}
          <div>
            <button onClick={exportImage} disabled={!source}>
              导出当前图片
            </button>
          </div>
        </section>
      )}
      {tab === '动画时间轴' && (
        <section>
          <input
            type="file"
            accept="image/*"
            multiple
            onChange={(event) => addFrames(event.target.files)}
          />
          <p>按顺序导入帧，调整每帧时长；项目保存后可继续编辑。</p>
          <div style={{ display: 'flex', gap: 10, overflowX: 'auto' }}>
            {frames.map((frame, index) => (
              <div
                key={frame.id}
                style={{ minWidth: 130, border: '1px solid #d7deea', padding: 8 }}
              >
                <img src={frame.url} style={{ width: 110, height: 90, objectFit: 'contain' }} />
                <div>
                  {index + 1}. {frame.name}
                </div>
                <input
                  type="number"
                  value={frame.delay}
                  onChange={(event) =>
                    setFrames((all) =>
                      all.map((item) =>
                        item.id === frame.id ? { ...item, delay: Number(event.target.value) } : item
                      )
                    )
                  }
                  style={{ width: 70 }}
                />{' '}
                ms{' '}
                <button
                  onClick={() => setFrames((all) => all.filter((item) => item.id !== frame.id))}
                >
                  删除
                </button>
              </div>
            ))}
          </div>
          <div style={{ marginTop: 10 }}>
            <button onClick={() => setFrames((all) => [...all].reverse())}>反转帧顺序</button>
            <button onClick={exportSprite}>导出雪碧图</button>
          </div>
        </section>
      )}
      {tab === '音视频工具' && (
        <section>
          <input
            type="file"
            accept="video/*,audio/*"
            onChange={(event) => {
              const file = event.target.files?.[0]
              if (file) setVideo(URL.createObjectURL(file))
            }}
          />
          {video && (
            <>
              <video
                ref={videoRef}
                src={video}
                controls
                style={{ width: '80%', maxHeight: 480, display: 'block', marginTop: 12 }}
              />
              <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                <button onClick={captureFrame}>提取当前视频帧</button>
                <label>
                  开始秒{' '}
                  <input
                    type="number"
                    value={trimStart}
                    onChange={(event) => setTrimStart(Number(event.target.value))}
                  />
                </label>
                <label>
                  结束秒{' '}
                  <input
                    type="number"
                    value={trimEnd}
                    onChange={(event) => setTrimEnd(Number(event.target.value))}
                  />
                </label>
                <button onClick={() => void exportClip()}>导出 WebM 片段</button>
              </div>
            </>
          )}
          <p>提供本地预览、定位、抽帧和轻量片段导出；复杂多轨剪辑不在本插件范围内。</p>
        </section>
      )}
      {tab === '批量导出' && (
        <section>
          <p>
            当前项目包含 {frames.length} 个动画帧、{edits.length} 个非破坏编辑步骤。
          </p>
          <button onClick={saveProject}>保存非破坏项目</button>
          <button onClick={exportImage} disabled={!source}>
            导出图片
          </button>
        </section>
      )}
      <canvas ref={canvasRef} hidden />
      {message && <p>{message}</p>}
    </div>
  )
}

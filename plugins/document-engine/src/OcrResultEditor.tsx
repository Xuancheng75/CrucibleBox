import { useEffect, useRef, useState } from 'react'
import type { OcrBlock, OcrPreview, OcrResult } from './engine-api'

export default function OcrResultEditor({
  result,
  sourcePath,
  loadPreview
}: {
  result: OcrResult
  sourcePath: string
  loadPreview: (path: string) => Promise<OcrPreview>
}) {
  const [blocks, setBlocks] = useState<OcrBlock[]>(result.blocks)
  const [preview, setPreview] = useState<OcrPreview | null>(null)
  const [previewError, setPreviewError] = useState<string | null>(null)
  const [selected, setSelected] = useState<number | null>(null)
  const editors = useRef<Array<HTMLTextAreaElement | null>>([])

  useEffect(() => {
    setBlocks(result.blocks)
    setSelected(null)
  }, [result])

  useEffect(() => {
    let active = true
    setPreview(null)
    setPreviewError(null)
    if (sourcePath) {
      void loadPreview(sourcePath).then(
        (value) => {
          if (active) setPreview(value)
        },
        (error) => {
          if (active) setPreviewError(error instanceof Error ? error.message : String(error))
        }
      )
    }
    return () => {
      active = false
    }
  }, [loadPreview, sourcePath])

  const selectBlock = (index: number) => {
    setSelected(index)
    editors.current[index]?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    editors.current[index]?.focus()
  }

  const exportResult = (format: 'txt' | 'json') => {
    const correctedText = blocks.map((block) => block.text).join('\n')
    const revisions = blocks.flatMap((block, index) =>
      block.text === result.blocks[index]?.text
        ? []
        : [
            {
              index,
              bbox: block.bbox,
              original: result.blocks[index]?.text ?? '',
              corrected: block.text
            }
          ]
    )
    const body =
      format === 'txt'
        ? correctedText
        : JSON.stringify(
            {
              ...result,
              originalText: result.text,
              text: correctedText,
              blocks: blocks.map((block, index) => ({
                ...block,
                rawText: result.blocks[index]?.text ?? ''
              })),
              revisions
            },
            null,
            2
          )
    const url = URL.createObjectURL(
      new Blob([body], { type: format === 'txt' ? 'text/plain' : 'application/json' })
    )
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = `文字识别-已校正.${format}`
    anchor.click()
    window.setTimeout(() => URL.revokeObjectURL(url), 1000)
  }

  return (
    <section style={{ marginTop: 10 }}>
      <p>单击图片中的区域定位文字；修改后导出保留原始识别和修订内容。</p>
      <div style={{ display: 'flex', gap: 8 }}>
        <button onClick={() => exportResult('txt')}>导出校正文本</button>
        <button onClick={() => exportResult('json')}>导出校正 JSON</button>
      </div>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 280px), 1fr))',
          gap: 12,
          marginTop: 10
        }}
      >
        <div
          style={{
            minWidth: 0,
            maxHeight: 420,
            overflow: 'auto',
            border: '1px solid var(--ob-color-border, #c7d9f2)',
            borderRadius: 8,
            background: 'var(--ob-color-bg-layout, #f6faff)'
          }}
        >
          {preview ? (
            <div style={{ position: 'relative', width: '100%', lineHeight: 0 }}>
              <img
                src={preview.dataUrl}
                alt="OCR 原始图片"
                style={{ display: 'block', width: '100%', height: 'auto' }}
              />
              <svg
                viewBox={`0 0 ${preview.width} ${preview.height}`}
                style={{ position: 'absolute', inset: 0, width: '100%', height: '100%' }}
                aria-label="识别区域"
              >
                {blocks.map((block, index) => {
                  const [left, top, right, bottom] = block.bbox
                  return (
                    <rect
                      key={`${index}-${block.bbox.join('-')}`}
                      x={left}
                      y={top}
                      width={Math.max(1, right - left)}
                      height={Math.max(1, bottom - top)}
                      fill={
                        selected === index ? 'var(--ob-color-primary-bg, #dfeeff)' : 'transparent'
                      }
                      stroke="var(--ob-color-primary, #2374e3)"
                      strokeWidth={selected === index ? 3 : 1.5}
                      onClick={() => selectBlock(index)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' || event.key === ' ') selectBlock(index)
                      }}
                      tabIndex={0}
                      role="button"
                      aria-label={`定位区域 ${index + 1}`}
                      style={{ cursor: 'pointer' }}
                    />
                  )
                })}
              </svg>
            </div>
          ) : (
            <p style={{ padding: 12, lineHeight: 1.5 }}>{previewError ?? '正在加载图片预览…'}</p>
          )}
        </div>
        <div style={{ maxHeight: 420, overflowY: 'auto', minWidth: 0 }}>
          {blocks.map((block, index) => (
            <label
              key={`${index}-${block.bbox.join('-')}`}
              style={{
                display: 'block',
                marginBottom: 8,
                padding: 8,
                border:
                  selected === index
                    ? '1px solid var(--ob-color-primary, #2374e3)'
                    : '1px solid var(--ob-color-border, #d4e2f5)',
                borderRadius: 6,
                background:
                  selected === index
                    ? 'var(--ob-color-primary-bg, #eaf3ff)'
                    : 'var(--ob-color-bg-container, #fff)'
              }}
            >
              <span style={{ fontSize: 12 }}>
                区域 {index + 1} · {block.type === 'formula' ? '公式' : '文字'} · 区域置信度{' '}
                {(block.confidence * 100).toFixed(1)}% · 位置 {block.bbox.join(', ')}
              </span>
              <textarea
                ref={(element) => {
                  editors.current[index] = element
                }}
                rows={2}
                value={block.text}
                onFocus={() => setSelected(index)}
                onChange={(event) =>
                  setBlocks((previous) =>
                    previous.map((item, itemIndex) =>
                      itemIndex === index ? { ...item, text: event.target.value } : item
                    )
                  )
                }
                style={{ display: 'block', width: '100%', boxSizing: 'border-box' }}
              />
            </label>
          ))}
        </div>
      </div>
    </section>
  )
}

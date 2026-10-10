import { useState } from 'react'
import type { ChunkResult } from './engine-api'

function download(name: string, content: string, mime: string) {
  const url = URL.createObjectURL(new Blob([content], { type: mime }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = name
  anchor.click()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export default function RagChunkEditor({ result }: { result: ChunkResult }) {
  const [chunks, setChunks] = useState<Array<Record<string, unknown>>>(result.chunks)
  const [selected, setSelected] = useState(0)
  const current = chunks[selected]
  const updateContent = (content: string) => setChunks(previous => previous.map((chunk, index) => index === selected ? { ...chunk, content, character_count: content.length } : chunk))
  const cleanWhitespace = () => setChunks(previous => previous.map(chunk => {
    const content = String(chunk.content ?? '').replace(/[\t ]+/g, ' ').replace(/\n{3,}/g, '\n\n').trim()
    return { ...chunk, content, character_count: content.length }
  }))
  const removeDuplicates = () => {
    const seen = new Set<string>()
    const next = chunks.filter(chunk => {
      const text = String(chunk.content ?? '').replace(/\s+/g, ' ').trim()
      if (!text || seen.has(text)) return false
      seen.add(text)
      return true
    })
    setChunks(next)
    setSelected(Math.min(selected, Math.max(0, next.length - 1)))
  }
  const exportJsonl = () => download('知识库语料-已校正.jsonl', chunks.map(chunk => JSON.stringify(chunk)).join('\n') + '\n', 'application/x-ndjson')
  const exportMarkdown = () => download('知识库语料-已校正.md', chunks.map((chunk, index) => `## ${index + 1}. ${String(chunk.section_path ?? chunk.source_file ?? '片段')}\n\n${String(chunk.content ?? '')}`).join('\n\n---\n\n'), 'text/markdown')
  return <div style={{ marginTop: 16, color: 'var(--ob-color-text, #222)' }}>
    <h4>分块预览与校正（{chunks.length} 块）</h4>
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
      <button onClick={cleanWhitespace}>清理多余空白</button>
      <button onClick={removeDuplicates}>删除重复片段</button>
      <button onClick={exportJsonl}>导出已校正 JSONL</button>
      <button onClick={exportMarkdown}>导出已校正 Markdown</button>
    </div>
    <div style={{ display: 'flex', gap: 12, marginTop: 12, maxHeight: 450 }}>
      <div style={{ width: 190, flexShrink: 0, overflowY: 'auto' }}>
        {chunks.map((chunk, index) => <button key={`${String(chunk.id ?? index)}-${index}`} onClick={() => setSelected(index)} aria-pressed={index === selected} style={{ display: 'block', width: '100%', textAlign: 'left', marginBottom: 4 }}>{index + 1}. {String(chunk.content ?? '').slice(0, 30)}</button>)}
      </div>
      {current && <div style={{ flex: 1, minWidth: 0, overflowY: 'auto' }}>
        <p style={{ marginTop: 0, overflowWrap: 'anywhere' }}>来源：{String(current.source_path ?? current.source_file ?? '未知')} · 页码：{String(current.page ?? current.start_page ?? '未知')} · 章节：{String(current.section_path ?? '未识别')}</p>
        <textarea value={String(current.content ?? '')} onChange={event => updateContent(event.target.value)} rows={12} style={{ width: '100%', boxSizing: 'border-box' }} aria-label="片段正文" />
        <button onClick={() => { setChunks(previous => previous.filter((_, index) => index !== selected)); setSelected(Math.max(0, selected - 1)) }}>删除此片段</button>
      </div>}
    </div>
  </div>
}

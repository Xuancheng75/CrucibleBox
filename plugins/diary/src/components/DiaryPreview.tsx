import React from 'react'
import ReactMarkdown from 'react-markdown'
import remarkMath from 'remark-math'
import rehypeKatex from 'rehype-katex'
import katexCss from 'katex/dist/katex.min.css'

interface DiaryPreviewProps {
  content: string
}

export default function DiaryPreview({ content }: DiaryPreviewProps) {
  return (
    <div className="diary-preview">
      <style>{katexCss}</style>
      <div className="preview-content markdown-body">
        <ReactMarkdown
          remarkPlugins={[remarkMath]}
          rehypePlugins={[rehypeKatex]}
        >
          {content}
        </ReactMarkdown>
      </div>
    </div>
  )
}

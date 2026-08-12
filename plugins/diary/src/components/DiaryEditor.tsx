import React, { useRef, useEffect } from 'react'

interface DiaryEditorProps {
  content: string
  onChange: (content: string) => void
}

export default function DiaryEditor({ content, onChange }: DiaryEditorProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    textareaRef.current?.focus()
  }, [])

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Tab') {
      e.preventDefault()
      const ta = textareaRef.current
      if (ta) {
        const start = ta.selectionStart
        const end = ta.selectionEnd
        const value = ta.value
        ta.value = value.substring(0, start) + '  ' + value.substring(end)
        ta.selectionStart = ta.selectionEnd = start + 2
        onChange(ta.value)
      }
    }
  }

  return (
    <div className="diary-editor">
      <textarea
        ref={textareaRef}
        className="editor-textarea"
        value={content}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="在此输入 Markdown 内容..."
        spellCheck={false}
        autoFocus
      />
    </div>
  )
}

import { useEffect, useRef, useState } from 'react'
import { theme } from 'antd'

export interface SearchResult {
  key: string
  label: string
  icon: React.ReactNode
  kind: string
  run: () => void
}

interface Props {
  open: boolean
  onClose: () => void
  query: string
  onQueryChange: (query: string) => void
  results: SearchResult[]
  title: string
}

export default function SearchDialog({ open, onClose, query, onQueryChange, results, title }: Props) {
  const { token } = theme.useToken()
  const [index, setIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    setIndex(0)
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0)
    return () => window.clearTimeout(timer)
  }, [open])

  if (!open) return null

  const choose = (result: SearchResult) => {
    result.run()
    onClose()
  }

  return (
    <div className="ob-search-overlay" onClick={onClose} role="presentation">
      <div className="ob-palette ob-search-dialog" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-label={title}
        style={{ background: token.colorBgElevated, borderColor: token.colorBorder, boxShadow: token.boxShadowSecondary }}>
        <input ref={inputRef} value={query} onChange={(event) => { onQueryChange(event.target.value); setIndex(0) }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') onClose()
            if (event.key === 'ArrowDown') { event.preventDefault(); setIndex((current) => Math.min(current + 1, results.length - 1)) }
            if (event.key === 'ArrowUp') { event.preventDefault(); setIndex((current) => Math.max(current - 1, 0)) }
            if (event.key === 'Enter' && results[index]) choose(results[index])
          }}
          placeholder="搜索插件或功能…" aria-label="搜索插件或功能" aria-controls="ob-search-results"
          aria-activedescendant={results[index] ? `ob-search-option-${index}` : undefined}
          style={{ color: token.colorText, borderBottomColor: token.colorBorderSecondary }} />
        <div id="ob-search-results" role="listbox" className="ob-search-results">
          {results.length === 0 ? <div className="ob-search-empty" style={{ color: token.colorTextTertiary }}>无匹配结果</div> :
            results.map((result, resultIndex) => (
              <div id={`ob-search-option-${resultIndex}`} key={result.key} role="option" aria-selected={resultIndex === index}
                onMouseEnter={() => setIndex(resultIndex)} onClick={() => choose(result)} className="ob-search-option"
                style={{ color: token.colorText, background: resultIndex === index ? token.colorPrimaryBg : 'transparent' }}>
                <span className="ob-search-option-icon" style={{ color: token.colorPrimary }}>{result.icon}</span>
                <span className="ob-search-option-label">{result.label}</span>
                <span className="ob-search-option-kind" style={{ color: token.colorTextTertiary }}>{result.kind}</span>
              </div>
            ))}
        </div>
      </div>
    </div>
  )
}

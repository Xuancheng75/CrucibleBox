import React, { useState, useEffect } from 'react'
import type { DiaryRenderProps } from './next-service'
import CalendarPage from './components/CalendarPage'
import DiaryPage from './components/DiaryPage'
import NotesPage from './components/NotesPage'
import { setApi } from './utils/db'
import diaryCss from './styles/diary.css'

export default function DiaryApp({ api }: DiaryRenderProps) {
  setApi(api)

  const [page, setPage] = useState<'calendar' | 'diary'>('calendar')
  const [section, setSection] = useState<'diary' | 'notes'>('diary')
  const [selectedDate, setSelectedDate] = useState<string>('')
  const [refreshKey, setRefreshKey] = useState(0)

  useEffect(() => {
    const styleId = 'diary-plugin-styles'
    if (!document.getElementById(styleId)) {
      const style = document.createElement('style')
      style.id = styleId
      style.textContent = diaryCss
      document.head.appendChild(style)
    }
  }, [])

  useEffect(() => {
    if (page === 'diary') {
      requestAnimationFrame(() => {
        const ta = document.querySelector<HTMLTextAreaElement>('.editor-textarea')
        if (ta) {
          ta.style.minHeight = '100px'
        }
      })
    }
  }, [page])

  const handleSelectDate = (date: string) => {
    setSelectedDate(date)
    setPage('diary')
  }

  const handleBack = () => {
    setRefreshKey((k) => k + 1)
    setPage('calendar')
  }

  return (
    <div className="diary-app">
      <nav
        style={{
          display: 'flex',
          gap: 8,
          padding: 12,
          borderBottom: '1px solid var(--ob-color-border, #ddd)'
        }}
      >
        <button
          type="button"
          onClick={() => setSection('diary')}
          aria-current={section === 'diary' ? 'page' : undefined}
        >
          日记
        </button>
        <button
          type="button"
          onClick={() => setSection('notes')}
          aria-current={section === 'notes' ? 'page' : undefined}
        >
          笔记
        </button>
      </nav>
      {section === 'notes' ? (
        <NotesPage api={api} />
      ) : page === 'calendar' ? (
        <CalendarPage key={refreshKey} onSelectDate={handleSelectDate} />
      ) : (
        <DiaryPage
          key={`${selectedDate}-${refreshKey}`}
          date={selectedDate}
          onBack={handleBack}
          onSelectDate={(d) => {
            setSelectedDate(d)
          }}
        />
      )}
    </div>
  )
}

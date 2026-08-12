import React, { useState, useEffect } from 'react'
import type { PluginRenderProps } from 'openbox-plugin-api'
import CalendarPage from './components/CalendarPage'
import DiaryPage from './components/DiaryPage'
import { setApi } from './utils/db'
import diaryCss from './styles/diary.css'

export default function DiaryApp({ api }: PluginRenderProps) {
  setApi(api)

  const [page, setPage] = useState<'calendar' | 'diary'>('calendar')
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
    setRefreshKey(k => k + 1)
    setPage('calendar')
  }

  return (
    <div className="diary-app">
      {page === 'calendar' ? (
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

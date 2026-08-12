import React, { useState, useEffect, useCallback } from 'react'
import CalendarView from './CalendarView'
import { getMonthEntries } from '../utils/db'
import type { DiaryMonthEntry } from '../utils/db'

interface CalendarPageProps {
  onSelectDate: (date: string) => void
}

export default function CalendarPage({ onSelectDate }: CalendarPageProps) {
  const now = new Date()
  const [year, setYear] = useState(now.getFullYear())
  const [month, setMonth] = useState(now.getMonth() + 1)
  const [entries, setEntries] = useState<DiaryMonthEntry[]>([])
  const [loading, setLoading] = useState(false)

  const loadEntries = useCallback(async () => {
    setLoading(true)
    try {
      const data = await getMonthEntries(year, month)
      setEntries(data)
    } catch {
      // ignore
    } finally {
      setLoading(false)
    }
  }, [year, month])

  useEffect(() => {
    loadEntries()
  }, [loadEntries])

  const goPrevMonth = () => {
    if (month === 1) {
      setYear(year - 1)
      setMonth(12)
    } else {
      setMonth(month - 1)
    }
  }

  const goNextMonth = () => {
    if (month === 12) {
      setYear(year + 1)
      setMonth(1)
    } else {
      setMonth(month + 1)
    }
  }

  const goToday = () => {
    const n = new Date()
    setYear(n.getFullYear())
    setMonth(n.getMonth() + 1)
  }

  return (
    <div className="diary-calendar-page">
      <div className="calendar-nav">
        <button className="nav-btn" onClick={goPrevMonth}>
          ←
        </button>
        <h2 className="nav-title">
          {year}年{month}月
        </h2>
        <button className="nav-btn" onClick={goNextMonth}>
          →
        </button>
        <button className="nav-btn today-btn" onClick={goToday}>
          今天
        </button>
      </div>
      {loading ? (
        <div className="calendar-loading">加载中...</div>
      ) : (
        <CalendarView
          year={year}
          month={month}
          entries={entries}
          selectedDate={null}
          onSelectDate={onSelectDate}
        />
      )}
    </div>
  )
}

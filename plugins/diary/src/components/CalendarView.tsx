import React from 'react'
import type { DiaryMonthEntry } from '../utils/db'
import { getDaysInMonth, getFirstDayOfMonth, dateToStr, todayStr } from '../utils/export'

interface CalendarViewProps {
  year: number
  month: number
  entries: DiaryMonthEntry[]
  selectedDate: string | null
  onSelectDate: (dateStr: string) => void
}

export default function CalendarView({ year, month, entries, selectedDate, onSelectDate }: CalendarViewProps) {
  const daysInMonth = getDaysInMonth(year, month)
  const firstDay = getFirstDayOfMonth(year, month)
  const today = todayStr()

  const entryDates = new Set(entries.map((e) => e.entry_date))

  const cells: (number | null)[] = []
  for (let i = 0; i < firstDay; i++) {
    cells.push(null)
  }
  for (let d = 1; d <= daysInMonth; d++) {
    cells.push(d)
  }
  while (cells.length < 42) {
    cells.push(null)
  }

  const weeks: (number | null)[][] = []
  for (let i = 0; i < cells.length; i += 7) {
    weeks.push(cells.slice(i, i + 7))
  }

  const weekDayLabels = ['一', '二', '三', '四', '五', '六', '日']

  return (
    <div className="diary-calendar">
      <div className="calendar-header">
        {weekDayLabels.map((label) => (
          <div key={label} className="calendar-weekday">
            {label}
          </div>
        ))}
      </div>
      <div className="calendar-body">
        {weeks.map((week, wi) => (
          <div key={wi} className="calendar-week">
            {week.map((day, di) => {
              if (day === null) {
                return <div key={di} className="calendar-day empty" />
              }
              const dateStr = dateToStr(new Date(year, month - 1, day))
              const isToday = dateStr === today
              const isSelected = dateStr === selectedDate
              const hasEntry = entryDates.has(dateStr)

              return (
                <div
                  key={di}
                  className={`calendar-day${isToday ? ' today' : ''}${isSelected ? ' selected' : ''}${hasEntry ? ' has-entry' : ''}`}
                  onClick={() => onSelectDate(dateStr)}
                >
                  <span className="day-number">{day}</span>
                  {hasEntry && <span className="day-dot" />}
                  {entries
                    .filter((e) => e.entry_date === dateStr)
                    .map((e) => (
                      <div key={e.entry_date} className="day-title">
                        {e.title}
                      </div>
                    ))}
                </div>
              )
            })}
          </div>
        ))}
      </div>
    </div>
  )
}

import React from 'react'
import { dateToStr } from '../utils/export'

interface WeekBarProps {
  selectedDate: string
  onSelectDate: (date: string) => void
}

export default function WeekBar({ selectedDate, onSelectDate }: WeekBarProps) {
  const date = new Date(selectedDate)
  const dayOfWeek = date.getDay()
  const mondayOffset = dayOfWeek === 0 ? -6 : 1 - dayOfWeek
  const monday = new Date(date)
  monday.setDate(date.getDate() + mondayOffset)

  const weekLabels = ['一', '二', '三', '四', '五', '六', '日']
  const days: { label: string; dateStr: string; dayNum: number; isSelected: boolean; isToday: boolean }[] = []

  for (let i = 0; i < 7; i++) {
    const d = new Date(monday)
    d.setDate(monday.getDate() + i)
    const ds = dateToStr(d)
    const today = dateToStr(new Date())
    days.push({
      label: weekLabels[i],
      dateStr: ds,
      dayNum: d.getDate(),
      isSelected: ds === selectedDate,
      isToday: ds === today
    })
  }

  return (
    <div className="week-bar">
      {days.map((day) => (
        <div
          key={day.dateStr}
          className={`week-day${day.isSelected ? ' selected' : ''}${day.isToday ? ' today' : ''}`}
          onClick={() => onSelectDate(day.dateStr)}
        >
          <span className="week-day-label">{day.label}</span>
          <span className="week-day-num">{day.dayNum}</span>
        </div>
      ))}
    </div>
  )
}

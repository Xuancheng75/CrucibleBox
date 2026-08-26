import React from 'react'
import { dateToStr } from '../utils/export'

interface WeekBarProps {
  selectedDate: string
  onSelectDate: (date: string) => void
}

function addDays(date: Date, days: number): Date {
  const next = new Date(date)
  next.setDate(next.getDate() + days)
  return next
}

/** 周一 00:00（本地时区） */
function mondayOf(date: Date): Date {
  const day = date.getDay()
  return addDays(date, day === 0 ? -6 : 1 - day)
}

/**
 * 日视图下的周条（1.9.12）：支持 ‹ › 跨周切换、周区间标题与「本周」快捷跳转。
 * 切换日期经 onSelectDate 上抛，未保存草稿由外层 tryLeave 守卫处理。
 */
export default function WeekBar({ selectedDate, onSelectDate }: WeekBarProps) {
  const selected = new Date(selectedDate)
  const monday = mondayOf(selected)

  const weekLabels = ['一', '二', '三', '四', '五', '六', '日']
  const todayStr = dateToStr(new Date())
  const days: { label: string; dateStr: string; dayNum: number; isSelected: boolean; isToday: boolean }[] = []

  for (let i = 0; i < 7; i++) {
    const d = addDays(monday, i)
    const ds = dateToStr(d)
    days.push({
      label: weekLabels[i],
      dateStr: ds,
      dayNum: d.getDate(),
      isSelected: ds === selectedDate,
      isToday: ds === todayStr
    })
  }

  const sunday = addDays(monday, 6)
  const rangeLabel = `${monday.getMonth() + 1}月${monday.getDate()}日 – ${sunday.getDate()}日`
  const currentMonday = mondayOf(new Date())
  const isCurrentWeek = dateToStr(monday) === dateToStr(currentMonday)

  const shiftWeek = (delta: number) => {
    onSelectDate(dateToStr(addDays(selected, delta * 7)))
  }

  return (
    <div className="week-bar-wrap">
      <div className="week-nav">
        <button className="nav-btn" aria-label="上一周" onClick={() => shiftWeek(-1)}>
          ←
        </button>
        <span className="week-range">{rangeLabel}</span>
        <button className="nav-btn" aria-label="下一周" onClick={() => shiftWeek(1)}>
          →
        </button>
        {!isCurrentWeek && (
          <button
            className="nav-btn"
            onClick={() => onSelectDate(dateToStr(new Date()))}
          >
            本周
          </button>
        )}
      </div>
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
    </div>
  )
}

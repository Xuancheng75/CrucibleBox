import { parseDiaryDate } from '../diary-domain'

export function downloadAsFile(content: string, filename: string) {
  const blob = new Blob([content], { type: 'text/markdown;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  document.body.appendChild(anchor)
  anchor.click()
  document.body.removeChild(anchor)
  URL.revokeObjectURL(url)
}

export function formatDate(dateStr: string): string {
  const date = parseDiaryDate(dateStr)
  if (!date) return dateStr
  const weekNames = ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六']
  return `${date.year}年${date.month}月${date.day}日 ${weekNames[date.weekday]}`
}

export function todayStr(): string {
  return dateToStr(new Date())
}

export function dateToStr(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

export function getDaysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate()
}

export function getFirstDayOfMonth(year: number, month: number): number {
  const day = new Date(year, month - 1, 1).getDay()
  return day === 0 ? 6 : day - 1
}

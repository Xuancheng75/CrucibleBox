import { useState } from 'react'
import type { CSSProperties } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import { parseDiceNotation, rollDice } from './random.js'

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  const parsed = Number(value)
  if (!Number.isFinite(parsed)) return fallback
  return Math.max(min, Math.min(max, Math.floor(parsed)))
}

interface HistoryEntry {
  id: number
  label: string
  values: number[]
  total: number
}

type RollMode = 'normal' | 'advantage' | 'disadvantage'

const QUICK_PRESETS = ['1d6', '1d20', '2d6', '4d6', '1d100']

export default function DiceRollerPlugin({ config }: PluginRenderProps) {
  const defaultSides = clampNumber(config.defaultSides, 2, 100, 6)
  const [count, setCount] = useState(2)
  const [sides, setSides] = useState(defaultSides)
  const [notation, setNotation] = useState('2d6')
  const [notationError, setNotationError] = useState<string | null>(null)
  const [mode, setMode] = useState<RollMode>('normal')
  const [result, setResult] = useState<number[]>([])
  const [history, setHistory] = useState<HistoryEntry[]>([])

  const total = result.reduce((sum, value) => sum + value, 0)

  const handleRoll = () => {
    let parsed: ReturnType<typeof parseDiceNotation>
    try {
      parsed = parseDiceNotation(notation)
      setCount(parsed.count)
      setSides(parsed.sides)
      setNotationError(null)
    } catch (error) {
      setNotationError(error instanceof Error ? error.message : String(error))
      return
    }
    const first = rollDice(parsed.count, parsed.sides)
    const second = mode === 'normal' ? null : rollDice(parsed.count, parsed.sides)
    const firstTotal = first.reduce((sum, value) => sum + value, 0)
    const secondTotal = second?.reduce((sum, value) => sum + value, 0) ?? firstTotal
    const next =
      mode === 'advantage'
        ? firstTotal >= secondTotal
          ? first
          : (second ?? first)
        : mode === 'disadvantage'
          ? firstTotal <= secondTotal
            ? first
            : (second ?? first)
          : first
    if (parsed.modifier !== 0) next.push(parsed.modifier)
    setResult(next)
    setHistory((prev) =>
      [
        {
          id: Date.now(),
          label: `${notation}${mode === 'normal' ? '' : mode === 'advantage' ? '（优势）' : '（劣势）'}`,
          values: next,
          total: next.reduce((sum, value) => sum + value, 0)
        },
        ...prev
      ].slice(0, 8)
    )
  }

  const numberInputStyle: CSSProperties = {
    width: '100%',
    padding: '8px 10px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    fontSize: 14,
    boxSizing: 'border-box',
    background: 'var(--ob-color-bg-container, #ffffff)',
    color: 'var(--ob-color-text, #1f2933)'
  }

  return (
    <div
      style={{
        minHeight: '100%',
        padding: 20,
        borderRadius: 12,
        background: 'var(--ob-color-bg-layout, #f7f9fb)',
        color: 'var(--ob-color-text, #1f2933)'
      }}
    >
      <h2 style={{ margin: '0 0 16px', fontSize: 20, color: 'var(--ob-color-text, #1f2933)' }}>
        骰子与随机数
      </h2>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))',
          gap: 12,
          marginBottom: 16
        }}
      >
        <label style={{ fontSize: 13, color: 'var(--ob-color-text-secondary, #52616b)' }}>
          骰子数量
          <input
            type="number"
            min={1}
            max={20}
            value={count}
            onChange={(event) => setCount(clampNumber(event.target.value, 1, 20, 1))}
            style={{ ...numberInputStyle, marginTop: 6 }}
          />
        </label>
        <label style={{ fontSize: 13, color: 'var(--ob-color-text-secondary, #52616b)' }}>
          骰子面数
          <input
            type="number"
            min={2}
            max={100}
            value={sides}
            onChange={(event) => setSides(clampNumber(event.target.value, 2, 100, 6))}
            style={{ ...numberInputStyle, marginTop: 6 }}
          />
        </label>
      </div>

      <label
        style={{
          display: 'block',
          marginBottom: 12,
          fontSize: 13,
          color: 'var(--ob-color-text-secondary, #52616b)'
        }}
      >
        骰子表达式
        <input
          value={notation}
          onChange={(event) => setNotation(event.target.value)}
          placeholder="例如 2d6+1"
          aria-label="骰子表达式"
          style={{ ...numberInputStyle, marginTop: 6 }}
        />
      </label>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, margin: '-2px 0 12px' }}>
        {QUICK_PRESETS.map((preset) => (
          <button
            key={preset}
            onClick={() => setNotation(preset)}
            style={{
              padding: '5px 10px',
              border: '1px solid var(--ob-color-border, #d9d9d9)',
              borderRadius: 999,
              background:
                notation === preset
                  ? 'var(--ob-color-primary-bg, #eef4ff)'
                  : 'var(--ob-color-bg-container, #fff)',
              color: 'var(--ob-color-text, #1f2933)',
              cursor: 'pointer'
            }}
          >
            {preset}
          </button>
        ))}
      </div>
      <label
        style={{
          display: 'block',
          marginBottom: 12,
          fontSize: 13,
          color: 'var(--ob-color-text-secondary, #52616b)'
        }}
      >
        投掷模式
        <select
          value={mode}
          onChange={(event) => setMode(event.target.value as RollMode)}
          style={{ ...numberInputStyle, marginTop: 6 }}
        >
          <option value="normal">标准</option>
          <option value="advantage">优势（投两组取高）</option>
          <option value="disadvantage">劣势（投两组取低）</option>
        </select>
      </label>
      {notationError && (
        <div style={{ color: 'var(--ob-color-error, #ff4d4f)', marginBottom: 10 }}>
          {notationError}
        </div>
      )}

      <button
        onClick={handleRoll}
        style={{
          width: '100%',
          padding: '12px 0',
          border: 'none',
          borderRadius: 8,
          background: 'var(--ob-color-primary, #2563eb)',
          color: '#fff',
          fontSize: 16,
          fontWeight: 700,
          cursor: 'pointer'
        }}
      >
        投掷
      </button>

      {result.length > 0 && (
        <div
          style={{
            marginTop: 18,
            padding: 16,
            background: 'var(--ob-color-bg-container, #ffffff)',
            borderRadius: 8,
            border: '1px solid var(--ob-color-border-secondary, #e6edf3)'
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              justifyContent: 'space-between',
              gap: 12
            }}
          >
            <strong style={{ fontSize: 16 }}>本次结果</strong>
            <span
              style={{ fontSize: 28, fontWeight: 800, color: 'var(--ob-color-primary, #2563eb)' }}
            >
              总计 {total}
            </span>
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 12 }}>
            {result.map((value, index) => (
              <span
                key={`${index}-${value}`}
                style={{
                  minWidth: 42,
                  padding: '8px 10px',
                  borderRadius: 8,
                  background: 'var(--ob-color-primary-bg, #eef4ff)',
                  color: 'var(--ob-color-primary, #1d4ed8)',
                  textAlign: 'center',
                  fontWeight: 700
                }}
              >
                {value}
              </span>
            ))}
          </div>
        </div>
      )}

      {history.length > 0 && (
        <div style={{ marginTop: 18 }}>
          <h3 style={{ margin: '0 0 10px', fontSize: 15, color: 'var(--ob-color-text, #1f2933)' }}>
            最近记录
          </h3>
          <div style={{ display: 'grid', gap: 8 }}>
            {history.map((entry) => (
              <div
                key={entry.id}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  gap: 12,
                  padding: '10px 12px',
                  background: 'var(--ob-color-bg-container, #ffffff)',
                  borderRadius: 8,
                  border: '1px solid var(--ob-color-border-secondary, #e6edf3)',
                  fontSize: 13
                }}
              >
                <span>
                  {entry.label} = {entry.values.join(' + ')}
                </span>
                <strong>{entry.total}</strong>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

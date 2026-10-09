import { useState } from 'react'

const presets = ['1d6', '1d20', '2d6', '4d6', '1d100']

function roll(sides: number): number {
  const sample = new Uint32Array(1)
  crypto.getRandomValues(sample)
  return 1 + Math.floor(sample[0] / 0x100000000 * sides)
}

export default function DicePanel() {
  const [notation, setNotation] = useState('2d6')
  const [result, setResult] = useState<number[]>([])
  const [history, setHistory] = useState<string[]>([])
  const [error, setError] = useState('')

  const throwDice = () => {
    const match = /^(\d{1,2})d(\d{1,3})([+-]\d{1,3})?$/i.exec(notation.trim())
    if (!match) { setError('请输入骰子表达式，例如 2d6 或 1d20+3'); return }
    const count = Number(match[1])
    const sides = Number(match[2])
    if (count < 1 || count > 50 || sides < 2 || sides > 1000) {
      setError('骰子数量须为 1–50，面数须为 2–1000')
      return
    }
    const values = Array.from({ length: count }, () => roll(sides))
    const modifier = Number(match[3] ?? 0)
    setResult(values)
    setError('')
    setHistory(old => [`${notation}: ${values.join(' + ')}${modifier ? ` ${modifier > 0 ? '+' : '-'} ${Math.abs(modifier)}` : ''} = ${values.reduce((a, b) => a + b, modifier)}`, ...old].slice(0, 30))
  }

  return <section style={{ padding: 20, color: 'var(--ob-color-text, #222)' }}>
    <h2>骰子投掷</h2>
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 14 }}>
      {presets.map(value => <button key={value} onClick={() => setNotation(value)}>{value}</button>)}
    </div>
    <label>表达式 <input value={notation} onChange={event => setNotation(event.target.value)} onKeyDown={event => { if (event.key === 'Enter') throwDice() }} /></label>
    <button onClick={throwDice}>投掷</button>
    {error && <p role="alert" style={{ color: 'var(--ob-color-error, #c00)' }}>{error}</p>}
    {result.length > 0 && <p>结果：{result.join('、')}；合计：{history[0]?.split(' = ').at(-1)}</p>}
    <h3>本次历史</h3>
    <ol>{history.map((entry, index) => <li key={`${index}-${entry}`}>{entry}</li>)}</ol>
  </section>
}

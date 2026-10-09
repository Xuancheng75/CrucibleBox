import { useState } from 'react'

type Unit = { label: string; factor: number; offset?: number }
const groups: Record<string, Record<string, Unit>> = {
  长度: { m: { label: '米', factor: 1 }, km: { label: '千米', factor: 1000 }, cm: { label: '厘米', factor: 0.01 }, mm: { label: '毫米', factor: 0.001 }, mi: { label: '英里', factor: 1609.344 }, ft: { label: '英尺', factor: 0.3048 }, in: { label: '英寸', factor: 0.0254 } },
  重量: { kg: { label: '千克', factor: 1 }, g: { label: '克', factor: 0.001 }, mg: { label: '毫克', factor: 0.000001 }, lb: { label: '磅', factor: 0.45359237 }, oz: { label: '盎司', factor: 0.028349523125 } },
  体积: { L: { label: '升', factor: 1 }, mL: { label: '毫升', factor: 0.001 }, gal: { label: '美制加仑', factor: 3.785411784 }, floz: { label: '美制液盎司', factor: 0.0295735295625 } },
  面积: { sqm: { label: '平方米', factor: 1 }, sqkm: { label: '平方千米', factor: 1000000 }, hectare: { label: '公顷', factor: 10000 }, sqft: { label: '平方英尺', factor: 0.09290304 } },
  温度: { C: { label: '摄氏度', factor: 1 }, F: { label: '华氏度', factor: 5 / 9, offset: -32 }, K: { label: '开尔文', factor: 1, offset: -273.15 } }
}

export default function UnitConverter() {
  const [group, setGroup] = useState('长度')
  const [from, setFrom] = useState('m')
  const [to, setTo] = useState('km')
  const [amount, setAmount] = useState('1')
  const units = groups[group]
  const source = units[from]
  const target = units[to]
  const parsed = Number(amount)
  const base = (parsed + (source.offset ?? 0)) * source.factor
  const converted = base / target.factor - (target.offset ?? 0)
  const chooseGroup = (next: string) => {
    const keys = Object.keys(groups[next])
    setGroup(next)
    setFrom(keys[0])
    setTo(keys[1])
  }
  return <section style={{ padding: 20, color: 'var(--ob-color-text, #222)' }}>
    <h2>单位换算</h2>
    <label>类别 <select value={group} onChange={event => chooseGroup(event.target.value)}>{Object.keys(groups).map(name => <option key={name}>{name}</option>)}</select></label>{'　'}
    <label>数值 <input type="number" value={amount} onChange={event => setAmount(event.target.value)} /></label>{'　'}
    <label>从 <select value={from} onChange={event => setFrom(event.target.value)}>{Object.entries(units).map(([key, unit]) => <option key={key} value={key}>{unit.label}</option>)}</select></label>{' → '}
    <label>到 <select value={to} onChange={event => setTo(event.target.value)}>{Object.entries(units).map(([key, unit]) => <option key={key} value={key}>{unit.label}</option>)}</select></label>
    <p style={{ fontSize: 24 }}>{Number.isFinite(converted) ? converted.toLocaleString('zh-CN', { maximumFractionDigits: 8 }) : '请输入有效数值'} {target.label}</p>
  </section>
}

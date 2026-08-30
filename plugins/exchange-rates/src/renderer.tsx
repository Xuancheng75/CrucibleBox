import { useState, useEffect, useCallback } from 'react'
import type { CSSProperties } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'

const CURRENCIES = ['USD', 'CNY', 'EUR', 'JPY', 'GBP', 'HKD', 'KRW', 'SGD', 'AUD', 'CAD', 'CHF', 'THB', 'NZD', 'SEK', 'NOK', 'DKK', 'INR', 'BRL', 'RUB', 'ZAR']

const CURRENCY_NAMES: Record<string, string> = {
  USD: '美元', CNY: '人民币', EUR: '欧元', JPY: '日元', GBP: '英镑',
  HKD: '港币', KRW: '韩元', SGD: '新加坡元', AUD: '澳元', CAD: '加元',
  CHF: '瑞士法郎', THB: '泰铢', NZD: '新西兰元', SEK: '瑞典克朗',
  NOK: '挪威克朗', DKK: '丹麦克朗', INR: '印度卢比', BRL: '巴西雷亚尔',
  RUB: '俄罗斯卢布', ZAR: '南非兰特'
}

const s: Record<string, CSSProperties> = {
  page: {
    minHeight: '100%',
    padding: 20,
    background: 'var(--ob-color-bg-layout, #f7f9fb)',
    color: 'var(--ob-color-text, #1f2933)',
    fontFamily: 'var(--ob-font-family, system-ui, sans-serif)'
  },
  card: {
    padding: 16,
    background: 'var(--ob-color-bg-container, #fff)',
    borderRadius: 10,
    border: '1px solid var(--ob-color-border-secondary, #e6edf3)',
    marginBottom: 16
  },
  converterRow: {
    display: 'flex',
    gap: 10,
    alignItems: 'flex-end',
    flexWrap: 'wrap' as const
  },
  field: {
    display: 'flex',
    flexDirection: 'column' as const,
    gap: 4
  },
  label: {
    fontSize: 12,
    color: 'var(--ob-color-text-secondary, #666)'
  },
  input: {
    padding: '8px 12px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    fontSize: 14,
    width: 120,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)'
  },
  select: {
    padding: '8px 12px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    fontSize: 13,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)'
  },
  btn: {
    padding: '8px 18px',
    border: 'none',
    borderRadius: 6,
    background: 'var(--ob-color-primary, #2563eb)',
    color: '#fff',
    fontSize: 13,
    fontWeight: 600,
    cursor: 'pointer'
  },
  btnSecondary: {
    padding: '8px 18px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)',
    fontSize: 13,
    cursor: 'pointer'
  },
  result: {
    marginTop: 14,
    padding: 14,
    background: 'var(--ob-color-primary-bg, #eef4ff)',
    borderRadius: 8,
    fontSize: 22,
    fontWeight: 700,
    color: 'var(--ob-color-primary, #1d4ed8)'
  },
  rateList: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))',
    gap: 8
  },
  rateItem: {
    display: 'flex',
    justifyContent: 'space-between',
    padding: '8px 12px',
    background: 'var(--ob-color-bg-container, #fff)',
    borderRadius: 6,
    border: '1px solid var(--ob-color-border-secondary, #f0f0f0)',
    fontSize: 13
  },
  meta: {
    fontSize: 11,
    color: 'var(--ob-color-text-secondary, #999)',
    marginTop: 8
  },
  loading: {
    textAlign: 'center' as const,
    padding: 40,
    color: 'var(--ob-color-text-secondary, #999)'
  },
  error: {
    padding: 12,
    background: '#fff2f0',
    border: '1px solid #ffccc7',
    borderRadius: 6,
    color: '#cf1322',
    fontSize: 13,
    marginBottom: 12
  }
}

export default function ExchangeRatesPlugin({ api }: PluginRenderProps) {
  const [rates, setRates] = useState<Record<string, number> | null>(null)
  const [updatedAt, setUpdatedAt] = useState(0)
  const [cached, setCached] = useState(false)
  const [provider, setProvider] = useState('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')

  const [amount, setAmount] = useState('1')
  const [from, setFrom] = useState('USD')
  const [to, setTo] = useState('CNY')
  const [convertResult, setConvertResult] = useState<{ result: number; rate: number } | null>(null)

  const fetchRates = useCallback(async () => {
    setLoading(true)
    setError('')
    const res = await api.sendToBackend({ type: 'getRates' }) as {
      rates?: Record<string, number>; updatedAt?: number; cached?: boolean; provider?: string; error?: string
    }
    if (res.error) {
      setError(res.error)
    } else if (res.rates) {
      setRates(res.rates)
      setUpdatedAt(res.updatedAt || 0)
      setCached(res.cached || false)
      setProvider(res.provider || '')
    }
    setLoading(false)
  }, [api])

  useEffect(() => {
    fetchRates()
  }, [fetchRates])

  const handleConvert = async () => {
    const num = parseFloat(amount)
    if (isNaN(num) || num <= 0) return
    const res = await api.sendToBackend({ type: 'convert', amount: num, from, to }) as {
      result?: number; rate?: number; error?: string
    }
    if (res.error) {
      setError(res.error)
    } else if (res.result !== undefined) {
      setConvertResult({ result: res.result, rate: res.rate || 0 })
    }
  }

  const handleRefresh = async () => {
    setLoading(true)
    setError('')
    const res = await api.sendToBackend({ type: 'refresh' }) as {
      rates?: Record<string, number>; updatedAt?: number; provider?: string; error?: string
    }
    if (res.error) {
      setError(res.error)
    } else if (res.rates) {
      setRates(res.rates)
      setUpdatedAt(res.updatedAt || 0)
      setCached(false)
      setProvider(res.provider || '')
    }
    setLoading(false)
  }

  const displayRates = rates
    ? Object.entries(rates)
        .filter(([code]) => CURRENCIES.includes(code) && code !== 'USD')
        .sort(([a], [b]) => CURRENCIES.indexOf(a) - CURRENCIES.indexOf(b))
    : []

  return (
    <div style={s.page}>
      <h2 style={{ margin: '0 0 16px', fontSize: 20 }}>实时汇率</h2>

      {error && <div style={s.error}>{error}</div>}

      <div style={s.card}>
        <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 12 }}>货币转换</div>
        <div style={s.converterRow}>
          <div style={s.field}>
            <label style={s.label}>金额</label>
            <input
              type="number"
              style={s.input}
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              min="0"
              step="any"
            />
          </div>
          <div style={s.field}>
            <label style={s.label}>从</label>
            <select style={s.select} value={from} onChange={(e) => setFrom(e.target.value)}>
              {CURRENCIES.map((c) => <option key={c} value={c}>{c} {CURRENCY_NAMES[c] || ''}</option>)}
            </select>
          </div>
          <div style={{ fontSize: 18, padding: '8px 0' }}>→</div>
          <div style={s.field}>
            <label style={s.label}>到</label>
            <select style={s.select} value={to} onChange={(e) => setTo(e.target.value)}>
              {CURRENCIES.map((c) => <option key={c} value={c}>{c} {CURRENCY_NAMES[c] || ''}</option>)}
            </select>
          </div>
          <button style={s.btn} onClick={handleConvert}>转换</button>
        </div>
        {convertResult && (
          <div style={s.result}>
            {parseFloat(amount).toFixed(2)} {from} = {convertResult.result.toFixed(4)} {to}
            <div style={{ fontSize: 12, fontWeight: 400, color: '#666', marginTop: 4 }}>
              汇率: 1 {from} = {convertResult.rate.toFixed(6)} {to}
            </div>
          </div>
        )}
      </div>

      <div style={s.card}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
          <div style={{ fontSize: 14, fontWeight: 600 }}>汇率列表 (基准: USD)</div>
          <button style={s.btnSecondary} onClick={handleRefresh} disabled={loading}>
            {loading ? '刷新中...' : '刷新'}
          </button>
        </div>

        {loading && !rates && <div style={s.loading}>正在获取汇率...</div>}

        {rates && (
          <>
            <div style={s.rateList}>
              {displayRates.map(([code, rate]) => (
                <div key={code} style={s.rateItem}>
                  <span>{CURRENCY_NAMES[code] || code} ({code})</span>
                  <span style={{ fontWeight: 600 }}>{rate.toFixed(4)}</span>
                </div>
              ))}
            </div>
            <div style={s.meta}>
              更新时间: {updatedAt ? new Date(updatedAt).toLocaleString('zh-CN') : '未知'}
              {cached && ' · 缓存'}{provider && ` · ${provider}`}
            </div>
          </>
        )}
      </div>
    </div>
  )
}

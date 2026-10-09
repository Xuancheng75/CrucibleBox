import { useState } from 'react'
import YAML from 'yaml'
import JSON5 from 'json5'
import parseToml from '@iarna/toml/parse-string.js'
import stringifyToml from '@iarna/toml/stringify.js'
import { XMLBuilder, XMLParser } from 'fast-xml-parser'
import Papa from 'papaparse'

type Format = 'json' | 'json5' | 'yaml' | 'toml' | 'xml' | 'csv' | 'tsv'
const formats: Format[] = ['json', 'json5', 'yaml', 'toml', 'xml', 'csv', 'tsv']

export function parseStructuredData(input: string, format: Format): unknown {
  switch (format) {
    case 'json': return JSON.parse(input)
    case 'json5': return JSON5.parse(input)
    case 'yaml': return YAML.parse(input)
    case 'toml': return parseToml(input)
    case 'xml': return new XMLParser({ ignoreAttributes: false, parseTagValue: false }).parse(input)
    case 'csv':
    case 'tsv': {
      const parsed = Papa.parse<Record<string, string>>(input, { header: true, skipEmptyLines: true, delimiter: format === 'tsv' ? '\t' : ',' })
      if (parsed.errors.length) throw new Error(parsed.errors[0].message)
      return parsed.data
    }
  }
}

export function stringifyStructuredData(value: unknown, format: Format): string {
  switch (format) {
    case 'json': return JSON.stringify(value, null, 2)
    case 'json5': return JSON5.stringify(value, null, 2)
    case 'yaml': return YAML.stringify(value)
    case 'toml': return stringifyToml(value)
    case 'xml': return new XMLBuilder({ ignoreAttributes: false, format: true }).build(value)
    case 'csv':
    case 'tsv': {
      if (!Array.isArray(value)) throw new Error('CSV/TSV 输出需要对象数组')
      return Papa.unparse(value, { delimiter: format === 'tsv' ? '\t' : ',' })
    }
  }
}

export default function DataConversionTab() {
  const [source, setSource] = useState<Format>('json')
  const [target, setTarget] = useState<Format>('yaml')
  const [input, setInput] = useState('{"name":"示例","items":[1,2]}')
  const [output, setOutput] = useState('')
  const [error, setError] = useState('')
  const convert = () => {
    try {
      setOutput(stringifyStructuredData(parseStructuredData(input, source), target))
      setError('')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }
  return <div>
    <div style={{ display: 'flex', gap: 10, alignItems: 'center', marginBottom: 10 }}>
      <label>输入格式 <select value={source} onChange={event => setSource(event.target.value as Format)}>{formats.map(format => <option key={format}>{format}</option>)}</select></label>
      <span>→</span>
      <label>输出格式 <select value={target} onChange={event => setTarget(event.target.value as Format)}>{formats.map(format => <option key={format}>{format}</option>)}</select></label>
      <button onClick={convert}>解析并转换</button>
    </div>
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))', gap: 10 }}>
      <textarea aria-label="输入数据" value={input} onChange={event => setInput(event.target.value)} rows={17} style={{ width: '100%', boxSizing: 'border-box', fontFamily: 'monospace' }} />
      <textarea aria-label="转换结果" readOnly value={output} rows={17} style={{ width: '100%', boxSizing: 'border-box', fontFamily: 'monospace' }} />
    </div>
    {error && <p role="alert" style={{ color: 'var(--ob-color-error, #c00)' }}>解析失败：{error}</p>}
  </div>
}

import { useState, useEffect, useCallback } from 'react'
import type { CSSProperties } from 'react'
import type { PluginRenderProps, SystemInfo } from 'cruciblebox-plugin-api'

const s: Record<string, CSSProperties> = {
  page: {
    minHeight: '100%',
    padding: 20,
    background: 'var(--ob-color-bg-layout, #f7f9fb)',
    color: 'var(--ob-color-text, #1f2933)',
    fontFamily: 'var(--ob-font-family, system-ui, sans-serif)'
  },
  grid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))',
    gap: 16
  },
  card: {
    padding: 16,
    background: 'var(--ob-color-bg-container, #fff)',
    borderRadius: 10,
    border: '1px solid var(--ob-color-border-secondary, #e6edf3)'
  },
  cardTitle: {
    fontSize: 14,
    fontWeight: 600,
    marginBottom: 12,
    color: 'var(--ob-color-text, #1f2933)'
  },
  row: {
    display: 'flex',
    justifyContent: 'space-between',
    padding: '4px 0',
    fontSize: 13,
    borderBottom: '1px solid var(--ob-color-border-secondary, #f0f0f0)'
  },
  label: {
    color: 'var(--ob-color-text-secondary, #666)'
  },
  value: {
    fontWeight: 500,
    textAlign: 'right' as const
  },
  progress: {
    height: 8,
    borderRadius: 4,
    background: 'var(--ob-color-border-secondary, #f0f0f0)',
    overflow: 'hidden' as const,
    marginTop: 4,
    marginBottom: 8
  },
  progressFill: {
    height: '100%',
    borderRadius: 4,
    transition: 'width 0.3s ease'
  },
  refreshBtn: {
    padding: '6px 14px',
    border: '1px solid var(--ob-color-border, #d9d9d9)',
    borderRadius: 6,
    background: 'var(--ob-color-bg-container, #fff)',
    color: 'var(--ob-color-text, #1f2933)',
    fontSize: 12,
    cursor: 'pointer',
    marginBottom: 16
  },
  loading: {
    textAlign: 'center' as const,
    padding: 40,
    color: 'var(--ob-color-text-secondary, #999)'
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

function Progress({ percent, color }: { percent: number; color: string }) {
  return (
    <div style={s.progress}>
      <div style={{ ...s.progressFill, width: `${Math.min(100, percent)}%`, background: color }} />
    </div>
  )
}

function getProgressColor(percent: number): string {
  if (percent > 90) return '#ff4d4f'
  if (percent > 70) return '#faad14'
  return 'var(--ob-color-primary, #2563eb)'
}

export default function SystemInfoPlugin({ api, config }: PluginRenderProps) {
  const [info, setInfo] = useState<SystemInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [intervalSeconds, setIntervalSeconds] = useState(() => Math.max(1, Math.min(30, Number(config.refreshInterval) || 3)))
  const [samples, setSamples] = useState<Array<{ cpu: number; memory: number }>>([])
  const refreshInterval = intervalSeconds * 1000

  const refresh = useCallback(async () => {
    const data = await api.sendToBackend({ type: 'getSystemInfo' }) as SystemInfo
    setInfo(data)
    setSamples((previous) => [...previous, { cpu: data.cpu.usage, memory: data.memory.usage }].slice(-24))
    setLoading(false)
  }, [api])

  useEffect(() => {
    refresh()
    const timer = setInterval(refresh, refreshInterval)
    return () => clearInterval(timer)
  }, [refresh, refreshInterval])

  if (loading || !info) {
    return <div style={s.page}><div style={s.loading}>正在获取系统信息...</div></div>
  }

  return (
    <div style={s.page}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <h2 style={{ margin: 0, fontSize: 20 }}>系统信息面板</h2>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <label style={{ fontSize: 12, color: 'var(--ob-color-text-secondary, #666)' }}>
            采样间隔
            <select value={intervalSeconds} onChange={(event) => setIntervalSeconds(Number(event.target.value))} style={{ marginLeft: 6, padding: 5 }}>
              {[1, 3, 5, 10, 30].map((value) => <option key={value} value={value}>{value}s</option>)}
            </select>
          </label>
          <button style={s.refreshBtn} onClick={refresh}>刷新</button>
        </div>
      </div>

      {samples.length > 1 && (
        <div style={{ ...s.card, marginBottom: 16 }}>
          <div style={s.cardTitle}>最近趋势（最多 24 个采样）</div>
          <div style={{ display: 'flex', alignItems: 'end', gap: 3, height: 64 }}>
            {samples.map((sample, index) => (
              <div key={index} title={`CPU ${sample.cpu.toFixed(1)}% / 内存 ${sample.memory.toFixed(1)}%`} style={{ flex: 1, minWidth: 3, height: `${Math.max(4, Math.min(100, sample.cpu))}%`, background: 'var(--ob-color-primary, #2563eb)', borderRadius: '3px 3px 0 0' }} />
            ))}
          </div>
          <div style={{ marginTop: 8, fontSize: 11, color: 'var(--ob-color-text-secondary, #666)' }}>柱高表示 CPU 使用率，悬停查看内存</div>
        </div>
      )}

      <div style={s.grid}>
        <div style={s.card}>
          <div style={s.cardTitle}>操作系统</div>
          <div style={s.row}><span style={s.label}>系统</span><span style={s.value}>{info.os.name}</span></div>
          <div style={s.row}><span style={s.label}>版本</span><span style={s.value}>{info.os.version}</span></div>
          <div style={s.row}><span style={s.label}>主机名</span><span style={s.value}>{info.os.hostname}</span></div>
        </div>

        <div style={s.card}>
          <div style={s.cardTitle}>处理器</div>
          <div style={s.row}><span style={s.label}>型号</span><span style={{ ...s.value, fontSize: 12, maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis' }}>{info.cpu.brand || 'N/A'}</span></div>
          <div style={s.row}><span style={s.label}>逻辑核心</span><span style={s.value}>{info.cpu.cores}</span></div>
          <div style={s.row}><span style={s.label}>物理核心</span><span style={s.value}>{info.cpu.physicalCores}</span></div>
          <div style={{ marginTop: 8 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
              <span style={s.label}>使用率</span>
              <span style={{ fontWeight: 600 }}>{info.cpu.usage.toFixed(1)}%</span>
            </div>
            <Progress percent={info.cpu.usage} color={getProgressColor(info.cpu.usage)} />
          </div>
        </div>

        <div style={s.card}>
          <div style={s.cardTitle}>内存</div>
          <div style={s.row}><span style={s.label}>总计</span><span style={s.value}>{formatBytes(info.memory.total)}</span></div>
          <div style={s.row}><span style={s.label}>可用</span><span style={s.value}>{formatBytes(info.memory.available)}</span></div>
          <div style={{ marginTop: 8 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
              <span style={s.label}>使用率</span>
              <span style={{ fontWeight: 600 }}>{info.memory.usage.toFixed(1)}%</span>
            </div>
            <Progress percent={info.memory.usage} color={getProgressColor(info.memory.usage)} />
          </div>
        </div>

        <div style={s.card}>
          <div style={s.cardTitle}>磁盘</div>
          {info.disks.map((disk, i) => {
            const usedPercent = disk.total > 0 ? ((disk.total - disk.available) / disk.total) * 100 : 0
            return (
              <div key={i} style={{ marginBottom: 10 }}>
                <div style={{ fontSize: 12, color: 'var(--ob-color-text-secondary, #666)', marginBottom: 2 }}>{disk.name}</div>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
                  <span>{formatBytes(disk.total - disk.available)} / {formatBytes(disk.total)}</span>
                  <span style={{ fontWeight: 600 }}>{usedPercent.toFixed(1)}%</span>
                </div>
                <Progress percent={usedPercent} color={getProgressColor(usedPercent)} />
              </div>
            )
          })}
          {info.disks.length === 0 && <div style={{ color: '#999', fontSize: 13 }}>无磁盘信息</div>}
        </div>

        <div style={{ ...s.card, gridColumn: '1 / -1' }}>
          <div style={s.cardTitle}>网络接口</div>
          {info.network.length === 0 && <div style={{ color: '#999', fontSize: 13 }}>无网络接口</div>}
          {info.network.map((iface, i) => (
            <div key={i} style={{ ...s.row, borderBottom: i < info.network.length - 1 ? '1px solid #f0f0f0' : 'none' }}>
              <span style={{ fontWeight: 500 }}>{iface.name}</span>
              <span style={{ fontSize: 12, color: '#666' }}>
                {iface.ip && <span style={{ marginRight: 12 }}>IP: {iface.ip}</span>}
                {iface.mac && <span>MAC: {iface.mac}</span>}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

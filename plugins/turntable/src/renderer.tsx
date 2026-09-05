import { useState, useEffect, useRef, useCallback } from 'react'
import type { PluginRenderProps } from 'cruciblebox-plugin-api'
import type { TurntableItem, SpinResult } from './types'
import { secureRandomUnit, targetRotationForWinner } from './turntable-domain'

const CANVAS_SIZE = 340
const CENTER = CANVAS_SIZE / 2
const RADIUS = CENTER - 10

function easeOutQuart(t: number): number {
  return 1 - Math.pow(1 - t, 4)
}

function cssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return value || fallback
}

function drawSector(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  radius: number,
  startAngle: number,
  endAngle: number,
  color: string
): void {
  ctx.beginPath()
  ctx.moveTo(cx, cy)
  ctx.arc(cx, cy, radius, startAngle, endAngle)
  ctx.closePath()
  ctx.fillStyle = color
  ctx.fill()
  ctx.strokeStyle = cssVar('--ob-color-bg-container', '#fff')
  ctx.lineWidth = 2
  ctx.stroke()
}

function drawText(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  radius: number,
  text: unknown,
  startAngle: number,
  endAngle: number
): void {
  const midAngle = startAngle + (endAngle - startAngle) / 2
  const textRadius = radius * 0.6

  ctx.save()
  ctx.translate(cx, cy)
  ctx.rotate(midAngle)
  ctx.textAlign = 'right'
  ctx.textBaseline = 'middle'
  ctx.fillStyle = '#fff'
  ctx.font = 'bold 14px sans-serif'

  const safeText = typeof text === 'string' ? text : String(text ?? '')
  const displayText = safeText.length > 6 ? safeText.slice(0, 6) + '..' : safeText
  ctx.fillText(displayText, textRadius, 0)
  ctx.restore()
}

function drawPointer(ctx: CanvasRenderingContext2D, cx: number, cy: number, radius: number): void {
  const pointerY = cy - radius - 5
  ctx.beginPath()
  ctx.moveTo(cx - 14, pointerY + 10)
  ctx.lineTo(cx, pointerY - 8)
  ctx.lineTo(cx + 14, pointerY + 10)
  ctx.closePath()
  ctx.fillStyle = cssVar('--ob-color-text', '#333')
  ctx.fill()
  ctx.strokeStyle = cssVar('--ob-color-bg-container', '#fff')
  ctx.lineWidth = 2
  ctx.stroke()

  ctx.beginPath()
  ctx.arc(cx, pointerY - 8, 5, 0, Math.PI * 2)
  ctx.fillStyle = cssVar('--ob-color-error', '#ff4d4f')
  ctx.fill()
}

interface ItemForm {
  label: string
  weight: number
}

function normalizeItem(item: unknown, index: number): TurntableItem | null {
  if (!item || typeof item !== 'object') return null

  const raw = item as Partial<TurntableItem> & { name?: unknown; title?: unknown }
  const fallbackLabel =
    typeof raw.name === 'string'
      ? raw.name
      : typeof raw.title === 'string'
        ? raw.title
        : ''
  const label = typeof raw.label === 'string' ? raw.label : fallbackLabel
  const weight = Number(raw.weight)

  return {
    id: Number.isFinite(Number(raw.id)) ? Number(raw.id) : Date.now() + index,
    label: label.trim() || `选项 ${index + 1}`,
    weight: Number.isFinite(weight) && weight > 0 ? weight : 1,
    color: typeof raw.color === 'string' && raw.color.trim() ? raw.color : '#1677ff',
    sort_order: Number.isFinite(Number(raw.sort_order)) ? Number(raw.sort_order) : index,
    created_at: typeof raw.created_at === 'string' ? raw.created_at : ''
  }
}

function normalizeItems(value: unknown): TurntableItem[] {
  return Array.isArray(value)
    ? value.map(normalizeItem).filter((item): item is TurntableItem => item !== null)
    : []
}

function isErrorResult(value: unknown): value is { error: string } {
  return !!value && typeof value === 'object' && typeof (value as { error?: unknown }).error === 'string'
}

export default function TurntablePlugin({ config, api }: PluginRenderProps) {
  const [items, setItems] = useState<TurntableItem[]>([])
  const [spinning, setSpinning] = useState(false)
  const [rotation, setRotation] = useState(0)
  const [winner, setWinner] = useState<TurntableItem | null>(null)
  const [modalOpen, setModalOpen] = useState(false)
  const [editingItem, setEditingItem] = useState<TurntableItem | null>(null)
  const [form, setForm] = useState<ItemForm>({ label: '', weight: 1 })
  const [resultVisible, setResultVisible] = useState(false)
  const [noRepeat, setNoRepeat] = useState(true)
  const [spinHistory, setSpinHistory] = useState<string[]>([])

  const canvasRef = useRef<HTMLCanvasElement>(null)
  const rotationRef = useRef(0)
  const animFrameRef = useRef<number>(0)

  const spinDuration = (config.spinDuration as number) || 4

  const loadItems = useCallback(async () => {
    const result = await api.sendToBackend({ type: 'getItems' })
    setItems(normalizeItems(result))
  }, [api])

  useEffect(() => {
    loadItems()
  }, [loadItems])

  const drawCanvas = useCallback((rotAngle: number) => {
    const canvas = canvasRef.current
    if (!canvas || items.length === 0) return

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const dpr = window.devicePixelRatio || 1
    canvas.width = CANVAS_SIZE * dpr
    canvas.height = CANVAS_SIZE * dpr
    ctx.scale(dpr, dpr)

    ctx.clearRect(0, 0, CANVAS_SIZE, CANVAS_SIZE)

    ctx.save()
    ctx.translate(CENTER, CENTER)
    ctx.rotate(rotAngle)
    ctx.translate(-CENTER, -CENTER)

    const totalWeight = items.reduce((s, i) => s + i.weight, 0)
    if (totalWeight <= 0) return

    let currentAngle = -Math.PI / 2
    for (const item of items) {
      const sectorAngle = (item.weight / totalWeight) * Math.PI * 2
      const endAngle = currentAngle + sectorAngle
      drawSector(ctx, CENTER, CENTER, RADIUS, currentAngle, endAngle, item.color || '#1677ff')
      drawText(ctx, CENTER, CENTER, RADIUS, item.label, currentAngle, endAngle)
      currentAngle = endAngle
    }

    ctx.restore()

    ctx.beginPath()
    ctx.arc(CENTER, CENTER, 30, 0, Math.PI * 2)
    ctx.fillStyle = cssVar('--ob-color-bg-container', '#fff')
    ctx.fill()
    ctx.strokeStyle = cssVar('--ob-color-border-secondary', '#ddd')
    ctx.lineWidth = 3
    ctx.stroke()

    drawPointer(ctx, CENTER, CENTER, RADIUS)

    ctx.beginPath()
    ctx.arc(CENTER, CENTER, RADIUS, 0, Math.PI * 2)
    ctx.strokeStyle = cssVar('--ob-color-border-secondary', '#ddd')
    ctx.lineWidth = 4
    ctx.stroke()
  }, [items])

  useEffect(() => {
    drawCanvas(rotation)
  }, [rotation, drawCanvas])

  useEffect(() => {
    const el = document.documentElement
    const observer = new MutationObserver(() => drawCanvas(rotationRef.current))
    observer.observe(el, { attributes: true, attributeFilter: ['style'] })
    return () => observer.disconnect()
  }, [drawCanvas])

  const spin = async () => {
    if (spinning || items.length < 2) return

    setSpinning(true)
    setWinner(null)
    setResultVisible(false)

    const result = await api.sendToBackend({ type: 'spin', payload: { noRepeat } }) as SpinResult | { error: string }

    if (isErrorResult(result)) {
      api.notify('转盘抽奖', result.error as string)
      setSpinning(false)
      return
    }
    if (!result || typeof result !== 'object' || !('winner' in result)) {
      api.notify('转盘抽奖', '抽奖失败：插件后端没有返回有效结果')
      setSpinning(false)
      return
    }

    const spinResult = result as SpinResult
    let targetRotation: number
    try {
      const fullSpins = 5 + Math.floor(secureRandomUnit() * 3)
      targetRotation = targetRotationForWinner(
        items,
        spinResult.winner.id,
        rotationRef.current,
        fullSpins
      )
    } catch (error) {
      api.notify('转盘抽奖', error instanceof Error ? error.message : '中奖选项与转盘不一致')
      setSpinning(false)
      await loadItems()
      return
    }

    const startRotation = rotationRef.current
    const startTime = performance.now()

    const animate = (now: number) => {
      const elapsed = (now - startTime) / 1000
      const progress = Math.min(elapsed / spinDuration, 1)
      const eased = easeOutQuart(progress)
      const currentRotation = startRotation + (targetRotation - startRotation) * eased

      rotationRef.current = currentRotation
      setRotation(currentRotation)

      if (progress < 1) {
        animFrameRef.current = requestAnimationFrame(animate)
      } else {
        rotationRef.current = targetRotation
        setRotation(targetRotation)
        setSpinning(false)
        setWinner(spinResult.winner)
        setResultVisible(true)
        setSpinHistory((history) => [spinResult.winner.label, ...history].slice(0, 8))
      }
    }

    animFrameRef.current = requestAnimationFrame(animate)
  }

  useEffect(() => {
    return () => {
      if (animFrameRef.current) {
        cancelAnimationFrame(animFrameRef.current)
      }
    }
  }, [])

  const openAddModal = () => {
    setEditingItem(null)
    setForm({ label: '', weight: 1 })
    setModalOpen(true)
  }

  const openEditModal = (item: TurntableItem) => {
    setEditingItem(item)
    setForm({ label: item.label, weight: item.weight })
    setModalOpen(true)
  }

  const handleSave = async () => {
    if (!form.label.trim()) return

    if (editingItem) {
      const updated = await api.sendToBackend({
        type: 'updateItem',
        payload: { id: editingItem.id, label: form.label.trim(), weight: form.weight }
      })
      if (isErrorResult(updated)) {
        api.notify('转盘抽奖', updated.error)
        return
      }
      const normalized = normalizeItem(updated, items.length)
      if (normalized) {
        setItems(prev => prev.map(i => i.id === editingItem.id ? normalized : i))
      } else {
        await loadItems()
      }
    } else {
      const added = await api.sendToBackend({
        type: 'addItem',
        payload: { label: form.label.trim(), weight: form.weight, color: '' }
      })
      if (isErrorResult(added)) {
        api.notify('转盘抽奖', added.error)
        return
      }
      const normalized = normalizeItem(added, items.length)
      if (normalized) {
        setItems(prev => [...prev, normalized])
      } else {
        await loadItems()
      }
    }

    setModalOpen(false)
    setEditingItem(null)
  }

  const handleDelete = async (id: number) => {
    await api.sendToBackend({ type: 'deleteItem', payload: { id } })
    setItems(prev => prev.filter(i => i.id !== id))
  }

  const mainBg = { background: 'var(--ob-color-bg-layout, #f5f5f5)', borderRadius: 12, padding: 20 }
  const sectionBg = { background: 'var(--ob-color-bg-container, #fff)', borderRadius: 8, padding: 16 }

  return (
    <div style={{ ...mainBg, minHeight: '100%' }}>
      <h2 style={{ margin: '0 0 16px', fontSize: 20, color: 'var(--ob-color-text, #333)' }}>
        转盘抽奖
      </h2>

      <div style={{ display: 'flex', gap: 20, flexWrap: 'wrap' as const }}>
        <div style={{ flex: '0 0 auto' }}>
          <div style={sectionBg}>
            <canvas
              ref={canvasRef}
              style={{ width: CANVAS_SIZE, height: CANVAS_SIZE, display: 'block' }}
            />
            <button
              onClick={spin}
              disabled={spinning || items.length < 2}
              style={{
                marginTop: 16,
                width: '100%',
                padding: '12px 0',
                fontSize: 18,
                fontWeight: 600,
                border: 'none',
                borderRadius: 8,
                cursor: spinning || items.length < 2 ? 'not-allowed' : 'pointer',
                background: spinning ? 'var(--ob-color-text-tertiary, #ccc)' : 'var(--ob-color-error, #ff4d4f)',
                color: '#fff',
                transition: 'background 0.3s'
              }}
            >
              {spinning ? '旋转中...' : '开始抽奖'}
            </button>
            <label style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 10, fontSize: 13, color: 'var(--ob-color-text-secondary, #666)' }}>
              <input type="checkbox" checked={noRepeat} onChange={(event) => setNoRepeat(event.target.checked)} />
              避免连续抽中同一选项
            </label>

            {resultVisible && winner && (
              <div style={{
                marginTop: 12,
                padding: '12px 16px',
                background: 'var(--ob-color-warning-bg, #fff7e6)',
                border: '1px solid var(--ob-color-warning, #ffd591)',
                borderRadius: 8,
                textAlign: 'center' as const
              }}>
                <div style={{ fontSize: 13, color: 'var(--ob-color-text-secondary, #666)', marginBottom: 4 }}>恭喜中奖</div>
                <div style={{
                  fontSize: 22,
                  fontWeight: 700,
                  color: winner.color
                }}>
                  {winner.label}
                </div>
              </div>
            )}
          </div>
          {spinHistory.length > 0 && (
            <div style={{ ...sectionBg, marginTop: 12 }}>
              <h3 style={{ margin: '0 0 8px', fontSize: 15 }}>最近中奖记录</h3>
              <div style={{ color: 'var(--ob-color-text-secondary, #666)', fontSize: 13 }}>
                {spinHistory.map((label, index) => `${index + 1}. ${label}`).join('　')}
              </div>
            </div>
          )}
        </div>

        <div style={{ flex: 1, minWidth: 320 }}>
          <div style={sectionBg}>
            <div style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 16
            }}>
              <h3 style={{ margin: 0, fontSize: 16, color: 'var(--ob-color-text, #333)' }}>
                选项列表
              </h3>
              <button
                onClick={openAddModal}
                style={{
                  padding: '6px 16px',
                  fontSize: 14,
                  border: 'none',
                  borderRadius: 6,
                  cursor: 'pointer',
                  background: 'var(--ob-color-primary, #1677ff)',
                  color: '#fff'
                }}
              >
                添加选项
              </button>
            </div>

            {items.length === 0 ? (
              <div style={{ padding: '40px 0', textAlign: 'center' as const, color: 'var(--ob-color-text-tertiary, #999)', fontSize: 14 }}>
                暂无选项，点击“添加选项”开始添加
              </div>
            ) : (
              <div style={{ overflowX: 'auto' as const }}>
                <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 14 }}>
                  <thead>
                    <tr style={{ borderBottom: '2px solid var(--ob-color-border-secondary, #f0f0f0)' }}>
                      <th style={{ padding: '8px 12px', textAlign: 'left' as const, color: 'var(--ob-color-text-secondary, #666)', fontWeight: 600, width: 40 }}></th>
                      <th style={{ padding: '8px 12px', textAlign: 'left' as const, color: 'var(--ob-color-text-secondary, #666)', fontWeight: 600 }}>选项名称</th>
                      <th style={{ padding: '8px 12px', textAlign: 'center' as const, color: 'var(--ob-color-text-secondary, #666)', fontWeight: 600, width: 80 }}>权重</th>
                      <th style={{ padding: '8px 12px', textAlign: 'center' as const, color: 'var(--ob-color-text-secondary, #666)', fontWeight: 600, width: 80 }}>概率</th>
                      <th style={{ padding: '8px 12px', textAlign: 'center' as const, color: 'var(--ob-color-text-secondary, #666)', fontWeight: 600, width: 100 }}>操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {items.map(item => {
                      const totalWeight = items.reduce((s, i) => s + i.weight, 0)
                      const pct = totalWeight > 0 ? ((item.weight / totalWeight) * 100).toFixed(1) : '0.0'
                      return (
                        <tr key={item.id} style={{ borderBottom: '1px solid var(--ob-color-border-secondary, #f0f0f0)' }}>
                          <td style={{ padding: '10px 12px', textAlign: 'center' as const }}>
                            <span style={{
                              display: 'inline-block',
                              width: 16,
                              height: 16,
                              borderRadius: 4,
                              background: item.color,
                              verticalAlign: 'middle'
                            }} />
                          </td>
                          <td style={{ padding: '10px 12px', color: 'var(--ob-color-text, #333)' }}>{item.label}</td>
                          <td style={{ padding: '10px 12px', textAlign: 'center' as const, color: 'var(--ob-color-text, #333)' }}>{item.weight}</td>
                          <td style={{ padding: '10px 12px', textAlign: 'center' as const, color: 'var(--ob-color-text-secondary, #666)' }}>{pct}%</td>
                          <td style={{ padding: '10px 12px', textAlign: 'center' as const }}>
                            <button
                              onClick={() => openEditModal(item)}
                              style={{
                                padding: '4px 10px',
                                marginRight: 6,
                                fontSize: 12,
                                border: '1px solid var(--ob-color-border, #d9d9d9)',
                                borderRadius: 4,
                                cursor: 'pointer',
                                background: 'var(--ob-color-bg-container, #fff)',
                                color: 'var(--ob-color-text, #333)'
                              }}
                            >
                              编辑
                            </button>
                            <button
                              onClick={() => handleDelete(item.id)}
                              style={{
                                padding: '4px 10px',
                                fontSize: 12,
                                border: '1px solid var(--ob-color-error, #ff4d4f)',
                                borderRadius: 4,
                                cursor: 'pointer',
                                background: 'var(--ob-color-bg-container, #fff)',
                                color: 'var(--ob-color-error, #ff4d4f)'
                              }}
                            >
                              删除
                            </button>
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
            )}

            {items.length > 0 && (
              <div style={{ marginTop: 12, padding: '8px 12px', background: 'var(--ob-color-success-bg, #f6ffed)', borderRadius: 6, fontSize: 13, color: 'var(--ob-color-success, #52c41a)' }}>
                共 {items.length} 个选项，合计权重 {items.reduce((s, i) => s + i.weight, 0)}
              </div>
            )}
          </div>
        </div>
      </div>

      {modalOpen && (
        <div style={{
          position: 'fixed' as const,
          top: 0, left: 0, right: 0, bottom: 0,
          background: 'rgba(0,0,0,0.45)',
          zIndex: 1000,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center'
        }}
          onClick={() => setModalOpen(false)}
        >
          <div style={{
            background: 'var(--ob-color-bg-container, #fff)',
            borderRadius: 12,
            padding: 24,
            width: 400,
            maxWidth: '90vw',
            boxShadow: '0 6px 30px rgba(0,0,0,0.15)'
          }}
            onClick={e => e.stopPropagation()}
          >
            <h3 style={{ margin: '0 0 20px', fontSize: 18, color: 'var(--ob-color-text, #333)' }}>
              {editingItem ? '编辑选项' : '添加选项'}
            </h3>

            <div style={{ marginBottom: 16 }}>
              <label style={{ display: 'block', marginBottom: 6, fontSize: 14, color: 'var(--ob-color-text, #333)', fontWeight: 500 }}>
                选项名称
              </label>
              <input
                type="text"
                value={form.label}
                onChange={e => setForm(f => ({ ...f, label: e.target.value }))}
                placeholder="请输入选项名称"
                style={{
                  width: '100%',
                  padding: '8px 12px',
                  fontSize: 14,
                  border: '1px solid var(--ob-color-border, #d9d9d9)',
                  borderRadius: 6,
                  outline: 'none',
                  boxSizing: 'border-box'
                }}
              />
            </div>

            <div style={{ marginBottom: 20 }}>
              <label style={{ display: 'block', marginBottom: 6, fontSize: 14, color: 'var(--ob-color-text, #333)', fontWeight: 500 }}>
                权重（数值越大，概率越高）
              </label>
              <input
                type="number"
                value={form.weight}
                min={0.1}
                step={0.1}
                onChange={e => setForm(f => ({ ...f, weight: Math.max(0.1, parseFloat(e.target.value) || 1) }))}
                style={{
                  width: '100%',
                  padding: '8px 12px',
                  fontSize: 14,
                  border: '1px solid var(--ob-color-border, #d9d9d9)',
                  borderRadius: 6,
                  outline: 'none',
                  boxSizing: 'border-box'
                }}
              />
            </div>

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10 }}>
              <button
                onClick={() => setModalOpen(false)}
                style={{
                  padding: '8px 20px',
                  fontSize: 14,
                  border: '1px solid var(--ob-color-border, #d9d9d9)',
                  borderRadius: 6,
                  cursor: 'pointer',
                  background: 'var(--ob-color-bg-container, #fff)',
                  color: 'var(--ob-color-text, #333)'
                }}
              >
                取消
              </button>
              <button
                onClick={handleSave}
                disabled={!form.label.trim()}
                style={{
                  padding: '8px 20px',
                  fontSize: 14,
                  border: 'none',
                  borderRadius: 6,
                  cursor: form.label.trim() ? 'pointer' : 'not-allowed',
                  background: form.label.trim() ? 'var(--ob-color-primary, #1677ff)' : 'var(--ob-color-text-tertiary, #ccc)',
                  color: '#fff'
                }}
              >
                {editingItem ? '保存' : '添加'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

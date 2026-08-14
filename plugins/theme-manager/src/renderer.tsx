import { useEffect, useState } from 'react'
import type { PluginRenderProps, Theme } from 'openbox-plugin-api'
import { themeColorVar } from '@openbox/ui'

const TONE = themeColorVar

const CUSTOM_KEYS: { key: keyof typeof DEFAULT_CUSTOM; label: string }[] = [
  { key: 'primary', label: '主色' },
  { key: 'bg', label: '页面背景' },
  { key: 'container', label: '面板背景' },
  { key: 'text', label: '文字' },
  { key: 'border', label: '边框' }
]

const DEFAULT_CUSTOM = {
  primary: '#1677ff',
  bg: '#f5f5f5',
  container: '#ffffff',
  text: '#333333',
  border: '#e8e8e8'
}

const FALLBACK_TOKENS: Record<string, string> = {
  colorPrimary: '#6366f1',
  colorPrimaryHover: '#818cf8',
  colorPrimaryBg: '#eef2ff',
  colorBg: '#f5f6fa',
  colorBgLayout: '#f5f6fa',
  colorBgContainer: '#ffffff',
  colorBgElevated: '#ffffff',
  colorText: '#333333',
  colorTextSecondary: '#888888',
  colorTextTertiary: '#c0c0c0',
  colorBorder: '#e8e8e8',
  colorBorderSecondary: '#f0f0f0',
  colorSuccess: '#52c41a',
  colorSuccessBg: '#f6ffed',
  colorWarning: '#faad14',
  colorWarningBg: '#fffbe6',
  colorError: '#ff4d4f',
  colorErrorBg: '#fff2f0',
  colorLink: '#6366f1'
}

export { FALLBACK_TOKENS, DEFAULT_CUSTOM }

function readToken(theme: Theme | undefined, key: string): string {
  const value = theme?.tokens?.[key]
  if (typeof value === 'string' && value && !value.startsWith('var-')) {
    return value
  }
  return FALLBACK_TOKENS[key] || '#000000'
}

function buildCustomTheme(
  theme: Theme | undefined,
  mode: 'light' | 'dark',
  colors: typeof DEFAULT_CUSTOM,
  id = 'custom',
  name = '自定义'
): Theme {
  const base: Record<string, string | number> = { ...FALLBACK_TOKENS }
  if (theme?.tokens) {
    for (const [key, value] of Object.entries(theme.tokens)) {
      base[key] = value
    }
  }
  return {
    id,
    name,
    mode,
    tokens: {
      ...base,
      colorPrimary: colors.primary,
      colorPrimaryHover: colors.primary,
      colorPrimaryBg: `${colors.primary}1a`,
      colorBg: mode === 'dark' ? '#141414' : '#f5f5f5',
      colorBgLayout: colors.bg,
      colorBgContainer: colors.container,
      colorText: colors.text,
      colorBorder: colors.border,
      colorLink: colors.primary,
      colorSuccess: colors.primary
    }
  }
}

function isValidTheme(value: unknown): value is Theme {
  const t = value as Theme | null
  return (
    !!t &&
    typeof t === 'object' &&
    typeof t.id === 'string' &&
    typeof t.name === 'string' &&
    (t.mode === 'light' || t.mode === 'dark') &&
    !!t.tokens &&
    typeof t.tokens === 'object'
  )
}

export { buildCustomTheme, isValidTheme }

const inputStyle: Record<string, string | number> = {
  width: '100%',
  padding: '8px 10px',
  border: `1px solid ${TONE('border', '#d9d9d9')}`,
  borderRadius: 6,
  fontSize: 14,
  background: TONE('bg-container', '#fff'),
  color: TONE('text', '#333'),
  boxSizing: 'border-box'
}

const primaryBtnStyle: Record<string, string | number> = {
  padding: '8px 16px',
  border: 'none',
  borderRadius: 6,
  cursor: 'pointer',
  background: TONE('primary', '#555'),
  color: '#fff',
  fontSize: 14
}

export default function ThemeManager({ theme, api, config, onConfigChange }: PluginRenderProps) {
  const [mode, setMode] = useState<'light' | 'dark'>(theme?.mode || 'light')
  const [colors, setColors] = useState<typeof DEFAULT_CUSTOM>({
    primary: readToken(theme, 'colorPrimary'),
    bg: readToken(theme, 'colorBgLayout'),
    container: readToken(theme, 'colorBgContainer'),
    text: readToken(theme, 'colorText'),
    border: readToken(theme, 'colorBorder')
  })
  const [savedCustoms, setSavedCustoms] = useState<Theme[]>(() => {
    try {
      const raw = String(config?.customThemes ?? '[]')
      const parsed = JSON.parse(raw) as unknown
      return Array.isArray(parsed) ? (parsed.filter(isValidTheme) as Theme[]) : []
    } catch {
      return []
    }
  })
  const [presets, setPresets] = useState<Theme[]>([])
  const [previewing, setPreviewing] = useState(false)
  const [msg, setMsg] = useState('')

  useEffect(() => {
    let active = true
    void api.theme
      .list()
      .then((themes) => {
        if (active) setPresets(themes)
      })
      .catch(() => {
        if (active) setMsg('无法读取内置主题列表')
      })
    return () => {
      active = false
      void api.theme.rollback().catch(() => undefined)
    }
  }, [api.theme])

  const persistCustoms = (next: Theme[]) => {
    setSavedCustoms(next)
    onConfigChange?.({ ...(config ?? {}), customThemes: JSON.stringify(next) })
  }

  const previewTheme = async (next: Theme) => {
    const ok = await api.theme.preview(next)
    setPreviewing((current) => ok || current)
    setMsg(ok ? `正在预览主题「${next.name}」` : '预览失败：缺少主题修改权限')
    return ok
  }

  const keepPreview = async () => {
    await api.theme.commit()
    setPreviewing(false)
    setMsg('已保留当前主题')
  }

  const rollbackPreview = async () => {
    const restored = await api.theme.rollback()
    setPreviewing((current) => !restored && current)
    setMsg(restored ? '已恢复预览前的主题' : '没有可恢复的主题预览')
  }

  const applyCustom = async () => {
    const next = buildCustomTheme(
      theme,
      mode,
      colors,
      `custom-${Date.now()}`,
      `自定义 ${savedCustoms.length + 1}`
    )
    const ok = await previewTheme(next)
    if (ok) {
      persistCustoms([...savedCustoms, next])
      setMsg(`已保存并正在预览自定义主题「${next.name}」`)
    }
  }

  const deleteCustom = (id: string) => {
    persistCustoms(savedCustoms.filter((t) => t.id !== id))
  }

  const exportTheme = async () => {
    const current = await api.theme.get()
    if (!current) {
      setMsg('暂无可导出的主题')
      return
    }
    const blob = new Blob([JSON.stringify(current, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `openbox-theme-${current.id}.json`
    a.click()
    URL.revokeObjectURL(url)
    setMsg('主题已导出')
  }

  const importTheme = async (file: File | undefined) => {
    if (!file) return
    const text = await file.text()
    try {
      const parsed = JSON.parse(text) as unknown
      if (!isValidTheme(parsed)) {
        setMsg('导入失败：主题文件格式无效')
        return
      }
      const ok = await previewTheme(parsed)
      setMsg(ok ? `已导入并正在预览主题「${parsed.name}」` : '导入失败：缺少主题修改权限')
    } catch {
      setMsg('导入失败：无法解析文件')
    }
  }

  return (
    <div
      style={{
        background: TONE('bg-layout', '#f5f5f5'),
        borderRadius: 'var(--ob-radius, 8px)',
        padding: 20,
        minHeight: '100%'
      }}
    >
      <h2 style={{ margin: '0 0 6px', fontSize: 20, color: TONE('text', '#333') }}>主题管理</h2>
      <p style={{ margin: '0 0 16px', fontSize: 13, color: TONE('text-secondary', '#888') }}>
        当前主题：{theme?.name || 'unknown'}（{theme?.mode || '-'}）
      </p>

      {previewing && (
        <div
          role="status"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            marginBottom: 16,
            padding: 12,
            border: `1px solid ${TONE('primary', '#555')}`,
            borderRadius: 8,
            background: TONE('primary-bg', '#eef2ff')
          }}
        >
          <span style={{ flex: 1, color: TONE('text', '#333'), fontSize: 13 }}>
            当前更改处于预览状态；关闭插件会自动恢复。
          </span>
          <button style={primaryBtnStyle} onClick={() => void keepPreview()}>
            保留
          </button>
          <button
            style={{ ...primaryBtnStyle, background: 'transparent', color: TONE('text', '#333') }}
            onClick={() => void rollbackPreview()}
          >
            撤销
          </button>
        </div>
      )}

      <div style={{ marginBottom: 20 }}>
        <h3 style={{ margin: '0 0 10px', fontSize: 15, color: TONE('text', '#333') }}>内置主题</h3>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(140px, 1fr))',
            gap: 10
          }}
        >
          {savedCustoms.map((preset) => {
            const active = theme?.id === preset.id
            const pTokens = preset.tokens as Record<string, string | number>
            return (
              <div
                key={preset.id}
                onClick={() => void previewTheme(preset)}
                style={{
                  cursor: 'pointer',
                  position: 'relative',
                  border: `1px solid ${active ? TONE('primary', '#555') : TONE('border', '#e8e8e8')}`,
                  borderRadius: 8,
                  padding: 12,
                  background: String(pTokens.colorBgContainer ?? '#fff'),
                  boxShadow: active ? `0 0 0 2px ${TONE('primary', '#555')}` : undefined
                }}
              >
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    deleteCustom(preset.id)
                  }}
                  title="删除该自定义主题"
                  style={{
                    position: 'absolute',
                    top: 4,
                    right: 4,
                    width: 20,
                    height: 20,
                    lineHeight: '18px',
                    textAlign: 'center',
                    border: 'none',
                    borderRadius: 4,
                    cursor: 'pointer',
                    background: 'transparent',
                    color: TONE('text-secondary', '#888'),
                    fontSize: 13
                  }}
                >
                  ×
                </button>
                <div style={{ display: 'flex', gap: 5, marginBottom: 8 }}>
                  {['colorPrimary', 'colorBgContainer', 'colorBg', 'colorText'].map((k) => (
                    <span
                      key={k}
                      style={{
                        width: 16,
                        height: 16,
                        borderRadius: 4,
                        display: 'inline-block',
                        background: String(pTokens[k] ?? '#ccc')
                      }}
                    />
                  ))}
                </div>
                <div
                  style={{
                    fontSize: 13,
                    fontWeight: 600,
                    color: String(pTokens.colorText ?? '#333')
                  }}
                >
                  {preset.name}
                </div>
              </div>
            )
          })}
          {presets.map((preset) => {
            const active = theme?.id === preset.id
            const isCyber = preset.id === 'cyber'
            return (
              <div
                key={preset.id}
                onClick={() => void previewTheme(preset)}
                style={{
                  cursor: 'pointer',
                  position: 'relative',
                  border: `1px solid ${active ? TONE('primary', '#555') : TONE('border', '#e8e8e8')}`,
                  borderRadius: 8,
                  padding: 12,
                  background: String(preset.tokens.colorBgContainer),
                  boxShadow: active
                    ? `0 0 0 2px ${TONE('primary', '#555')}`
                    : isCyber
                      ? '0 0 10px rgba(0,229,255,0.25)'
                      : '0 1px 3px rgba(0,0,0,0.08)',
                  overflow: 'hidden'
                }}
              >
                {isCyber && (
                  <span
                    style={{
                      position: 'absolute',
                      top: 0,
                      right: 0,
                      width: 22,
                      height: 22,
                      background: 'linear-gradient(135deg, #00e5ff, #ff003c)',
                      clipPath: 'polygon(100% 0, 0 0, 100% 100%)',
                      opacity: 0.85
                    }}
                  />
                )}
                <div style={{ display: 'flex', gap: 5, marginBottom: 8 }}>
                  {['colorPrimary', 'colorBgContainer', 'colorBg', 'colorText'].map((k) => (
                    <span
                      key={k}
                      style={{
                        width: 16,
                        height: 16,
                        borderRadius: 4,
                        display: 'inline-block',
                        background: String(preset.tokens[k])
                      }}
                    />
                  ))}
                </div>
                <div
                  style={{ fontSize: 13, fontWeight: 600, color: String(preset.tokens.colorText) }}
                >
                  {preset.name}
                </div>
              </div>
            )
          })}
        </div>
      </div>

      <div
        style={{
          background: TONE('bg-container', '#fff'),
          border: `1px solid ${TONE('border', '#e8e8e8')}`,
          borderRadius: 8,
          padding: 16,
          marginBottom: 20
        }}
      >
        <h3 style={{ margin: '0 0 12px', fontSize: 15, color: TONE('text', '#333') }}>
          自定义主题
        </h3>
        <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
          {(['light', 'dark'] as const).map((m) => (
            <button
              key={m}
              onClick={() => setMode(m)}
              style={{
                padding: '6px 14px',
                border: `1px solid ${mode === m ? TONE('primary', '#555') : TONE('border', '#d9d9d9')}`,
                borderRadius: 6,
                cursor: 'pointer',
                background: mode === m ? TONE('primary', '#555') : 'transparent',
                color: mode === m ? '#fff' : TONE('text', '#333'),
                fontSize: 13
              }}
            >
              {m === 'light' ? '亮色' : '深色'}
            </button>
          ))}
        </div>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fit, minmax(120px, 1fr))',
            gap: 10,
            marginBottom: 14
          }}
        >
          {CUSTOM_KEYS.map(({ key, label }) => (
            <label
              key={key}
              style={{ display: 'block', fontSize: 13, color: TONE('text-secondary', '#888') }}
            >
              {label}
              <input
                type="color"
                value={colors[key]}
                onChange={(e) => setColors((c) => ({ ...c, [key]: e.target.value }))}
                style={{
                  display: 'block',
                  width: '100%',
                  height: 34,
                  border: 'none',
                  borderRadius: 6,
                  background: 'transparent',
                  cursor: 'pointer'
                }}
              />
            </label>
          ))}
        </div>
        <div style={{ display: 'flex', gap: 10 }}>
          <button style={primaryBtnStyle} onClick={applyCustom}>
            应用自定义主题
          </button>
          <button
            style={{
              ...inputStyle,
              width: 'auto',
              padding: '8px 16px',
              background: 'transparent',
              cursor: 'pointer'
            }}
            onClick={() => {
              setMode('light')
              setColors(DEFAULT_CUSTOM)
            }}
          >
            重置
          </button>
        </div>
      </div>

      <div
        style={{
          background: TONE('bg-container', '#fff'),
          border: `1px solid ${TONE('border', '#e8e8e8')}`,
          borderRadius: 8,
          padding: 16
        }}
      >
        <h3 style={{ margin: '0 0 12px', fontSize: 15, color: TONE('text', '#333') }}>
          备份 / 恢复
        </h3>
        <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', alignItems: 'center' }}>
          <button style={primaryBtnStyle} onClick={exportTheme}>
            导出当前主题
          </button>
          <label
            style={{
              cursor: 'pointer',
              padding: '8px 16px',
              border: `1px solid ${TONE('border', '#d9d9d9')}`,
              borderRadius: 6,
              color: TONE('text', '#333'),
              fontSize: 14
            }}
          >
            导入 JSON 主题
            <input
              type="file"
              accept=".json,application/json"
              onChange={(e) => importTheme(e.target.files?.[0])}
              style={{ display: 'none' }}
            />
          </label>
        </div>
      </div>

      {msg && (
        <p style={{ margin: '16px 0 0', fontSize: 13, color: TONE('text-secondary', '#888') }}>
          {msg}
        </p>
      )}
    </div>
  )
}

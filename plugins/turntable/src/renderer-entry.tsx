import React from 'react'
import { createRoot } from 'react-dom/client'
import { createClient, type RendererContext } from '@cruciblebox/next-api'
import type { DecisionRenderProps } from './next-service'
import { createDecisionStorage } from './next-service'
import decision from './main'
import TurntablePlugin from './renderer'
import DicePanel from './DicePanel'

function RandomDecisionPlugin(props: DecisionRenderProps) {
  const [mode, setMode] = React.useState<'turntable' | 'dice'>('turntable')
  return (
    <div style={{ minHeight: '100%', background: 'var(--ob-color-bg-layout, #f5f5f5)' }}>
      <nav style={{ display: 'flex', gap: 8, padding: '16px 20px 0' }}>
        <button onClick={() => setMode('turntable')} aria-pressed={mode === 'turntable'}>
          转盘抽取
        </button>
        <button onClick={() => setMode('dice')} aria-pressed={mode === 'dice'}>
          骰子投掷
        </button>
      </nav>
      {mode === 'turntable' ? <TurntablePlugin {...props} /> : <DicePanel />}
    </div>
  )
}

export async function mount(context: RendererContext) {
  const client = createClient(context)
  const config = await client.config.get()
  if (!config || typeof config !== 'object' || Array.isArray(config))
    throw Error('INVALID_RESPONSE')
  let active = true
  decision.activate({ storage: createDecisionStorage(client), logger: console })
  const api = {
    execute: async (message: unknown) => {
      if (!active) throw Error('SESSION_DENIED')
      return decision.onMessage(message)
    },
    notify: client.notify
  }
  const root = createRoot(context.root)
  root.render(React.createElement(RandomDecisionPlugin, { api, config }))
  return () => {
    active = false
    root.unmount()
    decision.deactivate()
  }
}

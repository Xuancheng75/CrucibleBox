import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const homeSource = readFileSync(resolve(import.meta.dirname, '../src/pages/Home.tsx'), 'utf8')

describe('Home plugin load error resilience', () => {
  it('does not short-circuit the whole page when plugin loading fails', () => {
    expect(homeSource).not.toMatch(/if\s*\(\s*error\s*\)\s*\{\s*return/i)
  })

  it('announces the error below the search bar for assistive tech', () => {
    expect(homeSource).toContain('role="alert"')
    expect(homeSource).toContain('aria-live="assertive"')
    expect(homeSource).toContain('aria-atomic="true"')
  })

  it('keeps refresh and retry actions accessible and re-clickable', () => {
    expect(homeSource).toContain('aria-label="刷新插件列表"')
    expect(homeSource).toContain('aria-label="重试加载插件"')
    expect(homeSource).toMatch(/loading=\{refreshing\}/)
  })

  it('decides the refresh success toast from the actual request result, not a stale closure', () => {
    expect(homeSource).toContain('import { usePluginStore }')
    expect(homeSource).toContain('usePluginStore.getState().error')
    expect(homeSource).toMatch(/finally\s*\{\s*setRefreshing\(false\)\s*\}/)
    expect(homeSource).not.toMatch(/await fetchPlugins\(\)\s*\n\s*message\.success/)
  })
})

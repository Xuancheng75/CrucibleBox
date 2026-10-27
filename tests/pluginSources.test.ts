import { existsSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import pkg from '../package.json'
import lock from '../package-lock.json'
describe('host and public SDK source boundary', () => {
  it('builds public workspaces without a business plugin source tree', () => {
    expect('workspaces' in pkg).toBe(false)
    expect(existsSync(resolve('plugins'))).toBe(false)
    expect(Object.keys(lock.packages).some((path) => path.startsWith('plugins/'))).toBe(false)
  })
  it('preserves MIT licenses in the public SDKs', () => {
    for (const name of [
      'cruciblebox-next2-api',
      'cruciblebox-next2-ui',
      'cruciblebox-next2-build',
      'cruciblebox-next2-native'
    ])
      expect(readFileSync(resolve('packages', name, 'LICENSE'), 'utf8')).toContain('MIT License')
  })
})

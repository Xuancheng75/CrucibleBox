import { readFileSync } from 'node:fs'
import { expect, test } from 'vitest'
import { validateManifest, validateRequest } from '../vendor/next-api/src/index.mjs'
import { buildCustomTheme, DEFAULT_CUSTOM } from '../src/renderer'
test('Next theme package is renderer-only with explicit config/theme/download capabilities', () => {
  const manifest = validateManifest(
    readFileSync(new URL('../plugin.json', import.meta.url), 'utf8')
  )
  expect(manifest.id).toBe('theme-manager')
  expect(manifest.backend).toBeUndefined()
  expect(manifest.permissions).toEqual([
    'theme:read',
    'theme:write',
    'storage:read',
    'storage:write',
    'browser:downloads'
  ])
  expect(manifest.config?.customThemes).toMatchObject({ default: '[]' })
  const theme = buildCustomTheme(undefined, 'light', DEFAULT_CUSTOM)
  expect(() =>
    validateRequest(
      JSON.stringify({
        wireVersion: 3,
        requestId: 'test',
        session: 'a'.repeat(64),
        method: 'theme.preview',
        params: { theme }
      })
    )
  ).not.toThrow()
})

import test from 'node:test'
import assert from 'node:assert/strict'
import { validateRequest, validateThemeDesign } from '../src/index.mjs'
test('new asset RPCs remain bounded and reject unknown fields and invalid IDs', () => {
  const request = {
    wireVersion: 5,
    requestId: 'theme-1',
    session: 'a'.repeat(64),
    target: { kind: 'platform' },
    method: 'theme.assets.list',
    params: { limit: 16 }
  }
  assert.doesNotThrow(() => validateRequest(JSON.stringify(request)))
  for (const params of [{ limit: 17 }, { limit: 1, delete: true }])
    assert.throws(() => validateRequest(JSON.stringify({ ...request, params })))
  assert.throws(() =>
    validateRequest(
      JSON.stringify({ ...request, method: 'theme.assets.get', params: { id: '../private' } })
    )
  )
})
test('font and image references require declared assets; duplicate design IDs fail', () => {
  const id = '1'.repeat(64)
  assert.throws(() =>
    validateThemeDesign(JSON.stringify({ version: 2, fonts: [{ assetId: id, family: 'Sample' }] }))
  )
  assert.doesNotThrow(() =>
    validateThemeDesign(
      JSON.stringify({ version: 2, assets: [id], fonts: [{ assetId: id, family: 'Sample' }] })
    )
  )
  assert.throws(() =>
    validateThemeDesign(
      JSON.stringify({
        version: 2,
        assets: [id],
        fonts: [
          { assetId: id, family: 'Sample' },
          { assetId: id, family: 'sample' }
        ]
      })
    )
  )
})

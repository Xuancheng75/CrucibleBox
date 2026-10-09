import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { validateAppearance } from '../src/index.mjs'
const fixtures = JSON.parse(
  readFileSync(new URL('../../../contracts/next/appearance-fixtures.json', import.meta.url), 'utf8')
)
for (const f of fixtures)
  test(f.name, () => {
    const raw = JSON.stringify(f.appearance)
    if (f.valid) assert.deepEqual(validateAppearance(raw), f.appearance)
    else assert.throws(() => validateAppearance(raw))
  })

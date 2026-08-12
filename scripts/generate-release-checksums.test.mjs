import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { generateReleaseChecksums } from './generate-release-checksums.mjs'

test('writes deterministic checksums for nested release artifacts', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'openbox-release-checksums-'))
  try {
    mkdirSync(join(directory, 'linux'))
    writeFileSync(join(directory, 'windows.exe'), 'windows')
    writeFileSync(join(directory, 'linux', 'openbox.AppImage'), 'linux')

    const first = await generateReleaseChecksums(directory)
    const firstContent = readFileSync(first.outputPath, 'utf8')
    const second = await generateReleaseChecksums(directory)

    assert.equal(first.artifactCount, 2)
    assert.equal(readFileSync(second.outputPath, 'utf8'), firstContent)
    assert.match(
      firstContent,
      /^[a-f0-9]{64}  linux\/openbox\.AppImage\n[a-f0-9]{64}  windows\.exe\n$/u
    )
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})

test('rejects symlinks instead of hashing content outside the artifact tree', async (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'openbox-release-checksums-'))
  try {
    const external = join(directory, '..', 'openbox-external-artifact')
    writeFileSync(external, 'outside')
    try {
      symlinkSync(external, join(directory, 'linked-artifact'))
    } catch (error) {
      context.skip(`symlink creation is unavailable: ${error.message}`)
      return
    }
    await assert.rejects(generateReleaseChecksums(directory), /cannot contain symlinks/u)
    rmSync(external, { force: true })
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})

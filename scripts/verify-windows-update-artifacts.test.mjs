import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { verifyWindowsUpdateArtifacts } from './verify-windows-update-artifacts.mjs'

function fixture(overrides = {}) {
  const root = mkdtempSync(join(tmpdir(), 'openbox-windows-update-'))
  const installerName = 'CrucibleBox-1.5.23-windows-x64-setup.exe'
  const installer = Buffer.from('verified-installer')
  writeFileSync(join(root, installerName), installer)
  writeFileSync(join(root, `${installerName}.blockmap`), 'blockmap')
  const sha512 = createHash('sha512').update(installer).digest('base64')
  writeFileSync(
    join(root, 'latest.yml'),
    [
      `version: ${overrides.version ?? '1.5.23'}`,
      `path: ${overrides.path ?? installerName}`,
      `sha512: ${overrides.sha512 ?? sha512}`
    ].join('\n')
  )
  return { installerName, root }
}

test('accepts a complete Windows NSIS update payload', () => {
  const { installerName, root } = fixture()
  try {
    assert.deepEqual(verifyWindowsUpdateArtifacts(root, 'latest.yml', '1.5.23'), {
      installerName,
      metadataName: 'latest.yml',
      version: '1.5.23'
    })
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects a tampered installer before publication', () => {
  const { root } = fixture({ sha512: Buffer.alloc(64).toString('base64') })
  try {
    assert.throws(() => verifyWindowsUpdateArtifacts(root, 'latest.yml', '1.5.23'), /SHA-512/u)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects cross-version and path traversal metadata', () => {
  const versionFixture = fixture({ version: '1.5.24' })
  try {
    assert.throws(
      () => verifyWindowsUpdateArtifacts(versionFixture.root, 'latest.yml', '1.5.23'),
      /does not match/u
    )
  } finally {
    rmSync(versionFixture.root, { recursive: true, force: true })
  }

  const pathFixture = fixture({ path: '../outside-setup.exe' })
  try {
    assert.throws(
      () => verifyWindowsUpdateArtifacts(pathFixture.root, 'latest.yml', '1.5.23'),
      /release root/u
    )
  } finally {
    rmSync(pathFixture.root, { recursive: true, force: true })
  }
})

test('requires the blockmap used for differential downloads', () => {
  const { root } = fixture()
  rmSync(join(root, 'CrucibleBox-1.5.23-windows-x64-setup.exe.blockmap'))
  try {
    assert.throws(() => verifyWindowsUpdateArtifacts(root, 'latest.yml', '1.5.23'), /ENOENT/u)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('rejects a directory masquerading as metadata', () => {
  const root = mkdtempSync(join(tmpdir(), 'openbox-windows-update-'))
  mkdirSync(join(root, 'latest.yml'))
  try {
    assert.throws(() => verifyWindowsUpdateArtifacts(root, 'latest.yml', '1.5.23'), /regular file/u)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

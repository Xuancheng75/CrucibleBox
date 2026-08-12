import assert from 'node:assert/strict'
import { test } from 'node:test'
import { packagedExecutableCandidates, resolvePackagedExecutable } from './packaged-paths.mjs'

const root = process.cwd()
const portable = (path) => path.replaceAll('\\', '/')

test('builds the deterministic Windows x64 executable candidate', () => {
  assert.match(
    portable(packagedExecutableCandidates(root, 'win32', 'x64')[0]),
    /win-unpacked\/CrucibleBox\.exe$/u
  )
})

test('selects the Windows executable and reports the attempted path', () => {
  const candidates = packagedExecutableCandidates(root, 'win32', 'x64')
  assert.equal(
    resolvePackagedExecutable(root, 'win32', 'x64', (path) => path === candidates[0]),
    candidates[0]
  )
  assert.throws(
    () => resolvePackagedExecutable(root, 'win32', 'x64', () => false),
    (error) => candidates.every((candidate) => String(error).includes(candidate))
  )
})

test('rejects unsupported platforms and architectures before probing the filesystem', () => {
  assert.throws(() => packagedExecutableCandidates(root, 'linux', 'x64'), /only Windows x64/u)
  assert.throws(() => packagedExecutableCandidates(root, 'win32', 'arm64'), /only Windows x64/u)
})

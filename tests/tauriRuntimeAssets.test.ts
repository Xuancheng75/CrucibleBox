import { afterEach, describe, expect, it } from 'vitest'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
const roots: string[] = []
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})
function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'cb-assets-'))
  roots.push(root)
  mkdirSync(join(root, '7zip'))
  for (const file of [
    'cruciblebox-plugin-host.exe',
    '7zip/7za.exe',
    '7zip/7za.dll',
    '7zip/License.txt'
  ])
    writeFileSync(join(root, file), 'fixture')
  return root
}
function verify(root: string, profile?: string) {
  return spawnSync(
    process.execPath,
    [
      resolve('scripts/verify-tauri-runtime-assets.mjs'),
      ...(profile ? ['--profile=' + profile] : []),
      root
    ],
    { encoding: 'utf8' }
  )
}
describe('Tauri resource profiles', () => {
  it('base assets succeed without document or OCR runtimes and full requires OCR', () => {
    const root = fixture()
    expect(verify(root, 'base').status).toBe(0)
    expect(verify(root).status).not.toBe(0)
    expect(verify(root, 'full').stderr).toContain('ocr-worker.exe')
    expect(verify(root, 'unknown').status).not.toBe(0)
    rmSync(join(root, 'cruciblebox-plugin-host.exe'))
    expect(verify(root, 'base').stderr).toContain('cruciblebox-plugin-host.exe')
  })
  it('base overlay deletes only optional OCR resources and keeps the plugin sidecar', () => {
    const base = JSON.parse(readFileSync(resolve('src-tauri/tauri.base.conf.json'), 'utf8'))
    expect(base.bundle.externalBin).toEqual(['binaries/cruciblebox-plugin-host'])
    expect(base.bundle.resources).toEqual({
      'binaries/onnxruntime.dll': null,
      'binaries/onnxruntime_providers_shared.dll': null,
      'resources/formula-ocr': null
    })
    const full = JSON.parse(readFileSync(resolve('src-tauri/tauri.conf.json'), 'utf8'))
    expect(full.bundle.externalBin).toContain('binaries/ocr-worker')
    expect(full.bundle.resources['resources/formula-ocr']).toBe('formula-ocr')
  })
})

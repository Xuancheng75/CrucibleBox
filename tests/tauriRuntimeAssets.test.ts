import { afterEach, describe, expect, it } from 'vitest'
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs'
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
  for (const file of ['cruciblebox-plugin-host.exe']) writeFileSync(join(root, file), 'fixture')
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
  it('host needs only its generic runner and rejects packaged business runtimes', () => {
    const root = fixture()
    expect(verify(root, 'base').status).toBe(0)
    expect(verify(root).status).toBe(0)
    writeFileSync(join(root, 'unexpected-runtime.exe'), 'fixture')
    expect(verify(root, 'full').stderr).toContain('unexpected runtime')
    rmSync(join(root, 'unexpected-runtime.exe'))
    expect(verify(root, 'unknown').status).not.toBe(0)
    rmSync(join(root, 'cruciblebox-plugin-host.exe'))
    expect(verify(root, 'base').stderr).toContain('cruciblebox-plugin-host.exe')
  })
  it('both configurations exclude business runtimes and keep the generic runner', () => {
    const base = JSON.parse(readFileSync(resolve('src-tauri/tauri.base.conf.json'), 'utf8'))
    expect(base.bundle.externalBin).toEqual(['binaries/cruciblebox-plugin-host'])
    expect(base.bundle.resources).toBeUndefined()
    const full = JSON.parse(readFileSync(resolve('src-tauri/tauri.conf.json'), 'utf8'))
    expect(full.bundle.externalBin).toEqual(['binaries/cruciblebox-plugin-host'])
    expect(full.bundle.resources).toBeUndefined()
  })
})
